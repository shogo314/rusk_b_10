mod analysis;
mod api;
mod backend;
mod cli;
mod compiler;
mod diagnostics;
mod format;
mod frontend;
mod ir;
mod native;
mod stdlib;

use clap::Parser;
use cli::{Cli, Command, CompileArgs, EmitStage};
use diagnostics::{Stage, enter};
use frontend::{parse_cst, source};
use std::io::Write;

fn run(command: Command) -> Result<(), String> {
    match command {
        Command::Compile(args) => compile(args),
        Command::Fmt(args) => format::run(args),
        Command::Search(args) => api::search(args),
        Command::Doc(args) => api::doc(args),
        Command::Dependencies { file } => {
            let dependencies = {
                let _stage = enter(Stage::SourceLoading);
                source::load(&file)?.dependencies
            };
            let _stage = enter(Stage::Output);
            let mut stdout = std::io::stdout().lock();
            for path in dependencies {
                writeln!(stdout, "{}", path.display()).map_err(|e| format!("stdout: {e}"))?;
            }
            Ok(())
        }
    }
}

fn compile(args: CompileArgs) -> Result<(), String> {
    let _stage = enter(Stage::SourceLoading);
    let CompileArgs {
        file,
        output,
        no_opt,
        emit,
    } = args;
    if matches!(emit, EmitStage::Cst | EmitStage::Ast) {
        let result = if matches!(emit, EmitStage::Cst) {
            let text =
                std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            format!("{:#?}\n", parse_cst(&text, &file.to_string_lossy())?)
        } else {
            format!("{:#?}\n", source::load(&file)?.statements)
        };
        let _stage = enter(Stage::Output);
        std::fs::write(&output, result).map_err(|e| format!("{}: {e}", output.display()))?;
        return Ok(());
    }
    let emit = match emit {
        EmitStage::Hir => backend::Emit::Hir,
        EmitStage::Lir => backend::Emit::Lir,
        EmitStage::Cpp => backend::Emit::Cpp,
        EmitStage::Cst | EmitStage::Ast => unreachable!(),
    };
    let source = source::load(&file)?.statements;
    let cpp = compiler::Compiler::with_optimization(!no_opt)
        .compile(&[stdlib::load()?, compiler::Source::user(source)], emit)?;
    let _stage = enter(Stage::Output);
    std::fs::write(&output, cpp).map_err(|e| format!("{}: {e}", output.display()))?;
    Ok(())
}
fn main() {
    diagnostics::install_panic_hook();
    let command = {
        let _stage = enter(Stage::Arguments);
        let command = Cli::parse().command;
        if let Command::Fmt(args) = &command {
            args.validate().unwrap_or_else(|error| error.exit());
        }
        command
    };
    if let Err(e) = run(command) {
        let _ = writeln!(std::io::stderr().lock(), "{e}");
        std::process::exit(1);
    }
}
