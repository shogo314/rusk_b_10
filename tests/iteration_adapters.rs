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
fn adapters_share_values_and_copy_positions() {
    runs(
        include_str!("../examples/beta09/iteration_adapters.rusk"),
        "3 6 9\n9 10\n6 11 3 12 9 10 6 11 3 12\nabX! 5 4 3\n",
    );
}

#[test]
fn zip_checks_left_then_right_and_caches_readiness() {
    runs(
        r#"
struct Cursor { index: Int; events: Vec<Int>; tag: Int; }
fn __iter__(it: Cursor) -> Cursor { return it; }
fn __iter_has_next__(it: Cursor) -> Bool {
    it.events.push_back(it.tag);
    return it.index < 2;
}
fn __iter_next__(it: Cursor) -> Int {
    let result = it.index;
    it.index += 1;
    return result;
}
let events: Vec<Int> := [];
let left := Cursor { index: 0, events: events, tag: 1 };
let right := Cursor { index: 0, events: events, tag: 2 };
let pairs := zip(left, right);
wt __iter_has_next__(pairs), __iter_has_next__(pairs), events;
wt __iter_next__(pairs), events;
wt pairs.to_vec(), events;
"#,
        "Yes Yes 1 2\n0 0 1 2\n1 1 1 2 1 2 1 2\n",
    );
}

#[test]
fn adapters_compose_with_partially_consumed_and_short_circuiting_sources() {
    runs(
        r#"
let calls := 0;
let source := (0 .. 5).filter((x: Int) => x % 2 == 0).map((x: Int) => x + (calls += 1) * 0);
for value in source { wt value; break; }
let cursor := source.enumerate(-1);
wt calls;
let detached = cursor;
wt cursor.to_vec(), calls;
wt detached.to_vec(), calls;
let fewer := zip(0 .. 1, 2 .. 5);
wt fewer.any((pair: (Int, Int)) => pair[0] == 0);
let direct := __iter__([4, 5, 6]);
wt __iter_next__(direct), direct.enumerate().to_vec();
let range := (-9223372036854775808 .. -9223372036854775807).reversed();
wt range.to_vec();
let units: Vec<Unit> := [{}, {}];
wt units.enumerate().to_vec().len(), zip(units, [1, 2]).to_vec().len();
"#,
        "0\n1\n-1 2 0 4 3\n-1 2 0 4 3\nYes\n4 0 5 1 6\n-9223372036854775808\n2 2\n",
    );
}

#[test]
fn mismatches_and_structural_changes_keep_runtime_locations() {
    for (source, prefix, expected) in [
        (
            r#"let empty:Vec<Int>:=[];let other:=(0..2).map((x:Int)=>{wt "next";x});wt "before";zip(empty,other).to_vec();"#,
            "before\n",
            "zip inputs have different lengths",
        ),
        (
            r#"let empty:Vec<Int>:=[];let other:=(0..2).filter((x:Int)=>{wt x;true});wt "before";zip(empty,other).to_vec();"#,
            "before\n0\n",
            "zip inputs have different lengths",
        ),
        (
            r#"let cursor:=(1..3).map((x:Int)=>x);wt "before";zip(cursor,cursor);"#,
            "before\n",
            "zip requires distinct iterator states",
        ),
        (
            r#"for (i,x) in [1,2].enumerate(9223372036854775807){wt i,x;}"#,
            "9223372036854775807 1\n",
            "enumerate index overflow",
        ),
        (
            r#"let a:=[1,2];let cursor:=a.reversed();a.push_back(3);wt "before";cursor.to_vec();"#,
            "before\n",
            "container changed during iteration",
        ),
        (
            r#"let a:="ab";let cursor:=a.reversed();wt "before";a.push_back('c');wt cursor.to_vec();"#,
            "before\n",
            "string has active character references",
        ),
    ] {
        let output = Case::new(source).run("");
        assert_eq!(output.status.code(), Some(1), "{source}");
        assert_eq!(output.stdout, prefix.as_bytes());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:1:") && error.contains(expected),
            "{error}"
        );
    }
}

#[test]
fn reverse_owner_contract_keeps_restrictions_and_lifetimes() {
    runs(
        r#"
fn total(readonly noescape r: Range) -> Int {
    let sum := 0;
    for x in r.reversed() { sum += x; }
    return sum;
}
wt total(1 .. 4);
let s := "ab";
let held: Char;
{
    let it := s.reversed();
    held := __iter_next__(it);
}
s := "cd";
held = 'X';
wt held, s;
"#,
        "6\nX cd\n",
    );
    for (source, diagnostic) in [
        (
            "fn leak(noescape a:Vec<Int>)->ReverseVecIter<Int>{return a.reversed();}leak([1]);",
            "noescape",
        ),
        (
            "readonly let a:=[1];for value in a.reversed(){value=2;}",
            "readonly",
        ),
    ] {
        let output = Case::new(source).compile();
        assert_eq!(output.status.code(), Some(1), "{source}");
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(
            error.contains("main.rusk:") && error.contains(diagnostic),
            "{error}"
        );
    }
}
