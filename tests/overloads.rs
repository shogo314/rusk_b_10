mod support;
use support::Case;

fn succeeds(source: &str, expected: &str) {
    let out = Case::new(source).run("");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, expected.as_bytes());
}

fn rejects(source: &str, expected: &str) {
    let out = Case::new(source).compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "unexpected success: {source}");
    assert!(error.contains(expected), "{error}");
    assert!(error.contains("main.rusk:"), "{error}");
}

#[test]
fn fewer_generic_argument_positions_win_independently_of_declaration_order() {
    let declarations = [
        "fn choose<T,U>(a:T,b:U)->Int { return 2; }",
        "fn choose<T>(a:T,b:Int)->Int { return 1; }",
        "fn choose(a:Int,b:Int)->Int { return 0; }",
    ];
    for definitions in [
        declarations.join("\n"),
        declarations
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    ] {
        succeeds(
            &format!(
                "{definitions}\nwt choose(1,2),choose(\"a\",2),choose(1,\"a\"),choose::<Str>(\"a\",2);"
            ),
            "0 1 2 1\n",
        );
    }
}

#[test]
fn repeated_and_nested_type_parameters_count_argument_positions() {
    succeeds(
        r#"
fn repeated<T>(a:T,b:T)->Int { return 2; }
fn repeated<T>(a:T,b:Int)->Int { return 1; }
fn nested<T,U>(a:Vec<(T,U)>,b:Int)->Int { return 1; }
fn nested<T,U>(a:T,b:U)->Int { return 2; }
wt repeated(1,2),nested([(1,"a")],2);
"#,
        "1 1\n",
    );
    rejects(
        "fn f<T>(a:T,b:T){} fn f<T,U>(a:T,b:U){} f(1,2);",
        "ambiguous generic call",
    );
    rejects(
        "fn f<T>(a:T){} fn f<T>(a:Vec<T>){} f([1]);",
        "ambiguous generic call",
    );
}

#[test]
fn duplicate_declarations_only_fail_when_an_ambiguous_call_is_used() {
    succeeds(
        r#"
fn f(a:Int)->Int { return 1; }
fn f(a:Int)->Str { return "a"; }
fn g<T>(a:T)->T { return a; }
fn g<U>(a:U)->U { return a; }
wt 7;
"#,
        "7\n",
    );
    rejects("fn f(a:Int){} fn f(a:Int){} f(1);", "ambiguous call");
    rejects(
        "fn f<T>(a:T){} fn f<U>(a:U){} f(1);",
        "ambiguous generic call",
    );
    rejects(
        "fn f(a:Int)->Int{return 1;} fn f(a:Int)->Str{return \"a\";} let x:Int:=f(1);",
        "ambiguous call",
    );
}

#[test]
fn explicit_type_arguments_do_not_erase_declaration_rank() {
    succeeds(
        r#"
fn choose<T,U>(a:T,b:U)->Int { return 2; }
fn choose<T,U>(a:T,b:Int)->U { return U(); }
wt choose::<Int,Int>(1,2);
"#,
        "0\n",
    );
}

#[test]
fn zero_generic_argument_positions_tie_with_a_concrete_declaration() {
    rejects(
        "fn f()->Int{return 1;} fn f<T>()->T{return T();} let x:Int:=f();",
        "ambiguous call",
    );
    succeeds(
        "fn f()->Int{return 1;} fn f<T>()->T{return T();} wt f::<Int>();",
        "0\n",
    );
}

#[test]
fn chosen_body_failure_does_not_fall_back_to_a_more_generic_candidate() {
    rejects(
        r#"
fn f<T,U>(a:T,b:U)->Int { return 2; }
fn f<T>(a:T,b:Int)->Int { return a.missing; }
wt f(1,2);
"#,
        "has no fields",
    );
}

#[test]
fn native_signature_inference_and_expansion_use_the_same_rank() {
    succeeds(
        r#"
fn choose<T,U>(a:T,b:U)->Int { return 9; }
fn choose<T>(a:T,b:Int)->Str { return "chosen"; }
extern fn via<A,B,R>(a:A,b:B)->R {
    code:"@{return R} @{fn}(Obj<@{A}> a,Obj<@{B}> b,Loc loc){return @{call choose(A,B)->R}(a,b,loc);}",
}
wt via(1,2);
"#,
        "chosen\n",
    );
}

#[test]
fn integer_literal_context_is_independent_of_overload_order_and_uses_visible_groups() {
    let declarations = [
        "fn select_literal(value:Int)->Int{return 1;}",
        "fn select_literal(value:Int128)->Int{return 2;}",
        "fn select_literal<T>(value:T)->Int{return 3;}",
    ];
    for definitions in [
        declarations.join("\n"),
        declarations
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n"),
    ] {
        succeeds(
            &format!(
                r#"{definitions}
wt select_literal(1),select_literal(1i128),select_literal::<Int>(1);
let operation:=select_literal;
wt operation(1),operation(1i128),operation(2.0);
{{
    fn select_literal(value:Int128)->Int128{{return value;}}
    let local_operation:=select_literal;
    wt select_literal(18446744073709551616),local_operation(18446744073709551616);
}}
"#
            ),
            "1 2 3\n1 2 3\n18446744073709551616 18446744073709551616\n",
        );
    }
}
