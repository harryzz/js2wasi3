use anyhow::Context;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "js2wasi3", version, about = "Compile JavaScript to a WASI 0.3 Wasm component")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compile an entry module to a component.
    Build {
        entry: String,
        #[arg(short, long, default_value = "out.wasm")]
        out: String,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Build { entry, out } => {
            let source = std::fs::read_to_string(&entry).with_context(|| format!("reading {entry}"))?;
            let _program = js2wasi3_frontend::parse(&entry, &source)?;
            let ir = js2wasi3_ir::Module;
            let core = js2wasi3_backend::emit(&ir)?;
            let component = js2wasi3_component::package(&core)?;
            std::fs::write(&out, component).with_context(|| format!("writing {out}"))?;
        }
    }
    Ok(())
}
