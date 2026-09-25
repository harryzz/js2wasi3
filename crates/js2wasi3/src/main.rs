use anyhow::Context;
use clap::{Parser, Subcommand};
use std::path::Path;

use js2wasi3_component as component;
use js2wasi3_frontend::{self as frontend, Allocator};
use js2wasi3_ir as ir;

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
        /// WIT world of the component, as <namespace>:<package>/<world>.
        /// Default: js2wasi3:<name>/<name>, from the entry file name.
        #[arg(long)]
        world: Option<String>,
    },
}

/// Map the IR exports to a WIT world. Naming errors come back as source diagnostics.
fn plan_world(
    path: &str,
    ir: &ir::Module,
    name: component::WorldName,
) -> anyhow::Result<component::World> {
    let ty = |t: ir::AbiType| match t {
        ir::AbiType::F64 => component::ValType::F64,
        ir::AbiType::String => component::ValType::String,
    };
    let mut sigs = Vec::new();
    for e in &ir.exports {
        let f = ir.function(e.func);
        let pos = e.pos.unwrap_or(ir::Pos { line: 1, col: 1 });
        sigs.push(component::ExportSig {
            name: e.name.clone(),
            string_name: e.string_name,
            // A converting wrapper is named `<local>$export`.
            local: f.name.trim_end_matches("$export").to_string(),
            params: f.locals[..f.params as usize]
                .iter()
                .zip(&e.sig.params)
                .map(|(l, t)| (l.name.clone(), ty(*t)))
                .collect(),
            result: e.sig.result.map(ty),
            line: pos.line,
            col: pos.col,
        });
    }
    component::world(name, &sigs).map_err(|errs| {
        let diags = errs
            .into_iter()
            .map(|e| frontend::Diagnostic {
                path: path.to_string(),
                line: e.line,
                col: e.col,
                message: e.message,
                help: e.help,
            })
            .collect();
        frontend::Diagnostics(diags).into()
    })
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
                let name = if e.string_name {
                    format!("{:?}", e.name)
                } else {
                    e.name.clone()
                };
                println!("{line}:{col}\texport {name}{local}{from}");
            }
        }
        Cmd::Ir { file } => {
            let source = read(&file)?;
            let program = frontend::parse(&alloc, &file, &source)?;
            print!("{}", js2wasi3_lower::lower(&program)?);
        }
        Cmd::Build {
            entry,
            out,
            core,
            world,
        } => {
            let world = match world {
                Some(w) => component::WorldName::parse(&w)?,
                None => component::WorldName::from_path(Path::new(&entry)),
            };
            let source = read(&entry)?;
            let program = frontend::parse(&alloc, &entry, &source)?;
            let mut ir = js2wasi3_lower::lower(&program)?;
            let bytes = if core {
                js2wasi3_backend::emit(&ir)?
            } else {
                let world = if ir.main.is_some() {
                    component::command_world(world)
                } else {
                    plan_world(&entry, &ir, world)?
                };
                // The core module must export each function under its WIT name.
                for (e, name) in ir.exports.iter_mut().zip(&world.export_names) {
                    e.name = name.clone();
                }
                let module = js2wasi3_backend::emit(&ir)?;
                component::package(&module, &world)?
            };
            std::fs::write(&out, bytes).with_context(|| format!("writing {out}"))?;
        }
    }
    Ok(())
}
