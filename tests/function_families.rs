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
fn generic_and_overloaded_families_can_be_aliased_and_passed_through_rusk_generics() {
    succeeds(
        r#"
fn apply<T,F>(value:T,operation:F)->T{return operation(value);}
let operation:=copy;
wt apply(3,operation),apply("abc",operation);
fn choose(value:Int)->Int{return value+1;}
fn choose(value:Str)->Str{return value+"!";}
let selected:=choose;
wt apply(4,selected),apply("yes",selected);
"#,
        "3 abc\n5 yes!\n",
    );
}

#[test]
fn aliases_keep_the_original_name_and_lookup_scope() {
    succeeds(
        r#"
fn apply<T,F>(value:T,operation:F)->Int {
    fn choose(value:T)->Int{return 99;}
    return operation(value);
}
fn choose(value:Int)->Int{return 1;}
let operation:=choose;
{
    fn choose(value:Int)->Int{return 2;}
    wt operation(0),apply(0,operation);
}
"#,
        "1 1\n",
    );
    rejects(
        "fn choose(x:Int){}fn choose(x:Int){}let operation:=choose;operation(0);",
        "ambiguous",
    );
}

#[test]
fn native_higher_order_operations_infer_each_family_specialization() {
    succeeds(
        r#"
fn duplicate<T>(value:T)->T{return value+value;}
fn change(value:Int)->Int{return value+1;}
fn change(value:Str)->Str{return value+"!";}
wt [1,2].map(duplicate).to_vec();
wt ["a","b"].map(change).to_vec();
"#,
        "2 4\na! b!\n",
    );
}

#[test]
fn generic_family_values_keep_their_capture_environment_across_specializations_and_copy() {
    succeeds(
        r#"
let next:={
    let count:=0;
    fn advance<T>(unused:T)->Int{count+=1;return count;}
    advance
};
let detached=next;
wt detached(0),detached("a"),next(0),next("a");
"#,
        "1 2 1 2\n",
    );
}

#[test]
fn family_invocations_add_direct_argument_type_candidates() {
    succeeds(
        r#"
fn choose(value:Int)->Int{return value;}
let operation:=choose;
let item:={
    struct Item{value:Int;}
    fn choose(item:Item)->Int{return item.value;}
    Item{value:7}
};
wt operation(item);
"#,
        "7\n",
    );
}

#[test]
fn context_result_types_complete_return_only_generic_parameters() {
    succeeds(
        r#"
fn zero<T>()->T{return T();}
let construct:=zero;
let number:Int:=construct();
let text:Str:=construct();
wt number,text.len();
"#,
        "0 0\n",
    );
}

#[test]
fn argument_associated_captures_are_supplied_at_invocation_in_rusk_and_native_calls() {
    succeeds(
        r#"
fn apply<T,F>(value:T,operation:F)->Int{return operation(value);}
fn evaluate(value:Int)->Int{return value;}
let operation:=evaluate;
{
    let offset:=10;
    struct Item{value:Int;}
    fn evaluate(item:Item)->Int{return item.value+offset;}
    wt operation(Item{value:1}),apply(Item{value:2},operation);
    wt [Item{value:3}].map(operation).to_vec();
    offset=20;
    wt operation(Item{value:1});
}
"#,
        "11 12\n13\n21\n",
    );
    rejects(
        r#"
fn evaluate(value:Int)->Int{return value;}
let operation:=evaluate;
let item:={let offset:=10;struct Item{value:Int;}fn evaluate(item:Item)->Int{return item.value+offset;}Item{value:1}};
operation(item);
"#,
        "capture environment",
    );
}

#[test]
fn held_lexical_captures_and_invocation_captures_use_independent_environments() {
    succeeds(
        r#"
fn read_value<T>(item:T)->Int{return item_value(item);}
let operation:={
    let count:=0;
    fn combine<T>(item:T)->Int{count+=1;return count+read_value(item);}
    combine
};
let detached=operation;
{
    let offset:=10;
    struct Item{value:Int;}
    fn item_value(item:Item)->Int{return item.value+offset;}
    wt operation(Item{value:1}),detached(Item{value:1}),operation(Item{value:1});
    wt [Item{value:1}].map(operation).to_vec();
}
"#,
        "12 12 13\n14\n",
    );
}

