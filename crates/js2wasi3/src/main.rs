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
        /// With --wit: the world to use from those files (a name or <namespace>:<package>/<world>).
        #[arg(long)]
        world: Option<String>,
        /// A WIT file or directory with the world the module implements (instead of generating
        /// one from the exports).
        #[arg(long)]
        wit: Option<String>,
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
        // Generated worlds use only these two.
        t => unreachable!("{t:?} in a generated world"),
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
            throws: e.sig.throws,
            is_async: e.sig.is_async,
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

/// Match the module's exports with a user world's (T20): every world export needs a JS export
/// with its WIT name (converted like a generated world's, or a string export name), and every
/// JS export must be in the world. Returns the signatures by JS name and the WIT name of each.
fn match_world(
    program: &frontend::Program,
    world: &component::UserWorld,
) -> anyhow::Result<(
    js2wasi3_lower::Boundary,
    std::collections::HashMap<String, String>,
)> {
    let ty = |t: component::ValType| {
        use component::ValType as V;
        use ir::AbiType as A;
        match t {
            V::Bool => A::Bool,
            V::S8 => A::S8,
            V::U8 => A::U8,
            V::S16 => A::S16,
            V::U16 => A::U16,
            V::S32 => A::S32,
            V::U32 => A::U32,
            V::S64 => A::S64,
            V::U64 => A::U64,
            V::F32 => A::F32,
            V::F64 => A::F64,
            V::Char => A::Char,
            V::String => A::String,
        }
    };
    let mut diags = Vec::new();
    let diag = |e: Option<&frontend::Export>, message: String, help: Option<String>| {
        let (line, col) = e.map_or((1, 1), |e| program.line_col(e.span.start));
        frontend::Diagnostic {
            path: program.path.clone(),
            line,
            col,
            message,
            help,
        }
    };
    // WIT name → JS export.
    let mut by_wit: std::collections::HashMap<String, &frontend::Export> = Default::default();
    for e in &program.exports {
        if e.from.is_some() || e.local.is_none() {
            continue; // reported by the lowering
        }
        let wit = if e.string_name {
            e.name.clone()
        } else {
            match component::names::to_wit(&e.name) {
                Ok(w) => w,
                Err(why) => {
                    diags.push(diag(
                        Some(e),
                        format!("export `{}` has no WIT name: {why}", e.name),
                        None,
                    ));
                    continue;
                }
            }
        };
        if !world.exports.iter().any(|w| w.name == wit) {
            diags.push(diag(
                Some(e),
                format!(
                    "export `{}` (WIT name `{wit}`) isn't in world `{}`",
                    e.name, world.name
                ),
                None,
            ));
            continue;
        }
        by_wit.insert(wit, e);
    }
    let mut boundary = js2wasi3_lower::Boundary::new();
    let mut names = std::collections::HashMap::new();
    for w in &world.exports {
        let Some(e) = by_wit.get(&w.name) else {
            diags.push(diag(
                None,
                format!(
                    "world `{}` exports `{}: {}`, and the module has no export for it",
                    world.name, w.name, w.signature
                ),
                Some(format!(
                    "export a function whose name converts to `{}`, or `export {{ f as \"{}\" }}`",
                    w.name, w.name
                )),
            ));
            continue;
        };
        boundary.insert(
            e.name.clone(),
            js2wasi3_lower::WorldSig {
                params: w.params.iter().map(|(_, t)| ty(*t)).collect(),
                result: w.result.map(ty),
                throws: w.throws,
                is_async: w.is_async,
            },
        );
        names.insert(e.name.clone(), w.name.clone());
    }
    if !diags.is_empty() {
        return Err(frontend::Diagnostics(diags).into());
    }
    Ok((boundary, names))
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
            let program = js2wasi3_lower::parse(&alloc, &file, &source)?;
            print!("{}", js2wasi3_lower::lower(&program)?);
        }
        Cmd::Build {
            entry,
            out,
            core,
            world,
            wit,
        } => {
            let source = read(&entry)?;
            let program = js2wasi3_lower::parse(&alloc, &entry, &source)?;
            if let Some(wit) = wit {
                if program.exports.is_empty() {
                    anyhow::bail!(
                        "--wit: {entry} has no exports, so it's a command (its world is wasi:cli/command)"
                    );
                }
                let user = component::load_world(Path::new(&wit), world.as_deref())?;
                let (boundary, names) = match_world(&program, &user)?;
                let mut ir = js2wasi3_lower::lower_with(&program, Some(&boundary))?;
                for e in &mut ir.exports {
                    e.name = names[&e.name].clone();
                }
                let module = js2wasi3_backend::emit(&ir)?;
                let bytes = if core {
                    module
                } else {
                    let world = component::World {
                        name: component::WorldName::from_path(Path::new(&entry)),
                        export_names: Vec::new(),
                        wit: String::new(),
                        user: Some((user.resolve, user.id)),
                    };
                    let timers = ["$timers", "$immediates", "$ticks"]
                        .iter()
                        .any(|n| ir.names.contains(*n));
                    component::package(&module, &world).with_context(|| {
                        if timers {
                            "the module uses timers: its world must `import wasi:clocks/monotonic-clock@0.3.1;`"
                        } else {
                            "packaging the component"
                        }
                    })?
                };
                std::fs::write(&out, bytes).with_context(|| format!("writing {out}"))?;
                return Ok(());
            }
            let world = match world {
                Some(w) => component::WorldName::parse(&w)?,
                None => component::WorldName::from_path(Path::new(&entry)),
            };
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
