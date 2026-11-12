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
fn captured_ordering_works_through_native_sorting() {
    succeeds(
        r#"
fn compare_fields(a:Int,b:Int)->Ordering{return a<=>b;}
{
    let comparisons:=0;
    struct Item{value:Int;}
    fn __cmp__(a:Item,b:Item)->Ordering{comparisons+=1;return compare_fields(a.value,b.value);}
    let values:=[Item{value:2},Item{value:1}];
    values.sort();
    wt comparisons,values[0].value,values[1].value;
}
"#,
        "1 1 2\n",
    );
}

#[test]
fn native_references_pass_capture_arguments_for_every_return_abi() {
    succeeds(
        r#"
extern fn calls<T>(value:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> value,Loc loc){auto a=@{call source(T)->Int}(value,loc);auto b=@{boxed_call source(T)->Int}(value,loc);auto c=@{value_call source(T)->Int}(value,loc);auto d=@{owned_call source(T)->Int}(value,loc);auto e=@{rooted_call source(T)->Int}(value,loc);return make_obj<@{Int}>(*a+*b+c+*d+*e);}",
}
extern fn units<T>(value:T) {
    code:"void @{fn}(Obj<@{T}> value,Loc loc){@{call tick(T)->Unit}(value,loc);auto a=@{boxed_call tick(T)->Unit}(value,loc);@{value_call tick(T)->Unit}(value,loc);auto b=@{owned_call tick(T)->Unit}(value,loc);auto c=@{rooted_call tick(T)->Unit}(value,loc);}",
}
{
    let count:=3;
    struct Item{value:Int;}
    fn source(unused:Item)->Int {return count;}
    fn tick(unused:Item){count+=1;}
    wt calls(Item{value:0});
    units(Item{value:0});
    wt count;
}
"#,
        "15\n8\n",
    );
}

#[test]
fn nested_native_calls_restore_the_callers_capture_environment() {
    succeeds(
        r#"
extern fn drive<T>(value:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> value,Loc loc){auto a=*@{call step(T)->Int}(value,loc);auto b=*@{call step(T)->Int}(value,loc);return make_obj<@{Int}>(a*100+b);}",
}
fn run(start:Int)->Int {
    let count:=start;
    struct Item{value:Int;}
    fn step(unused:Item)->Int {
        count+=1;
        if count==1 {wt run(10);}
        return count;
    }
    return drive(Item{value:0});
}
wt run(0);
"#,
        "1112\n102\n",
    );
}

#[test]
fn native_environment_requirements_are_checked_at_the_source_call() {
    let declaration = r#"
extern fn drive<T>(value:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> value,Loc loc){return @{call step(T)->Int}(value,loc);}",
}
"#;
    rejects(
        &format!(
            "{declaration}{{let count:=0;struct Item{{value:Int;}}drive(Item{{value:0}});fn step(x:Item)->Int{{return count;}}}}"
        ),
        "not ready",
    );
    rejects(
        &format!(
            "{declaration}let item:={{let count:=0;struct Item{{value:Int;}}fn step(x:Item)->Int{{return count;}}Item{{value:0}}}};drive(item);"
        ),
        "capture environment",
    );
}

#[test]
fn explicit_extern_guarantees_still_apply_to_the_source_arguments() {
    succeeds(
        r#"
extern fn drive<T>(readonly noescape value:T)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{T}> value,Loc loc){return @{call step(T)->Int}(value,loc);}",
}
fn inspect(readonly noescape x:Int)->Int {
    let offset:=1;
    struct Item{value:Int;}
    fn step(item:Item)->Int{return item.value+offset;}
    return drive(Item{value:x});
}
wt inspect(7);
"#,
        "8\n",
    );
}

#[test]
fn composed_builtin_contracts_account_for_hidden_capture_parameters() {
    succeeds(
        r#"
fn order_int(a:Int,b:Int)->Ordering{return a<=>b;}
fn check(readonly noescape direction:Int)->Bool {
    struct Item{value:Int;}
    fn __cmp__(a:Item,b:Item)->Ordering {
        if direction>0{return order_int(a.value,b.value);}
        return order_int(b.value,a.value);
    }
    return __total_less__::<Item>(Item{value:1},Item{value:2});
}
wt check(1),check(-1);
"#,
        "Yes No\n",
    );
}

#[test]
fn native_capture_example_preserves_copies_and_character_owner_lifetimes() {
    succeeds(
        include_str!("../examples/beta10/native_captures.rusk"),
        "1 102 2\n3 3\nZ a\nZb!\n1112\n102\n",
    );
}

#[test]
fn native_type_function_references_use_the_environment_of_each_operation() {
    succeeds(
        r#"
extern symbol policies {
    code:"template<class T,auto Evaluate>struct NativePolicy{Obj<T> value;std::int64_t run(Loc loc)const{return Evaluate(value,loc);}};",
}
extern type<T> Policy {cpp_name:"NativePolicy<{T},@{value_call evaluate(T)->Int}>",}
extern fn policy<T>(value:T)->Policy<T> {
    code:"Obj<@{Policy<T>}> @{fn}(Obj<@{T}> value,Loc){return make_obj<@{Policy<T>}>(@{Policy<T>}{value});}",
}
extern fn run<T>(value:Policy<T>)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{Policy<T>}> value,Loc loc){return make_obj<@{Int}>(value->run(loc));}",
}
extern fn run_first<T>(values:Vec<Policy<T>>)->Int {
    code:"Obj<@{Int}> @{fn}(Obj<@{Vec<Policy<T>>}> values,Loc loc){return make_obj<@{Int}>((*values)[0]->run(loc));}",
}
{
    let count:=10;
    struct Item{value:Int;}
    fn evaluate(item:Item)->Int{count+=item.value;return count;}
    let stored:=policy(Item{value:2});
    wt run(stored),run_first([stored]),count;
}
"#,
        "12 14 14\n",
    );
}

#[test]
fn derived_record_and_enum_operations_forward_environments_to_field_copy_hooks() {
    succeeds(
        r#"
struct Holder<T>{value:T;}
fn clone_value<T>(value:T)->T{return copy(value);}
fn assign_value<T>(a:T,b:T){a=b;}
{
    let calls:=0;
    struct Cell{value:Int;}
    fn copy(value:Cell)->Cell{calls+=1;return Cell{value:value.value};}
    let holder:=Holder::<Cell>{value:Cell{value:7}};
    let detached:=clone_value(holder);
    let optional:=Some(Cell{value:9});
    let detached_optional:=clone_value(optional);
    assign_value(holder,detached);
    assign_value(optional,detached_optional);
    wt calls,detached.value.value,detached_optional?!.value;
}
"#,
        "4 7 9\n",
    );
}
