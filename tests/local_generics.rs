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
    assert!(!error.contains("internal compiler error"), "{error}");
}

#[test]
fn specializations_share_declaration_time_snapshots() {
    succeeds(
        r#"
{
    let n:=1;
    fn paired<T>(x:T)->(T,Int) {return (x,n);}
    n=2;
    let a:=paired(3);let b:=paired("a");wt a[0],a[1],b[0],b[1];
    n:=9;let c:=paired(4);wt c[1],n;
    fn next<T>(unused:T)->Int {n+=1;return n;}
    wt next(0),next("a"),n;
}
"#,
        "3 2 a 2\n2 9\n10 11 11\n",
    );
}

#[test]
fn late_specializations_materialize_captures_through_named_and_lambda_parents() {
    succeeds(
        r#"
{
    let n:=7;
    fn outer()->Int {
        fn middle()->Int {
            fn inner<T>(unused:T)->Int {return n;}
            return inner(0);
        }
        return middle();
    }
    wt outer();
}
{
    let n:=8;
    let read:=()=>{
        fn inner<T>(unused:T)->Int {return n;}
        inner(0)
    };
    wt read();
    let detached=read;n=9;wt detached(),read();
}
{
    let n:=10;
    fn outer()->Int {
        let read:=()=>{
            fn inner<T>(unused:T)->Int {return n;}
            inner(0)
        };
        return read();
    }
    wt outer();
}
{
    let n:=11;
    fn outer<T>(x:T)->Int {
        fn inner<U>(unused:U)->Int {return n;}
        return inner(x);
    }
    wt outer(0),outer("a");
}
"#,
        "7\n8\n8 9\n10\n11 11\n",
    );
}

#[test]
fn specialization_after_rebinding_still_uses_the_declaration_binding() {
    succeeds(
        r#"
{
    let n:=1;
    fn outer()->Int {
        fn read<T>(unused:T)->Int {return n;}
        n:=2;
        return read(0);
    }
    wt outer(),n;
}
"#,
        "1 1\n",
    );
}

#[test]
fn local_generic_mutual_recursion_uses_one_snapshot_per_declaration_and_name() {
    succeeds(
        r#"
{
    let count:=0;
    fn even<T>(x:T,depth:Int)->Int {
        count+=1;
        if depth==0{return count;}
        return odd(x,depth-1);
    }
    fn odd<T>(x:T,depth:Int)->Int {
        count+=1;
        if depth==0{return count;}
        return even(x,depth-1);
    }
    wt even(0,3),odd("a",0),count;
}
"#,
        "4 5 5\n",
    );
}

#[test]
fn generic_capture_readiness_is_checked_even_before_the_first_specialization() {
    for source in [
        "{let n:=1;wt read(0);fn read<T>(x:T)->Int{return n;}}",
        "let run:=()=>{let n:=1;return read(0);fn read<T>(x:T)->Int{return n;}};wt run();",
        "{let n:=1;fn f<T>(x:T)->Int{return g(x);}wt f(0);fn g<T>(x:T)->Int{return n;}}",
    ] {
        rejects(source, "capture environment");
    }
    succeeds("{wt identity(3);fn identity<T>(x:T)->T{return x;}}", "3\n");
}

#[test]
fn generic_captures_respect_pattern_bindings_and_access_contracts() {
    succeeds(
        r#"
{
    let text:="ab";let n:=text[0];
    fn constant<T>(x:T)->Int {return match x {n=>7,};}
    n:='x';text.push_back('!');wt constant(1),text;
}
"#,
        "7 ab!\n",
    );
    rejects(
        "{readonly let n:=1;fn f<T>(unused:T){n=2;}f(0);}",
        "readonly",
    );
    rejects("{fixed let n:=1;fn f<T>(unused:T){n:=2;}f(0);}", "fixed");
    rejects(
        "fn publish(noescape n:Int)->Int{fn f<T>(unused:T)->Int{return n;}return f(0);}wt publish(1);",
        "noescape",
    );
}

#[test]
fn generic_capture_example_preserves_copied_characters_and_owner_lifetimes() {
    succeeds(
        include_str!("../examples/beta10/local_generics.rusk"),
        "3 2 a 2\n2 9\n4 5 5\nZ a\nZb!\n1 2 1 2\n",
    );
}

#[test]
fn generic_captures_work_through_native_higher_order_callbacks_and_copies() {
    succeeds(
        r#"
{
    let offset:=10;
    fn add<T>(value:T)->T {return value+offset;}
    let operation:=(value:Int)=>add(value);
    wt [1,2].map(operation).to_vec();
    let detached=operation;
    offset=20;
    wt detached(1),operation(1);
}
"#,
        "11 12\n11 21\n",
    );
}

#[test]
fn copied_callbacks_keep_one_environment_for_all_specializations_of_one_declaration() {
    succeeds(
        r#"
let operation:={
    let count:=0;
    fn next<T>(unused:T)->Int{count+=1;return count;}
    ()=>{let a=next(0);let b=next("a");(a,b)}
};
let detached=operation;
wt detached(),operation();
"#,
        "1 2 1 2\n",
    );
}
