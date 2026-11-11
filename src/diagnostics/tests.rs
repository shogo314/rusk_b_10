use super::*;
use std::process::Command;

#[test]
fn stage_context_is_nested_thread_local_and_restored_on_unwind() {
    assert_eq!(CURRENT_STAGE.with(Cell::get), None);
    let outer = enter(Stage::SourceLoading);
    {
        let _inner = enter(Stage::Parsing);
        assert_eq!(CURRENT_STAGE.with(Cell::get), Some(Stage::Parsing));
        assert_eq!(
            std::thread::spawn(|| CURRENT_STAGE.with(Cell::get))
                .join()
                .unwrap(),
            None
        );
    }
    assert_eq!(CURRENT_STAGE.with(Cell::get), Some(Stage::SourceLoading));
    let failure = std::panic::catch_unwind(|| {
        let _inner = enter(Stage::Ast);
        panic!("test unwinding");
    });
    assert!(failure.is_err());
    assert_eq!(CURRENT_STAGE.with(Cell::get), Some(Stage::SourceLoading));
    drop(outer);
    assert_eq!(CURRENT_STAGE.with(Cell::get), None);
}

// Only this child-process fixture installs a hook; the parent test harness and
// concurrent tests retain Rust's standard hook. No production fault switch.
#[test]
fn panic_hook_child() {
    let Ok(kind) = std::env::var("RUSK_TEST_PANIC_HOOK_CHILD") else {
        return;
    };
    install_panic_hook();
    let _stage = enter(Stage::Cpp);
    match kind.as_str() {
        "string" => panic!("synthetic invariant violation"),
        "owned" => std::panic::panic_any(String::from("owned panic reason")),
        "other" => std::panic::panic_any(17_u32),
        "unknown" => {
            drop(_stage);
            panic!("without a stage");
        }
        "hir" => {
            use crate::ir::hir::{Body, Instruction, Program, Rvalue, Value, ValueId};
            Program {
                types: Default::default(),
                functions: vec![],
                main: Body {
                    instructions: vec![Instruction::Object {
                        result: Value {
                            id: ValueId::Local(0),
                            ty: "Missing".into(),
                        },
                        expression: Rvalue::Variant {
                            index: 0,
                            fields: vec![],
                        },
                    }],
                },
            }
            .assert_valid();
        }
        "lir" => {
            use crate::ir::lir::{Function, Program};
            Program {
                types: Default::default(),
                functions: vec![],
                main: Function {
                    slots: vec![],
                    blocks: vec![],
                },
            }
            .assert_valid();
        }
        _ => panic!("unknown fixture"),
    }
}

#[test]
fn panic_hook_reports_context_and_optional_backtrace_in_a_child_process() {
    for (kind, reason, stage, trace) in [
        (
            "string",
            "synthetic invariant violation",
            "C++ generation",
            false,
        ),
        ("owned", "owned panic reason", "C++ generation", false),
        ("other", "non-string panic payload", "C++ generation", false),
        ("unknown", "without a stage", "unknown", false),
        (
            "string",
            "synthetic invariant violation",
            "C++ generation",
            true,
        ),
        ("hir", "HIR invariant violated:", "HIR validation", false),
        ("lir", "LIR invariant violated:", "LIR validation", false),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "diagnostics::tests::panic_hook_child",
                "--nocapture",
            ])
            .env("RUSK_TEST_PANIC_HOOK_CHILD", kind)
            .env("RUST_BACKTRACE", if trace { "1" } else { "0" })
            .env_remove("RUST_LIB_BACKTRACE")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        let error = String::from_utf8_lossy(&output.stderr);
        for expected in [
            "error: Rusk internal compiler error".to_string(),
            format!("version: {}", env!("CARGO_PKG_VERSION")),
            format!("stage: {stage}"),
            format!("reason: {reason}"),
            "Rust location:".to_string(),
            "Please report this as a Rusk bug".to_string(),
        ] {
            assert!(error.contains(&expected), "missing {expected}: {error}");
        }
        let location = match kind {
            "hir" => "hir/validate.rs:",
            "lir" => "lir/mod.rs:",
            _ => "diagnostics/tests.rs:",
        };
        assert!(error.contains(location), "{error}");
        if trace {
            assert!(error.contains("backtrace:\n"), "{error}");
            assert!(error.contains("panic_hook_child"), "{error}");
        } else {
            assert!(error.contains("Set RUST_BACKTRACE=1"), "{error}");
            assert!(!error.contains("backtrace:\n"), "{error}");
        }
    }
}
