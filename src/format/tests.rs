use super::*;

#[test]
fn syntax_layout_keeps_generics_unary_operators_and_array_repetition_distinct() {
    let input = "fn f<T>(x:Vec<(Int,T)>)->Int{let a:=- -1;let b:=[a;3];return b[0]+x[0][0];}wt 1<2,3>2;wt 3>(2);f::<Int>([]);";
    let formatted = source(input, "test.rusk").unwrap();
    assert!(
        formatted.contains("fn f<T>(x: Vec<(Int, T)>) -> Int {"),
        "{formatted}"
    );
    assert!(formatted.contains("let a := - -1;"), "{formatted}");
    assert!(formatted.contains("[a; 3]"), "{formatted}");
    assert!(formatted.contains("b[0] + x[0][0]"), "{formatted}");
    assert!(formatted.contains("wt 1 < 2, 3 > 2;"), "{formatted}");
    assert!(formatted.contains("wt 3 > (2);"), "{formatted}");
    assert!(formatted.contains("f::<Int>([]);"), "{formatted}");
    assert_eq!(source(&formatted, "test.rusk").unwrap(), formatted);
    frontend::parse(&formatted, "test.rusk").unwrap();
}

#[test]
fn literals_comments_semicolons_and_loop_labels_survive() {
    let input = "//! file\n/// function\nfn f(){outer // label\n :loop{let x:={1};wt \"// \\\"\", '\\n';// tail\nbreak outer;}}\n";
    let formatted = source(input, "test.rusk").unwrap();
    assert_eq!(source(&formatted, "test.rusk").unwrap(), formatted);
    frontend::parse(&formatted, "test.rusk").unwrap();
    assert!(formatted.contains("\"// \\\"\""));
    assert!(formatted.contains("/// function\nfn f()"));
    assert!(formatted.contains("1\n        };"), "{formatted}");
}

fn ast_without_spans(text: &str, path: &str) -> String {
    let ast = format!("{:#?}", frontend::parse(text, path).unwrap());
    let mut skipping = false;
    ast.lines()
        .filter(|line| {
            if line.trim_end().ends_with("Span {") {
                skipping = true;
                return false;
            }
            if skipping {
                if line.trim().starts_with('}') {
                    skipping = false;
                }
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn every_library_and_example_source_keeps_its_ast_and_formats_idempotently() {
    fn sources(path: &std::path::Path, result: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, result);
            } else if path.extension().is_some_and(|ext| ext == "rusk") {
                result.push(path);
            }
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut paths = vec![];
    sources(&root.join("lib"), &mut paths);
    sources(&root.join("examples"), &mut paths);
    assert!(paths.len() > 210, "the whole source corpus must be checked");
    for path in paths {
        let name = path.to_string_lossy();
        let text = std::fs::read_to_string(&path).unwrap();
        let formatted = source(&text, &name).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            source(&formatted, &name).unwrap(),
            formatted,
            "idempotence: {name}"
        );
        assert_eq!(
            ast_without_spans(&text, &name),
            ast_without_spans(&formatted, &name),
            "AST: {name}"
        );
    }
}

#[test]
fn expressions_patterns_and_comments_keep_their_syntax() {
    for input in [
        "enum E<T>{A(T),B}fn f<T>(x:T)->E<T>{return E::A(x);}match f(1){E::A(-1)=>{},E::A(x) if x>0=>{wt x;},E::B=>{}}",
        "let x:=if true{{1}}else{2};let f:=(x:Int)=>{let y:={x};y};wt f(x),[f(1);3];",
        "fn f(x:Vec<Int>){for (i,v) in [(0,1)]{if v>0{continue;}else{break;}}while let Some(x)=None{wt x;}}",
        "let a:=1;let b:={a};(a,b)=(b,a);wt {a}+{b},({a},[b;2]);",
    ] {
        let formatted = source(input, "test.rusk").unwrap();
        assert_eq!(
            ast_without_spans(input, "test.rusk"),
            ast_without_spans(&formatted, "test.rusk")
        );
        assert_eq!(source(&formatted, "test.rusk").unwrap(), formatted);
    }
}

#[test]
fn inline_documentation_is_not_promoted_and_blank_lines_keep_comments_detached() {
    for input in [
        "struct S{ /// inline field\nx:Int;}",
        "fn f( /// inline parameter\nx:Int){}",
        "/// detached\n\nfn f(){}",
        "/// interrupted\n// ordinary\nfn f(){}",
        "//! file\r\nstruct S{\r\n/// attached\r\nx:Int;}",
        "enum E{A(Int)}match E::A(1){E::A(-1)=>{}}",
    ] {
        let formatted = source(input, "test.rusk").unwrap();
        assert_eq!(source(&formatted, "test.rusk").unwrap(), formatted);
        assert_eq!(
            ast_without_spans(input, "test.rusk"),
            ast_without_spans(&formatted, "test.rusk")
        );
    }
}

#[test]
fn long_argument_groups_break_and_formatting_is_idempotent() {
    let input = format!(
        "wt f({});",
        (0..25)
            .map(|i| format!("argument_{i}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    let formatted = source(&input, "long.rusk").unwrap();
    assert!(formatted.contains("f(\n"), "{formatted}");
    assert!(formatted.contains("    argument_0,"), "{formatted}");
    assert_eq!(source(&formatted, "long.rusk").unwrap(), formatted);
}

#[test]
fn formatter_does_not_require_types_or_include_targets() {
    let formatted = source(
        "include \"missing.rusk\";let x:Unknown:=missing();",
        "test.rusk",
    )
    .unwrap();
    assert!(formatted.contains("Unknown := missing()"));
}
