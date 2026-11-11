mod support;
use std::{fs, process::Command};
use support::Case;

fn fmt(case: &Case, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .arg("fmt")
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn stdout_check_and_write_share_the_same_format_and_do_not_compile() {
    let input = "//! file\n/// returns its argument\nfn f(x:Int)->Int{return x;}wt f(7);";
    let case = Case::new(input);
    let output = fmt(&case, &["main.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());
    assert_eq!(fs::read_to_string(case.0.join("main.rusk")).unwrap(), input);
    let formatted = String::from_utf8(output.stdout).unwrap();
    assert!(
        formatted.contains("fn f(x: Int) -> Int {\n    return x;\n}"),
        "{formatted}"
    );
    let output = fmt(&case, &["--check", "main.rusk"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("formatting differs"));
    let output = fmt(&case, &["--write", "main.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(case.0.join("main.rusk")).unwrap(),
        formatted
    );
    assert!(fmt(&case, &["main.rusk", "--check"]).status.success());
    assert_eq!(fmt(&case, &["main.rusk"]).stdout, formatted.as_bytes());
    assert!(!case.0.join("a.cpp").exists());
    assert_eq!(case.run("").stdout, b"7\n");
}

#[test]
fn all_inputs_are_parsed_before_any_file_is_changed() {
    let case = Case::new("wt 7;");
    fs::write(case.0.join("bad.rusk"), "let := ;").unwrap();
    for options in [
        vec!["--write", "main.rusk", "bad.rusk"],
        vec!["--check", "main.rusk", "bad.rusk"],
        vec!["--write", "main.rusk", "missing.rusk"],
    ] {
        let output = fmt(&case, &options);
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{error}");
        assert!(!error.contains("internal compiler error"), "{error}");
        assert!(output.stdout.is_empty());
        assert_eq!(
            fs::read_to_string(case.0.join("main.rusk")).unwrap(),
            "wt 7;"
        );
    }
    assert_eq!(fs::read_dir(&case.0).unwrap().count(), 2);
}

#[test]
fn syntax_only_formatting_and_multiple_files_work() {
    let case = Case::new("include \"missing.rusk\";wt unknown(1);");
    fs::write(case.0.join("helper.rusk"), "fn f(){}").unwrap();
    assert!(fmt(&case, &["main.rusk"]).status.success());
    assert!(
        fmt(&case, &["--write", "main.rusk", "helper.rusk", "main.rusk"])
            .status
            .success()
    );
    assert!(
        fmt(&case, &["--check", "main.rusk", "helper.rusk"])
            .status
            .success()
    );
    let output = fmt(&case, &["main.rusk", "helper.rusk"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn formatted_programs_keep_sharing_capture_comparison_and_labeled_control_flow() {
    let input = "let shared:=1;let values:=[shared,shared];fn bump(x:Int)->Int{x+=1;return x;}let f:=()=>bump(shared);let a:=f();let b=shared;a=7;wt shared,b,values;let count:=0;wt 0<{count+=1;2}<3,count;outer:for i in 0..3{for j in 0..3{if j==1{continue outer;}count+=i;}}wt count;";
    let case = Case::new(input);
    for options in [vec![], vec!["--no-opt"]] {
        let before = case.run_with_options("", &options);
        assert!(before.status.success(), "{before:?}");
        assert_eq!(before.stdout, b"7 2 7 7\nYes 1\n4\n");
        let output = fmt(&case, &["--write", "main.rusk"]);
        assert!(output.status.success(), "{output:?}");
        let after = case.run_with_options("", &options);
        assert_eq!(after.status.code(), before.status.code());
        assert_eq!(after.stdout, before.stdout);
        assert_eq!(after.stderr, before.stderr);
        fs::write(case.0.join("main.rusk"), input).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_and_closed_stdout_are_handled() {
    use std::{
        ffi::OsString,
        os::{
            fd::OwnedFd,
            unix::{ffi::OsStringExt, net::UnixStream},
        },
        process::Stdio,
    };
    let case = Case::new("wt 1;");
    let path = OsString::from_vec(b"source-\xff.rusk".to_vec());
    fs::rename(case.0.join("main.rusk"), case.0.join(&path)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .args(["fmt", "--write"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(case.0.join(&path)).unwrap(), b"wt 1;\n");
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .arg("fmt")
        .arg(&path)
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{error}");
    assert!(
        error.contains("stdout:") && !error.contains("internal compiler error"),
        "{error}"
    );
}

#[test]
fn invalid_options_and_dash_paths_are_handled_without_writing() {
    let case = Case::new("wt 1;");
    for args in [
        vec![],
        vec!["--check", "--write", "main.rusk"],
        vec!["--unknown", "main.rusk"],
    ] {
        let output = fmt(&case, &args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        assert_eq!(
            fs::read_to_string(case.0.join("main.rusk")).unwrap(),
            "wt 1;"
        );
    }
    fs::rename(case.0.join("main.rusk"), case.0.join("-source.rusk")).unwrap();
    assert!(
        fmt(&case, &["--write", "--", "-source.rusk"])
            .status
            .success()
    );
    assert!(
        fmt(&case, &["--check", "--", "-source.rusk"])
            .status
            .success()
    );
}

#[cfg(unix)]
#[test]
fn updates_preserve_permissions_symlinks_and_unchanged_files() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let case = Case::new("wt 1;");
    let path = case.0.join("main.rusk");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    symlink("main.rusk", case.0.join("link.rusk")).unwrap();
    assert!(
        fmt(&case, &["--write", "link.rusk", "main.rusk"])
            .status
            .success()
    );
    assert!(
        fs::symlink_metadata(case.0.join("link.rusk"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    assert!(fmt(&case, &["--write", "main.rusk"]).status.success());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
    assert_eq!(fs::read_dir(&case.0).unwrap().count(), 2);

    fs::write(&path, "wt 2;").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o440)).unwrap();
    let output = fmt(&case, &["--write", "main.rusk"]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert_eq!(fs::read_to_string(&path).unwrap(), "wt 2;");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
}
