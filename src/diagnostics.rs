//! Process-level internal error reporting. User errors continue through Result.
use std::{
    backtrace::{Backtrace, BacktraceStatus},
    cell::Cell,
    io::Write,
    marker::PhantomData,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    Arguments,
    SourceLoading,
    Parsing,
    Formatting,
    Ast,
    StandardLibrary,
    Declarations,
    TypeChecking,
    Hir,
    HirValidation,
    Lir,
    LirValidation,
    Effects,
    Bindings,
    Readonly,
    Cpp,
    Output,
}

impl Stage {
    fn label(self) -> &'static str {
        match self {
            Self::Arguments => "command-line arguments",
            Self::SourceLoading => "source/resource loading",
            Self::Parsing => "syntax parsing",
            Self::Formatting => "source formatting",
            Self::Ast => "AST construction",
            Self::StandardLibrary => "standard library loading",
            Self::Declarations => "declaration registration",
            Self::TypeChecking => "type checking and instantiation",
            Self::Hir => "HIR construction",
            Self::HirValidation => "HIR validation",
            Self::Lir => "HIR to LIR lowering",
            Self::LirValidation => "LIR validation",
            Self::Effects => "effect and escape analysis",
            Self::Bindings => "fixed binding analysis",
            Self::Readonly => "readonly analysis",
            Self::Cpp => "C++ generation",
            Self::Output => "output writing",
        }
    }
}

thread_local! {
    static CURRENT_STAGE: Cell<Option<Stage>> = const { Cell::new(None) };
}

// A guard must be dropped on the thread whose context it changed.
#[must_use]
pub(crate) struct StageGuard {
    previous: Option<Stage>,
    _thread: PhantomData<Rc<()>>,
}

pub(crate) fn enter(stage: Stage) -> StageGuard {
    StageGuard {
        previous: CURRENT_STAGE.with(|current| current.replace(Some(stage))),
        _thread: PhantomData,
    }
}

impl Drop for StageGuard {
    fn drop(&mut self) {
        let _ = CURRENT_STAGE.try_with(|current| current.set(self.previous));
    }
}

pub(crate) fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let stage = CURRENT_STAGE
            .try_with(Cell::get)
            .ok()
            .flatten()
            .map(Stage::label)
            .unwrap_or("unknown");
        let reason = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("non-string panic payload");
        // Reporting a panic must not cause another panic if stderr is closed.
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "error: Rusk internal compiler error");
        let _ = writeln!(stderr, "version: {}", env!("CARGO_PKG_VERSION"));
        let _ = writeln!(stderr, "stage: {stage}");
        let _ = writeln!(stderr, "reason: {reason}");
        if let Some(location) = info.location() {
            let _ = writeln!(stderr, "Rust location: {location}");
        }
        let backtrace = Backtrace::capture();
        if backtrace.status() == BacktraceStatus::Captured {
            let _ = writeln!(stderr, "backtrace:\n{backtrace}");
        } else {
            let _ = writeln!(stderr, "Set RUST_BACKTRACE=1 to include a backtrace.");
        }
        let _ = writeln!(
            stderr,
            "Please report this as a Rusk bug with the diagnostic and a reproducing source."
        );
    }));
}

#[cfg(test)]
mod tests;
