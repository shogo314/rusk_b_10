mod support;
use support::Case;

fn succeeds(source: &str, expected: &str) {
    let case = Case::new(source);
    for options in [&[][..], &["--no-opt"][..]] {
        let out = if options.is_empty() {
            case.run("")
        } else {
            case.run_with_options("", options)
        };
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(out.stdout, expected.as_bytes());
    }
}

fn rejects(source: &str, expected: &str) {
    let out = Case::new(source).compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "unexpected success: {source}");
    assert!(error.contains(expected), "{error}");
}

#[test]
fn capture_free_local_functions_support_forward_calls_and_mutual_recursion() {
    succeeds(
        r#"
fn run(n:Int)->Bool {
    return even(n);
    fn even(n:Int)->Bool {
        if n==0 { return true; }
        return odd(n-1);
    }
    fn odd(n:Int)->Bool {
        if n==0 { return false; }
        return even(n-1);
    }
}
wt run(6),run(5);
{ before(); fn before() { wt 7; } }
"#,
        "Yes No\n7\n",
    );
}

#[test]
fn nearest_function_family_hides_all_outer_overloads() {
    succeeds(
        r#"
fn choose(x:Int)->Int { return 1; }
{
    fn choose(x:Int)->Int { return 2; }
    wt choose(0);
    { fn choose(x:Int)->Int { return 3; } wt choose(0); }
    wt choose(0);
}
wt choose(0);
"#,
        "2\n3\n2\n1\n",
    );
    rejects(
        "fn f(x:Int){} { fn f(x:Str){} f(1); }",
        "no matching function",
    );
    rejects("{fn hidden(){} } hidden();", "no matching function");
}

#[test]
fn local_generic_functions_use_definition_scopes_and_outer_type_arguments() {
    succeeds(
        r#"
fn operation(x:Int)->Int { return 1; }
fn global<T>(x:T)->Int { return operation(x); }
fn identity<T>(x:T)->T {
    fn local(value:T)->T { return value; }
    return local(x);
}
fn shadowed<T>(x:T)->T {
    fn local<T>(value:T)->T { return value; }
    return local(x);
}
{
    fn operation(x:Int)->Int { return 2; }
    fn local<T>(x:T)->Int { return operation(x); }
    wt global(0),local(0),identity("ok"),shadowed(3);
}
"#,
        "1 2 ok 3\n",
    );
}

#[test]
fn capture_free_local_function_values_survive_their_definition_scope() {
    succeeds(
        r#"
let operation := {
    fn square(x:Int)->Int { return x*x; }
    square
};
wt operation(4);
"#,
        "16\n",
    );
}

#[test]
fn native_references_and_inference_use_the_template_definition_scope() {
    succeeds(
        r#"
fn choose(x:Int)->Int { return 7; }
extern fn via<T,R>(x:T)->R {
    code:"@{return R} @{fn}(Obj<@{T}> x,Loc loc){return @{call choose(T)->R}(x,loc);}",
}
{
    fn choose(x:Int)->Str { return "wrong scope"; }
    wt via(1);
}
"#,
        "7\n",
    );
}

#[test]
fn duplicate_local_declarations_and_variable_collisions_are_diagnosed() {
    succeeds("{fn f(x:Int){} fn f(x:Int){} } wt 1;", "1\n");
    rejects("{fn f(x:Int){} fn f(x:Int){} f(1);}", "ambiguous call");
    rejects("{fn f(){} let f:=1;}", "conflicts with function");
    rejects("fn outer(f:Int){fn f(){} }", "conflicts with variable");
    rejects("{fn unused(){wt missing;} }", "unknown variable");
}

#[test]
fn functions_and_variables_shadow_by_scope_rather_than_by_symbol_kind() {
    succeeds(
        r#"
let choose := (x:Int)=>x+1;
{
    fn choose(x:Int)->Int { return x+2; }
    let indirect := (x:Int)=>choose(x);
    wt choose(1),indirect(1);
}
wt choose(1);
fn target(x:Int)->Int { return x+3; }
let target := (x:Int)=>x+4;
wt target(1);
"#,
        "3 3\n2\n5\n",
    );
    rejects("fn f(x:Int){} let f:=1; f(2);", "not callable");
    rejects(
        "fn outer(){fn f(){} return; let f:=1;}",
        "conflicts with function",
    );
}

#[test]
fn named_captures_share_objects_but_freeze_bindings_at_declaration() {
    succeeds(
        r#"
{
    let n:=10;
    fn read()->Int {return n;}
    n=20;wt read();
    n:=30;wt read(),n;
    fn next()->Int {n+=1;return n;}
    wt next(),n,read();
}
"#,
        "20\n20 30\n31 31 20\n",
    );
}

