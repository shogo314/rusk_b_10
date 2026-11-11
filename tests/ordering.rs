mod support;
use support::Case;

fn runs(source: &str, expected: &str) {
    let case = Case::new(source);
    for options in [&[][..], &["--no-opt"][..]] {
        let output = if options.is_empty() {
            case.run("")
        } else {
            case.run_with_options("", options)
        };
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, expected.as_bytes());
    }
}

#[test]
fn total_and_partial_orders_drive_stable_sorting() {
    runs(
        include_str!("../examples/beta09/ordering.rusk"),
        "Yes Yes Yes\nb a c\nnan -inf -0 0 inf nan\nYes Yes\n",
    );
    runs(
        r#"
wt false < true, is_less(false <=> true), (3 <=> 3) == Ordering::Equal;
wt 1 <=> 2, 2 <=> 2, 3 <=> 2;
wt is_less((1, [2, 3]) <=> (1, [2, 4])), is_greater([1, 2] <=> [1]);
wt (1.0, 2) < (2.0, 1), is_equal(partial_cmp(-0.0, 0.0)?!);
let nested := [[2], [1, 2], [1]];
sort(nested);
for value in nested { wt value; }
"#,
        "Yes Yes Yes\nLess Equal Greater\nYes Yes\nYes Yes\n1\n1 2\n2\n",
    );
}

#[test]
fn fallback_uses_a_single_same_direction_comparison_and_honors_dedicated_hooks() {
    runs(
        r#"
struct Key { rank: Int; visits: Vec<Int>; }
fn __cmp__(a: Key, b: Key) -> Ordering { a.visits[0] += 1; return a.rank <=> b.rank; }
fn __lt__(a: Key, b: Key) -> Bool { a.visits[0] += 100; return a.rank > b.rank; }
let a := Key { rank: 1, visits: [0] };
let b := Key { rank: 2, visits: [0] };
let c := Key { rank: 3, visits: [0] };
wt a < b, a <= b, a > b, a >= b, a.visits[0], b.visits[0];
wt a <= b <= c, a.visits[0], b.visits[0], c.visits[0];
wt c <= b <= a, b.visits[0], c.visits[0];
let values := [b, a];
sort(values);
wt values[0].rank, values[1].rank;
"#,
        "No Yes No No 103 0\nYes 104 1 0\nNo 1 1\n1 2\n",
    );
}

#[test]
fn fallback_composes_with_generics_typed_native_references_and_effects() {
    runs(
        r#"
struct Key<T> { value: T; }
fn __cmp__<T>(a:Key<T>, b:Key<T>)->Ordering {return a.value <=> b.value;}
extern fn via<T,R>(a:T,b:T)->R {
    code:"@{return R} @{fn}(Obj<@{T}> a, Obj<@{T}> b, Loc loc) { return @{call __lt__(T,T)->R}(a,b,loc); }",
}
fn check(readonly noescape a:Key<Int>, readonly noescape b:Key<Int>)->Bool {return a < b;}
let a:=Key::<Int>{value:1};
let b:=Key::<Int>{value:2};
readonly let view:=a;
wt check(view,b), via(a,b), via(b,a), is_less(a<=>b);
"#,
        "Yes Yes No Yes\n",
    );
}

#[test]
fn missing_invalid_and_ambiguous_ordering_is_diagnosed() {
    for (source, expected) in [
        ("wt 1.0 <=> 2.0;", "no matching function '__cmp__'"),
        (
            "let a:Vec<Float>:=[];sort(a);",
            "no matching native callable '__cmp__'",
        ),
        (
            "let a:Vec<Int>:=[];sort_by_key(a,(x:Int)=>Float(x));",
            "no matching native callable '__cmp__'",
        ),
        (
            "wt (1.0,2) <=> (2.0,1);",
            "tuple field 0 of type 'Float' has no total order",
        ),
        ("wt 1 <=> 2 <=> 3;", "cannot be chained"),
        (
            "struct Key {x:Int;} fn __cmp__(a:Key,b:Key)->Int {return 0;} wt Key{x:1} < Key{x:2};",
            "expected Ordering",
        ),
        (
            "struct Key {x:Int;} fn __cmp__(a:Key,b:Key)->Ordering {return a.x<=>b.x;} fn __lt__(a:Key,b:Key)->Int {return 0;} wt Key{x:1} < Key{x:2};",
            "expected Bool",
        ),
        (
            "struct Key {x:Int;} fn __cmp__(a:Key,b:Key)->Ordering {return a.x<=>b.x;} fn __lt__<T>(a:T,b:Key)->Bool{return true;} fn __lt__<T>(a:Key,b:T)->Bool{return false;} wt Key{x:1} < Key{x:2};",
            "ambiguous generic call to '__lt__'",
        ),
        (
            "struct Key {x:Int;} fn __lt__(a:Key,b:Key)->Bool {return a.x<b.x;} let a:Vec<Key>:=[];sort(a);",
            "no matching native callable '__cmp__'",
        ),
    ] {
        let case = Case::new(source);
        let output = case.compile();
        assert_eq!(output.status.code(), Some(1), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:") && error.contains(expected),
            "{source}\n{error}"
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}
