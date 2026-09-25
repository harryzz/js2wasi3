use anyhow::Context;
use clap::{Parser, Subcommand};
use js2wasi3_frontend::{self as frontend, Allocator};

#[derive(Parser)]
#[command(
    name = "js2wasi3",
    version,
    about = "Compile JavaScript to a WASI 0.3 Wasm component"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Parse and check a module; print its top-level statements and exports.
    Parse { file: String },
    /// Lower a module to IR and print it.
    Ir { file: String },
    /// Compile an entry module to a component.
    Build {
        entry: String,
        #[arg(short, long, default_value = "out.wasm")]
        out: String,
        /// Write the core Wasm module only (no component wrapping).
        #[arg(long)]
        core: bool,
    },
}

fn read(path: &str) -> anyhow::Result<String> {
    std::fs::read_to_string(path).with_context(|| format!("reading {path}"))
}

fn main() -> anyhow::Result<()> {
    let alloc = Allocator::default();
    match Cli::parse().cmd {
        Cmd::Parse { file } => {
            let source = read(&file)?;
            let program = frontend::parse(&alloc, &file, &source)?;
            for (line, col, kind) in program.top_level() {
                println!("{line}:{col}\t{kind}");
            }
            for e in &program.exports {
                let (line, col) = program.line_col(e.span.start);
                let local = e
                    .local
                    .as_deref()
                    .map(|l| format!(" (local {l})"))
                    .unwrap_or_default();
                let from = e
                    .from
                    .as_deref()
                    .map(|f| format!(" from {f:?}"))
                    .unwrap_or_default();
                println!("{line}:{col}\texport {}{local}{from}", e.name);
            }
        }
        Cmd::Ir { file } => {
            let source = read(&file)?;
            let program = frontend::parse(&alloc, &file, &source)?;
            print!("{}", js2wasi3_lower::lower(&program)?);
        }
        Cmd::Build { entry, out, core } => {
            let source = read(&entry)?;
            let program = frontend::parse(&alloc, &entry, &source)?;
            let ir = js2wasi3_lower::lower(&program)?;
            let module = js2wasi3_backend::emit(&ir)?;
            let bytes = if core {
                module
            } else {
                js2wasi3_component::package(&module)?
            };
            std::fs::write(&out, bytes).with_context(|| format!("writing {out}"))?;
        }
    }
    Ok(())
}