#[test]
fn dfs_and_mutual_recursion_receive_the_original_capture_environment() {
    succeeds(
        r#"
{
    let graph:Vec<Vec<Int>>:=[[1,2],[2],[]];
    let visited:=[false;3];
    let count:=0;
    fn dfs(v:Int) {
        if visited[v] {return;}
        visited[v]=true;count+=1;
        for u in graph[v] {dfs(u);}
    }
    dfs(0);wt count;
}
let operation:={
    let count:=0;
    fn even(n:Int)->Int {count+=1;if n==0{return count;}return odd(n-1);}
    fn odd(n:Int)->Int {count+=1;if n==0{return count;}return even(n-1);}
    even
};
wt operation(3);
let detached=operation;
wt detached(0),operation(0);
{
    let n:=5;
    fn visit(depth:Int)->Int {
        if depth==0 {return n;}
        n:=100;
        return visit(depth-1);
    }
    wt visit(2),n;
}
"#,
        "3\n4\n5 5\n5 5\n",
    );
}

#[test]
fn captured_function_values_and_lambdas_keep_environments_alive_and_copy_them() {
    succeeds(
        r#"
let next:={
    let count:=0;
    fn increment()->Int {count+=1;return count;}
    increment
};
let shared:=next;
let detached=next;
wt next(),shared(),detached();
let independent=next;
wt independent();
next=independent;
wt next(),independent();
let read:={
    let n:=7;
    fn value()->Int {return n;}
    () => value()
};
wt read();
"#,
        "1 2 1\n3\n4 4\n7\n",
    );
}

#[test]
fn indirect_calls_and_nested_declarations_propagate_capture_dependencies() {
    succeeds(
        r#"
let operation:={
    let n:=8;
    fn outer()->Int {return inner();}
    fn inner()->Int {return n;}
    outer
};
wt operation();
{
    let n:=9;
    fn outer()->Int {
        fn inner()->Int {return n;}
        return inner();
    }
    wt outer();
}
"#,
        "8\n9\n",
    );
}

#[test]
fn capture_readiness_is_checked_through_calls_and_function_values() {
    for source in [
        "{let n:=1;wt read();fn read()->Int{return n;}}",
        "{let n:=1;let f:=read;fn read()->Int{return n;}}",
        "{let n:=1;fn f()->Int{return g();}wt f();fn g()->Int{return n;}}",
        "{let n:=1;let f:=()=>read();fn read()->Int{return n;}}",
        "let run:=()=>{let n:=1;return read();fn read()->Int{return n;}};wt run();",
    ] {
        rejects(source, "capture environment");
    }
    rejects(
        "{let n:Int;fn read()->Int{return n;}}",
        "before initialization",
    );
}

#[test]
fn repeated_factory_calls_keep_independent_named_capture_environments() {
    succeeds(
        r#"
let factory := (start:Int) => {
    let count:=start;
    fn next()->Int {count+=1;return count;}
    next
};
let a:=factory(0);
let b:=factory(100);
wt a(),b(),a();
let detached=a;
wt detached(),a();
"#,
        "1 101 2\n3 3\n",
    );
}

#[test]
fn local_function_example_keeps_character_owners_and_releases_recursive_environments() {
    succeeds(
        include_str!("../examples/beta10/local_functions.rusk"),
        "3\n1 101 2\n3 3\n4\n5 5\na\nZb!\n",
    );
}

#[test]
fn named_captures_preserve_contracts_and_do_not_retain_unused_variables() {
    succeeds(
        r#"
fn compute(noescape n:Int)->Int {
    fn read()->Int {return n+1;}
    return read();
}
wt compute(1);
{
    let text:="ab";
    let unused:=text[0];
    fn constant()->Int {return 7;}
    unused:='x';text.push_back('!');wt constant(),text;
}
{
    fixed let n:=1;
    fn bump(){n+=1;}
    bump();wt n;
}
"#,
        "2\n7 ab!\n2\n",
    );
    rejects("{readonly let n:=1;fn write(){n=2;}write();}", "readonly");
    rejects("{fixed let n:=1;fn change(){n:=2;}change();}", "fixed");
    rejects(
        r#"
extern fn retain<F>(f:F){code:"void @{fn}(Obj<@{F}>,Loc){}",}
fn publish(noescape n:Int){fn read()->Int{return n;}retain(read);}
publish(1);
"#,
        "noescape",
    );
}

#[test]
fn unreachable_capture_initializers_produce_source_errors_instead_of_invalid_ir() {
    for source in [
        "fn outer(){return;let n:={return;};fn f(){wt n;}}outer();",
        "fn outer(){return;let n:={return;};let f:=()=>n;}outer();",
        "fn outer(){return;let n:={return;};fn f<T>(x:T){wt n;}f(0);}outer();",
        "fn outer(){return;let n:={return;};fn middle(){fn f<T>(x:T){wt n;}f(0);}}outer();",
        "fn outer(){return;let n:={return;};let middle:=()=>{fn f<T>(x:T){wt n;}f(0);};}outer();",
    ] {
        rejects(source, "used before initialization");
    }
}
