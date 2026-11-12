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
fn local_records_support_forward_names_shadowing_and_ordinary_copy() {
    succeeds(
        r#"
struct Point{value:Int;}
let root:=Point{value:1};
{
    let p:Point:=Point{value:2};
    struct Point{value:Int;}
    let detached=p;detached.value=3;
    wt p.value,detached.value;
}
wt root.value;
"#,
        "2 3\n1\n",
    );
    rejects(
        "{struct Item{value:Int;}struct Item{value:Int;}}",
        "duplicate type",
    );
    rejects("{struct Item{value:Int;}}let x:Item;", "unknown type");
    rejects(
        "{struct __RuskLocalType0{value:Int;}}",
        "reserved internal type",
    );
}

#[test]
fn local_generic_records_and_enums_keep_their_nominal_identity() {
    succeeds(
        r#"
{
    struct Box<T>{value:T;}
    fn Box<T>(value:T)->Box<T>{return Box::<T>{value:value};}
    let a:=Box(3);let b=a;b.value=4;wt a.value,b.value;
    enum Choice<T>{Some(T),Empty}
    let choice:Choice<Int>:=Choice::Some(7);
    let detached=choice;
    wt match detached{Choice::Some(n)=>n,Choice::Empty=>0,};
    let empty:=Choice::<Int>::Empty;
    wt match empty{Choice::Some(n)=>n,Choice::Empty=>8,};
}
"#,
        "3 4\n7\n8\n",
    );
}

#[test]
fn local_type_fields_substitute_outer_parameters_but_local_type_names_shadow_them() {
    succeeds(
        r#"
fn run<T>(value:T)->T {
    struct Holder{value:T;}
    enum Choice{Some(Holder),Empty}
    let chosen:=Choice::Some(Holder{value:value});
    return match chosen{Choice::Some(held)=>held.value,Choice::Empty=>value,};
}
fn shadow<T>(unused:T)->Int {
    struct T{value:Int;}
    let held:T:=T{value:8};return held.value;
}
wt run(7),run("a"),shadow("unused");
"#,
        "7 a 8\n",
    );
}

#[test]
fn type_parameter_constructors_find_the_local_type_declaration_scope() {
    succeeds(
        r#"
fn zero<T>()->T{return T();}
{
    struct Point{value:Int;}
    fn Point()->Point{return Point{value:7};}
    let p:Point:=zero();wt p.value;
    struct Box<T>{value:T;}
    fn Box<T>()->Box<T>{return Box::<T>{value:T()};}
    let b:Box<Int>:=zero();wt b.value;
}
"#,
        "7\n0\n",
    );
}

#[test]
fn escaping_values_find_capture_free_operations_through_their_argument_types() {
    succeeds(
        r#"
fn extract<T>(x:T)->Int{return value(x);}
let item:={
    struct Item{value:Int;}
    fn value(x:Item)->Int{return x.value;}
    Item{value:12}
};
wt value(item),extract(item);
let detached=item;detached.value=13;wt value(item),value(detached);
"#,
        "12 12\n12 13\n",
    );
}

#[test]
fn associated_operations_propagate_capture_environments_through_rusk_generics() {
    succeeds(
        r#"
fn extract<T>(x:T)->Int{return adjusted(x);}
{
    let offset:=10;
    struct Item{value:Int;}
    fn adjusted(x:Item)->Int{return x.value+offset;}
    let item:=Item{value:2};wt extract(item);
}
"#,
        "12\n",
    );
    rejects(
        r#"
fn extract<T>(x:T)->Int{return adjusted(x);}
let item:={let offset:=10;struct Item{value:Int;}fn adjusted(x:Item)->Int{return x.value+offset;}Item{value:2}};
wt extract(item);
"#,
        "capture environment",
    );
}

#[test]
fn both_argument_types_contribute_without_left_to_right_priority() {
    rejects(
        r#"
let a:={struct A{value:Int;}fn combine<T>(a:A,b:T)->Int{return 1;}A{value:1}};
let b:={struct B{value:Int;}fn combine<T>(a:T,b:B)->Int{return 2;}B{value:2}};
wt combine(a,b);
"#,
        "ambiguous generic call",
    );
    succeeds(
        r#"
let a:={struct A{value:Int;}fn combine(a:A,b:A)->Int{return a.value;}A{value:7}};
wt combine(a,a);
"#,
        "7\n",
    );
}

#[test]
fn local_operator_hooks_use_the_same_hiding_rules_and_work_in_rusk_reductions() {
    succeeds(
        r#"
fn add_fields(a:Int,b:Int)->Int{return a+b;}
{
    struct Point{value:Int;}
    fn __add__(a:Point,b:Point)->Point{return Point{value:add_fields(a.value,b.value)};}
    let p:=Point{value:2};let q:=p+p;wt q.value;
    let total:=[p,q].sum(Point{value:0});wt total.value;
}
"#,
        "4\n6\n",
    );
    rejects(
        "{struct Item{value:Int;}fn __add__(a:Item,b:Item)->Item{return a;}wt 1+2;}",
        "no matching function",
    );
}

#[test]
fn associated_lookup_does_not_recurse_into_container_type_arguments() {
    rejects(
        r#"
fn extract<T>(x:T)->Int{return element_only(x);}
let values:={struct Item{value:Int;}fn element_only(x:Vec<Item>)->Int{return 7;}[Item{value:1}]};
wt extract(values);
"#,
        "no matching function",
    );
}

#[test]
fn local_nominal_types_are_stable_per_factory_and_distinct_between_declarations() {
    succeeds(
        r#"
fn same_type<T>(a:T,b:T)->Int{return a.value;}
let factory:=(value:Int)=>{struct Item{value:Int;}Item{value:value}};
let a:=factory(1);let b:=factory(2);wt same_type(a,b);
"#,
        "1\n",
    );
    let out = Case::new(
        r#"
fn same_type<T>(a:T,b:T){}
let a:={struct Item{value:Int;}Item{value:1}};
let b:={struct Item{value:Int;}Item{value:2}};
same_type(a,b);
"#,
    )
    .compile();
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(error.contains("Item (declared at"), "{error}");
    assert!(!error.contains("__RuskLocalType"), "{error}");
    for source in [
        "{struct Node{next:Node;}}",
        "{struct A{value:B;}enum B{Some(A)}}",
    ] {
        rejects(source, "recursive");
    }
}

#[test]
fn pure_local_ordering_hooks_work_through_native_sorting() {
    succeeds(
        r#"
fn compare_fields(a:Int,b:Int)->Ordering{return a<=>b;}
{
    struct Item{value:Int;}
    fn __cmp__(a:Item,b:Item)->Ordering{return compare_fields(a.value,b.value);}
    let values:=[Item{value:2},Item{value:1}];
    values.sort();
    wt values.map((item:Item)=>item.value).to_vec();
}
"#,
        "1 2\n",
    );
}

#[test]
fn function_values_carry_the_environment_separately_from_local_type_values() {
    succeeds(
        r#"
let make:=(offset:Int)=>{
    struct Item{value:Int;}
    fn adjusted(x:Item)->Int{offset+=1;return x.value+offset;}
    (Item{value:1},adjusted)
};
let a:=make(10);let b:=make(20);
wt a[1](b[0]),b[1](a[0]);
let detached=a;
wt detached[1](detached[0]),a[1](a[0]);
"#,
        "12 22\n13 13\n",
    );
}

#[test]
fn local_type_example_matches_the_published_behavior() {
    succeeds(
        include_str!("../examples/beta10/local_types.rusk"),
        "7 7\n3\n12 22\n13 13\n1 2\n",
    );
}
