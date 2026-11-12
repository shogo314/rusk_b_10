//! Adapt the explicit Rusk environment ABI to native functions' source ABI.
//! The stack binding is per thread and per declaration slot. It owns no data:
//! the wrapper's explicit arguments keep objects alive during the native call.
use super::program::{Context, signature};
use crate::ir::lir::{Program, ProgramFunction};

pub(super) fn parameters<'a>(f: &'a ProgramFunction, context: &Context) -> &'a [String] {
    &f.params[..f.params.len() - context.functions[f.id].captures.len()]
}

pub(super) fn arguments(
    f: &ProgramFunction,
    context: &Context,
    mut args: Vec<String>,
) -> Vec<String> {
    let count = parameters(f, context).len();
    for (index, (function, slot)) in context.functions[f.id].captures.iter().enumerate() {
        args.push(format!(
            "RuskNativeCapture<{function},{slot},{}>::get()",
            context.types[&f.params[count + index]]
        ));
    }
    args
}

pub(super) fn prelude(context: &Context) -> String {
    if context.functions.iter().all(|f| f.captures.is_empty()) {
        return String::new();
    }
    r#"
template<std::size_t Function, std::size_t Slot, class T> struct RuskNativeCapture {
    inline static thread_local const Obj<T>* current = nullptr;
    static const Obj<T>& get() { if (!current) std::abort(); return *current; }
};
template<std::size_t Function, std::size_t Slot, class T> struct RuskNativeCaptureBinding {
    const Obj<T>* previous;
    explicit RuskNativeCaptureBinding(const Obj<T>& value)
        : previous(RuskNativeCapture<Function,Slot,T>::current) {
        RuskNativeCapture<Function,Slot,T>::current = &value;
    }
    ~RuskNativeCaptureBinding() { RuskNativeCapture<Function,Slot,T>::current = previous; }
    RuskNativeCaptureBinding(const RuskNativeCaptureBinding&) = delete;
    RuskNativeCaptureBinding& operator=(const RuskNativeCaptureBinding&) = delete;
};
"#
    .into()
}

fn reference_signature(f: &ProgramFunction, context: &Context, object: bool) -> String {
    let result = if f.result == "Unit" && !object {
        "void".into()
    } else {
        format!("Obj<{}>", context.types[&f.result])
    };
    let mut params = parameters(f, context)
        .iter()
        .enumerate()
        .map(|(i, ty)| format!("const Obj<{}>& rusk_a{i}", context.types[ty]))
        .collect::<Vec<_>>();
    params.push("[[maybe_unused]] Loc rusk_loc".into());
    format!(
        "{result} rusk_capture_reference_{}_{object}({})",
        f.id,
        params.join(",")
    )
}

pub(super) fn declarations(program: &Program, context: &Context) -> String {
    let mut out = String::new();
    for f in &program.functions {
        if let Some(raw) = &context.functions[f.id].native_entry
            && (context.native_declarations.contains(&f.id)
                || raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
        {
            // Build a separate source-ABI declaration. The original entry can
            // be overloaded, while the generated environment wrapper is unique.
            let result = if f.result == "Unit" && !context.functions[f.id].unit_object {
                "void".into()
            } else {
                format!("Obj<{}>", context.types[&f.result])
            };
            let mut params = parameters(f, context)
                .iter()
                .map(|ty| format!("Obj<{}>", context.types[ty]))
                .collect::<Vec<_>>();
            params.push("Loc".into());
            out += &format!("{result} {raw}({})", params.join(","));
            out += ";\n";
        }
    }
    for &(id, object) in &context.native_direct_references {
        if !context.functions[id].captures.is_empty() {
            out += &reference_signature(&program.functions[id], context, object);
            out += ";\n";
        }
    }
    out
}

pub(super) fn definitions(program: &Program, context: &Context, optimize: bool) -> String {
    let mut out = String::new();
    for f in &program.functions {
        let info = &context.functions[f.id];
        let Some(raw) = &info.native_entry else {
            continue;
        };
        let count = parameters(f, context).len();
        out += &signature(f, context, optimize);
        out += "{\n";
        for (index, (function, slot)) in info.captures.iter().enumerate() {
            out += &format!(
                "RuskNativeCaptureBinding<{function},{slot},{}> rusk_binding{index}(rusk_p{});\n",
                context.types[&f.params[count + index]],
                count + index
            );
        }
        let mut args = (0..count).map(|i| format!("rusk_p{i}")).collect::<Vec<_>>();
        args.push("rusk_call_loc".into());
        out += &format!("return {raw}({});\n}}\n", args.join(","));
    }
    for &(id, object) in &context.native_direct_references {
        let f = &program.functions[id];
        let info = &context.functions[id];
        if info.captures.is_empty() {
            continue;
        }
        out += &reference_signature(f, context, object);
        let args = (0..parameters(f, context).len())
            .map(|i| format!("rusk_a{i}"))
            .collect();
        let mut args = arguments(f, context, args);
        args.push("rusk_loc".into());
        let call = format!("{}({})", info.cpp, args.join(","));
        if f.result == "Unit" && object != info.unit_object {
            if object {
                out += &format!(
                    "{{{call};return make_obj<{}>({}{{}});}}\n",
                    context.types["Unit"], context.types["Unit"]
                );
            } else {
                out += &format!("{{static_cast<void>({call});}}\n");
            }
        } else {
            out += &format!("{{return {call};}}\n");
        }
    }
    out
}
