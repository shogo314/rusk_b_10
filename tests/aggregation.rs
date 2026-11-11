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
fn reductions_share_selected_elements_and_keep_first_ties() {
    runs(
        include_str!("../examples/beta09/aggregation.rusk"),
        "7 20 1.75\n1 3 1 0\n3 9 1 2\n8 Yes Yes\n-0 1\n",
    );
}

#[test]
fn sum_accepts_unrelated_additive_types_without_copying() {
    runs(
        r#"
extern type Count { cpp_name: "std::int64_t", }
extern fn Count(x: Int) -> Count {
    cpp_name: "make_count", code: "Obj<std::int64_t> make_count(Obj<std::int64_t> x,Loc){return make_obj<std::int64_t>(*x);}",
}
extern fn __add__(a: Count, b: Count) -> Count {
    cpp_name: "add_count", code: "Obj<std::int64_t> add_count(Obj<std::int64_t> a,Obj<std::int64_t> b,Loc){return make_obj<std::int64_t>(*a+*b);}",
}
extern fn set_value(a: Count, x: Int) {
    cpp_name: "set_count", code: "void set_count(Obj<std::int64_t> a,Obj<std::int64_t> x,Loc){*a=*x;}",
}
extern fn value(a: Count) -> Int {
    cpp_name: "read_count", code: "Obj<std::int64_t> read_count(Obj<std::int64_t> a,Loc){return make_obj<std::int64_t>(*a);}",
}
let start := Count(10);
let empty: Vec<Count> := [];
let alias := sum(empty, start);
set_value(alias, 9);
let nonempty := sum([Count(1), Count(2)], start);
wt value(start), value(nonempty), sum([]), sum([1i128, 2i128]), sum(["a", "b"]);
"#,
        "9 12 0 3 ab\n",
    );
}

#[test]
fn extrema_use_remaining_positions_and_compare_only_after_first_value() {
    runs(
        r#"
let it := (0 .. 5).map((x: Int) => 5 - x);
for value in it { wt value; break; }
wt argmin(it)?!, argmin(it).is_none();
let calls := 0;
let comp := (a: Int, b: Int) => { calls += 1; a > b };
wt min_by_comp([1, 4, 4, 0], comp)?!, calls;
wt max_by_comp([1, 4, 4, 0], comp)?!, calls;
wt argmin_by_comp([1, 4, 4, 0], comp)?!, calls;
let empty: Vec<Int> := [];
wt max_by_comp(empty, comp).is_none(), calls;
let default_seed := 2;
let total := sum((0 .. 0), default_seed);
total = 4;
wt default_seed;
"#,
        "5\n3 Yes\n4 3\n0 6\n1 9\nYes 9\n4\n",
    );
}

#[test]
fn structural_changes_and_missing_comparisons_are_diagnosed() {
    let run = Case::new(
        r#"let a:=[1,2];wt "before";min_by_comp(a,(x:Int,y:Int)=>{a.push_back(0);x<y});"#,
    )
    .run("");
    assert_eq!(run.status.code(), Some(1));
    assert_eq!(run.stdout, b"before\n");
    let error = String::from_utf8(run.stderr).unwrap();
    assert!(error.contains("main.rusk:1:") && error.contains("container changed during iteration"));

    for (source, expected) in [
        ("min([1.0,2.0]);", "no matching native callable '__cmp__'"),
        (
            "struct Entry{x:Int;} let a:Vec<Entry>:=[];sum(a);",
            "no zero-argument constructor",
        ),
        (
            "readonly let x:=1;let a:Vec<Int>:=[];let y:=sum(a,x);y=2;",
            "readonly",
        ),
        (
            "fn f(noescape x:Int)->Int{let a:Vec<Int>:=[];return sum(a,x);}f(1);",
            "noescape",
        ),
    ] {
        let case = Case::new(source);
        let output = case.compile();
        assert_eq!(output.status.code(), Some(1), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:") && error.contains(expected),
            "{error}"
        );
        assert!(!case.0.join("main.cpp").exists());
    }
}
