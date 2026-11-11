mod support;
use std::{fs, process::Command};
use support::Case;

fn cli(case: &Case, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn help_and_version_succeed_without_reading_a_source_file() {
    let case = Case::new("not a valid program");
    for (args, expected) in [
        (vec!["--help"], "dependencies"),
        (vec!["-h"], "compile"),
        (vec!["compile", "--help"], "--emit"),
        (vec!["dependencies", "--help"], "<FILE>"),
        (vec!["help", "compile"], "--no-opt"),
    ] {
        let output = cli(&case, &args);
        assert!(output.status.success(), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains("Usage:") && text.contains(expected), "{text}");
    }
    let output = cli(&case, &["--version"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    assert_eq!(
        output.stdout,
        format!("rusk {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
    );
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn argument_errors_exit_with_two_and_do_not_generate_output() {
    let case = Case::new("wt 7;");
    for args in [
        vec![],
        vec!["unknown"],
        vec!["compile"],
        vec!["dependencies"],
        vec!["compile", "main.rusk", "--emit", "unknown"],
        vec!["compile", "main.rusk", "--emit"],
        vec!["compile", "main.rusk", "--output"],
        vec!["compile", "main.rusk", "--unknown"],
        vec!["compile", "main.rusk", "extra.rusk"],
        vec!["dependencies", "main.rusk", "--no-opt"],
        vec!["dependencies", "main.rusk", "extra.rusk"],
    ] {
        let output = cli(&case, &args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty(), "{args:?}: {output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("Usage:") || error.contains("--help"),
            "{args:?}: {error}"
        );
        assert!(!case.0.join("a.cpp").exists(), "{args:?}");
    }
}

#[test]
fn default_output_and_optimization_flags_keep_the_existing_compilation() {
    let case = Case::new("let value:=3;let alias:=value;value+=4;wt value,alias;");
    let output = case.run("");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"7 7\n");
    let expected = fs::read(case.0.join("main.cpp")).unwrap();
    // Case::run honors the reference-backend setting; use the same mode when
    // comparing its explicit output path with the CLI's default output path.
    let mut args = vec!["compile", "main.rusk"];
    if std::env::var_os("RUSK_TEST_NO_OPT").is_some() {
        args.push("--no-opt");
    }
    let output = cli(&case, &args);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stdout.is_empty() && output.stderr.is_empty());
    assert_eq!(fs::read(case.0.join("a.cpp")).unwrap(), expected);

    assert!(case.compile_with_options(&["--no-opt"]).status.success());
    let expected = fs::read(case.0.join("main.cpp")).unwrap();
    let output = cli(
        &case,
        &[
            "compile",
            "main.rusk",
            "--emit",
            "hir",
            "--emit=cpp",
            "-o",
            "unused.cpp",
            "--output=reference.cpp",
            "--no-opt",
            "--no-opt",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    assert_eq!(fs::read(case.0.join("reference.cpp")).unwrap(), expected);
    assert!(!case.0.join("unused.cpp").exists());
}

#[test]
fn emission_stages_and_output_paths_work_with_options_before_the_input() {
    let case = Case::new("wt 1;");
    for stage in ["cst", "ast", "hir", "lir", "cpp"] {
        let output = cli(
            &case,
            &["compile", "main.rusk", "--emit", stage, "-o", "expected"],
        );
        assert!(output.status.success(), "{stage}: {output:?}");
        let expected = fs::read(case.0.join("expected")).unwrap();
        let emit = format!("--emit={stage}");
        let output = cli(
            &case,
            &["compile", &emit, "--output", "actual output", "main.rusk"],
        );
        assert!(output.status.success(), "{stage}: {output:?}");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        assert!(!expected.is_empty());
        assert_eq!(fs::read(case.0.join("actual output")).unwrap(), expected);
    }
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn dependencies_list_files_without_type_checking_or_generating_cpp() {
    let case = Case::new("include \"helper.rusk\";wt missing;");
    fs::write(
        case.0.join("helper.rusk"),
        "fn twice(x:Int)->Int{return x*2;}",
    )
    .unwrap();
    let output = cli(&case, &["dependencies", "main.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty());
    let text = String::from_utf8(output.stdout).unwrap();
    let mut actual: Vec<_> = text.lines().map(std::path::PathBuf::from).collect();
    actual.sort();
    let mut expected = vec![
        fs::canonicalize(case.0.join("main.rusk")).unwrap(),
        fs::canonicalize(case.0.join("helper.rusk")).unwrap(),
    ];
    expected.sort();
    assert_eq!(actual, expected);
    assert!(!case.0.join("a.cpp").exists());
}

#[test]
fn source_and_io_errors_still_exit_with_one() {
    let case = Case::new("let value:Int;wt value;");
    for (args, message) in [
        (vec!["compile", "main.rusk"], "used before initialization"),
        (vec!["compile", "missing.rusk"], "missing.rusk"),
        (vec!["dependencies", "missing.rusk"], "missing.rusk"),
    ] {
        let output = cli(&case, &args);
        assert_eq!(output.status.code(), Some(1), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(error.contains(message), "{args:?}: {error}");
        assert!(!error.contains("Usage:"), "{args:?}: {error}");
        assert!(!case.0.join("a.cpp").exists());
    }
}

#[test]
fn invalid_sources_are_user_diagnostics_and_preserve_existing_output() {
    for (source, expected) in [
        ("let := ;", "syntax error"),
        ("wt missing;", "main.rusk:"),
        ("let x:Int:=\"wrong\";", "main.rusk:"),
        ("fixed let x:Int;loop{x:=1;}", "cannot rebind fixed"),
        ("readonly let x:=1;x=2;", "readonly"),
        (
            "fn leak(noescape x:Int)->Int{return x;}leak(1);",
            "noescape",
        ),
        (
            "extern fn f<T>(x:T){code:\"@{Missing}\",} f(1);",
            "main.rusk:",
        ),
        (
            "extern type<T> Cell {cpp_name:\"{T\",} let c:Cell<Int>;",
            "main.rusk:",
        ),
        ("include \"missing.rusk\";", "cannot load"),
        (
            "extern symbol missing {code_file:\"missing.hpp\",}",
            "main.rusk:",
        ),
    ] {
        let case = Case::new(source);
        let target = case.0.join("a.cpp");
        fs::write(&target, b"previous successful output").unwrap();
        for stage in ["hir", "lir", "cpp"] {
            for no_opt in [false, true] {
                let mut args = vec!["compile", "main.rusk", "--emit", stage];
                if no_opt {
                    args.push("--no-opt");
                }
                let output = cli(&case, &args);
                let error = String::from_utf8_lossy(&output.stderr);
                assert_eq!(output.status.code(), Some(1), "{source}: {error}");
                assert!(output.stdout.is_empty(), "{source}: {output:?}");
                assert!(error.contains(expected), "{source}: {error}");
                assert!(
                    !error.contains("internal compiler error"),
                    "{source}: {error}"
                );
                assert!(!error.contains("panicked at"), "{source}: {error}");
                assert_eq!(fs::read(&target).unwrap(), b"previous successful output");
            }
        }
    }
}

#[test]
fn generated_deep_inputs_report_limits_without_replacing_output() {
    let case = Case::new("");
    let source = case.0.join("main.rusk");
    let target = case.0.join("a.cpp");
    fs::write(&target, "previous output").unwrap();
    let parens = format!("wt {}1{};", "(".repeat(512), ")".repeat(512));
    for args in [
        vec!["compile", "main.rusk", "--emit", "cst"],
        vec!["compile", "main.rusk", "--emit", "ast"],
        vec!["fmt", "main.rusk", "--write"],
        vec!["doc", "--file", "main.rusk"],
    ] {
        fs::write(&source, &parens).unwrap();
        let output = cli(&case, &args);
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{args:?}: {error}");
        assert!(
            error.contains("main.rusk:1:") && error.contains("delimiter nesting limit"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&source).unwrap(), parens);
        assert_eq!(fs::read_to_string(&target).unwrap(), "previous output");
    }
    for expression in [
        format!("{}1", "1+".repeat(512)),
        format!("{}1", "1**".repeat(512)),
    ] {
        fs::write(&source, format!("wt {expression};")).unwrap();
        for options in [&[][..], &["--no-opt"][..]] {
            let mut args = vec!["compile", "main.rusk"];
            args.extend(options);
            let output = cli(&case, &args);
            let error = String::from_utf8_lossy(&output.stderr);
            assert_eq!(output.status.code(), Some(1), "{error}");
            assert!(
                error.contains("nesting limit") && error.contains("main.rusk:"),
                "{error}"
            );
            assert_eq!(fs::read_to_string(&target).unwrap(), "previous output");
        }
    }
    let mut layered = "x".to_owned();
    for _ in 0..25 {
        layered = format!("({layered}){}", "+x".repeat(300));
    }
    let deep_function = format!("fn f<T>(x:T)->T{{return {layered};}}wt f(1);");
    let wide_block = format!("fn f(){{{}}} f();", "wt 1;".repeat(2400));
    for (program, message) in [
        (deep_function, "expression nesting limit"),
        (wide_block, "block statement limit"),
    ] {
        fs::write(&source, program).unwrap();
        let output = cli(&case, &["compile", "main.rusk", "--emit", "ast"]);
        let error = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(1), "{error}");
        assert!(
            error.contains("main.rusk:1:") && error.contains(message),
            "{error}"
        );
        assert_eq!(fs::read_to_string(&target).unwrap(), "previous output");
    }

    fs::write(&source, "include \"0.rusk\";").unwrap();
    for n in 0..200 {
        let text = if n == 199 {
            "wt 1;".into()
        } else {
            format!("include \"{}.rusk\";", n + 1)
        };
        fs::write(case.0.join(format!("{n}.rusk")), text).unwrap();
    }
    let output = cli(&case, &["dependencies", "main.rusk"]);
    let error = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{error}");
    assert!(
        error.contains(".rusk:1:") && error.contains("include nesting limit"),
        "{error}"
    );
    assert!(output.stdout.is_empty());
}

#[test]
fn syntax_and_type_mutations_use_the_normal_cli_error_path() {
    let case = Case::new("");
    let target = case.0.join("a.cpp");
    let seed = "fn pick<T>(noescape x:T, y:T)->T{return y;} wt pick(1, 2);";
    for (from, to, syntax_error) in [
        ("fn pick", "fn (", true),
        ("->T", "->", true),
        ("return y", "return x", false),
        ("pick(1, 2)", "pick(1, \"2\")", false),
        ("wt pick", "wt missing", false),
    ] {
        let source = seed.replace(from, to);
        fs::write(case.0.join("main.rusk"), &source).unwrap();
        let result = cli(&case, &["fmt", "main.rusk"]);
        assert_eq!(
            result.status.code(),
            Some(i32::from(syntax_error)),
            "{source}: {result:?}"
        );
        for options in [&[][..], &["--no-opt"][..]] {
            let mut args = vec!["compile", "main.rusk"];
            args.extend(options);
            let result = cli(&case, &args);
            let error = String::from_utf8_lossy(&result.stderr);
            assert_eq!(result.status.code(), Some(1), "{source}: {error}");
            assert!(error.contains("main.rusk:"), "{source}: {error}");
            assert!(!target.exists());
        }
    }
}

#[test]
fn nested_blocks_with_many_statements_can_be_lowered() {
    let prefix = format!("{{{}", "wt 1;".repeat(900));
    let source = format!("fn f(){{{}{}}} f();", prefix.repeat(3), "}".repeat(3));
    let case = Case::new(&source);
    let output = cli(&case, &["compile", "main.rusk"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(case.0.join("a.cpp").exists());
}

#[test]
fn output_write_errors_are_user_diagnostics() {
    let case = Case::new("wt 1;");
    let output = cli(&case, &["compile", "main.rusk", "-o", "missing/output.cpp"]);
    let error = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{error}");
    assert!(error.contains("missing/output.cpp"), "{error}");
    assert!(!error.contains("internal compiler error"), "{error}");
}

#[cfg(unix)]
#[test]
fn closed_dependency_output_is_an_io_error_not_a_panic() {
    use std::{os::fd::OwnedFd, os::unix::net::UnixStream, process::Stdio};
    let case = Case::new("wt 1;");
    let (reader, writer) = UnixStream::pair().unwrap();
    drop(reader);
    let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
        .current_dir(&case.0)
        .args(["dependencies", "main.rusk"])
        .stdout(Stdio::from(OwnedFd::from(writer)))
        .stderr(Stdio::piped())
        .output()
        .unwrap();
    let error = String::from_utf8_lossy(&output.stderr);
    assert_eq!(output.status.code(), Some(1), "{error}");
    assert!(error.contains("stdout:"), "{error}");
    assert!(!error.contains("internal compiler error"), "{error}");
}

#[test]
fn dash_prefixed_files_can_be_passed_after_the_option_separator() {
    let case = Case::new("wt 7;");
    fs::rename(case.0.join("main.rusk"), case.0.join("-source.rusk")).unwrap();
    let output = cli(&case, &["compile", "--", "-source.rusk"]);
    assert!(output.status.success(), "{output:?}");
    assert!(case.0.join("a.cpp").exists());
}

#[cfg(unix)]
#[test]
fn file_paths_need_not_be_utf8() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    let case = Case::new("wt 7;");
    let file = OsString::from_vec(b"source-\xff.rusk".to_vec());
    fs::rename(case.0.join("main.rusk"), case.0.join(&file)).unwrap();
    for stage in ["cst", "ast", "cpp"] {
        let output = Command::new(env!("CARGO_BIN_EXE_rusk"))
            .current_dir(&case.0)
            .args(["compile", "--emit", stage])
            .arg(&file)
            .output()
            .unwrap();
        assert!(output.status.success(), "{stage}: {output:?}");
        assert!(!fs::read(case.0.join("a.cpp")).unwrap().is_empty());
    }
}