#[test]
fn family_acquisition_checks_all_lexical_candidates_and_preserves_access_contracts() {
    succeeds(
        "{before();fn before(){wt 7;}let operation:=before;operation();}",
        "7\n7\n",
    );
    rejects(
        "{let n:=0;let operation:=next;fn next<T>(value:T)->Int{return n;}operation(0);}",
        "not ready",
    );
    rejects(
        "{let n:=0;fn choose(x:Int){}let operation:=choose;fn choose(x:Str){wt n;}}",
        "not ready",
    );
    rejects(
        r#"
extern fn retain<F>(f:F){code:"void @{fn}(Obj<@{F}> f,Loc loc){@{invoke F(Int)->Int}(f,make_obj<@{Int}>(0),loc);}",}
fn publish(noescape value:Int){fn operation<T>(unused:T)->Int{return value;}retain(operation);}
publish(1);
"#,
        "noescape",
    );
    rejects(
        "fn change(readonly value:Int){fn operation<T>(unused:T){value=2;}let f:=operation;f(0);}change(1);",
        "readonly",
    );
}

#[test]
fn family_example_preserves_specializations_copies_and_character_owners() {
    succeeds(
        include_str!("../examples/beta10/function_families.rusk"),
        "3 abc\n2 4\n1 2 1 2\n12 12 13\n14\nZ a\nZb!\n",
    );
}

#[test]
fn native_callback_context_completes_return_only_parameters_without_selecting_by_result() {
    succeeds(
        r#"
extern fn apply<F,R>(f:F)->R{
    code:"@{return R} @{fn}(Obj<@{F}> f,Loc loc){return @{invoke F()->R}(f,loc);}",
}
fn zero<T>()->T{return T();}
let number:Int:=apply(zero);
let text:Str:=apply(zero);
wt number,text.len();
"#,
        "0 0\n",
    );
    rejects(
        r#"
extern fn apply<F,R>(f:F,value:Int)->R{
    code:"Obj<@{R}> @{fn}(Obj<@{F}> f,Obj<@{Int}> value,Loc loc){return @{invoke F(Int)->R}(f,value,loc);}",
}
fn select<T>(value:T)->Int{return 1;}
fn select<T>(value:T)->Str{return "a";}
let result:Int:=apply(select,0);
"#,
        "no matching",
    );
}

#[test]
fn explicitly_specialized_references_validate_their_type_arguments_at_acquisition() {
    succeeds(
        "fn identity<T>(value:T)->T{return value;}let f:=identity::<Int>;wt f(3);",
        "3\n",
    );
    rejects(
        "fn identity<T>(value:T)->T{return value;}let f:=identity::<Missing>;",
        "unknown type",
    );
    rejects(
        "fn identity<T>(value:T)->T{return value;}let f:=identity::<Int,Str>;",
        "unknown variable or function",
    );
}

#[test]
fn function_family_unit_results_use_the_same_cleanup_as_rusk_callbacks() {
    succeeds(
        r#"
extern fn token(count:Int)->Unit {
    return_abi:"object",
    code:"Obj<Unit> token(Obj<std::int64_t> count,Loc){return Obj<Unit>(new Unit{},[count](auto* value){delete value;++*count;});}",
    cpp_name:"token",
}
extern fn boxed_apply<F>(f:F,count:Int)->Unit {
    return_abi:"object",
    code:"Obj<@{Unit}> @{fn}(Obj<@{F}> f,Obj<@{Int}> count,Loc loc){return @{boxed_invoke F(Int)->Unit}(f,count,loc);}",
}
extern fn discard_apply<F>(f:F,count:Int) {
    code:"void @{fn}(Obj<@{F}> f,Obj<@{Int}> count,Loc loc){@{invoke F(Int)->Unit}(f,count,loc);}",
}
let count:=0;
let direct:=token(count);
wt count;
direct:={};
wt count;
let make:=token;
let held:=make(count);
wt count;
held:={};
wt count;
let boxed:=boxed_apply(make,count);
wt count;
boxed:={};
wt count;
discard_apply(make,count);
wt count;
"#,
        "0\n1\n2\n2\n3\n3\n4\n",
    );
}

#[test]
fn copied_environments_are_preserved_by_recursive_family_acquisition_and_mutual_calls() {
    succeeds(
        include_str!("../examples/beta10/copied_recursion.rusk"),
        "3 1\n2\n4 4\n",
    );
}
