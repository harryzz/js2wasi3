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
        /// Build a command (`wasi:cli/run`) even if the entry has exports, as node runs it: the
        /// exports are dropped.
        #[arg(long, conflicts_with = "wit")]
        command: bool,
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
    let mut ifaces: std::collections::HashMap<String, &component::WorldInterface> =
        Default::default();
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
        // `export const host = { … }` implements interface `host` (T20 slice 5).
        if !e.string_name
            && let Some(iface) = world.export_interfaces.iter().find(|i| camel_case(&i.name) == e.name)
        {
            ifaces.insert(e.name.clone(), iface);
            continue;
        }
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
    let mut boundary = js2wasi3_lower::Boundary::default();
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
        // Compound types go to the signature's table.
        let mut types = Vec::new();
        let site = ("[export]$root".to_string(), w.name.clone());
        let mut abi = |t: &component::Ty| match t {
            component::Ty::Scalar(v) => ty(*v),
            t => {
                types.push(wit_type(t, &ty, &site));
                ir::AbiType::Compound(types.len() as u32 - 1)
            }
        };
        let params = w.params.iter().map(|(_, t)| abi(t)).collect();
        let result = w.result.as_ref().map(&mut abi);
        let err = match &w.err {
            None => ir::ErrType::Empty,
            Some(t) => ir::ErrType::Of(abi(t)),
        };
        boundary.exports.insert(
            e.name.clone(),
            js2wasi3_lower::WorldSig {
                params,
                result,
                throws: w.throws,
                is_async: w.is_async,
                err,
                types,
            },
        );
        names.insert(e.name.clone(), w.name.clone());
    }
    // Exported interfaces (T20 slice 5): their functions are `iface.fn` in the lowering.
    for iface in &world.export_interfaces {
        let js = camel_case(&iface.name);
        if !ifaces.contains_key(&js) {
            diags.push(diag(
                None,
                format!(
                    "world `{}` exports interface `{}`, and the module has no `export const {js} = {{ … }}` for it",
                    world.name, iface.module
                ),
                None,
            ));
            continue;
        }
        for (name, sig) in &iface.functions {
            let w = match sig {
                Ok(w) => w,
                Err(why) => {
                    diags.push(diag(None, format!("interface `{}`: {why}", iface.module), None));
                    continue;
                }
            };
            let mut types = Vec::new();
            let site = (format!("[export]{}", iface.module), name.clone());
            let mut abi = |t: &component::Ty| match t {
                component::Ty::Scalar(v) => ty(*v),
                t => {
                    types.push(wit_type(t, &ty, &site));
                    ir::AbiType::Compound(types.len() as u32 - 1)
                }
            };
            let params = w.params.iter().map(|(_, t)| abi(t)).collect();
            let result = w.result.as_ref().map(&mut abi);
            let err = match &w.err {
                None => ir::ErrType::Empty,
                Some(t) => ir::ErrType::Of(abi(t)),
            };
            let key = format!("{js}.{}", camel_case(name));
            boundary.exports.insert(
                key.clone(),
                js2wasi3_lower::WorldSig {
                    params,
                    result,
                    throws: w.throws,
                    is_async: w.is_async,
                    err,
                    types,
                },
            );
            names.insert(key, format!("{}#{name}", iface.module));
        }
    }
    // Imported interfaces (T20 slice 5), by name with and without the version.
    for iface in &world.imports {
        let mut funcs = Vec::new();
        for (name, sig) in &iface.functions {
            let mut types = Vec::new();
            let site = (iface.module.clone(), name.clone());
            let mut abi = |t: &component::Ty| match t {
                component::Ty::Scalar(v) => ty(*v),
                t => {
                    types.push(wit_type(t, &ty, &site));
                    ir::AbiType::Compound(types.len() as u32 - 1)
                }
            };
            let (params, result, throws, is_async, signature, unsupported) = match sig {
                Ok(w) => {
                    // The whole WIT result: the stub unwraps a `result`.
                    let whole = if w.throws {
                        Some(component::Ty::Result {
                            ok: w.result.clone().map(Box::new),
                            err: w.err.clone().map(Box::new),
                        })
                    } else {
                        w.result.clone()
                    };
                    (
                        w.params.iter().map(|(_, t)| abi(t)).collect(),
                        whole.as_ref().map(&mut abi),
                        w.throws,
                        w.is_async,
                        w.signature.clone(),
                        None,
                    )
                }
                Err(why) => (Vec::new(), None, false, false, String::new(), Some(why.clone())),
            };
            funcs.push(js2wasi3_lower::WorldImport {
                js_name: camel_case(name),
                wit_name: name.clone(),
                throws,
                is_async,
                import: ir::Import {
                    module: iface.module.clone(),
                    name: name.clone(),
                    sig: ir::AbiSig {
                        params,
                        result,
                        is_async,
                        types,
                        ..ir::AbiSig::default()
                    },
                    unwrap: None,
                },
                signature,
                unsupported,
            });
        }
        if let Some((plain, _)) = iface.module.split_once('@') {
            boundary.imports.insert(plain.to_string(), funcs.clone());
        }
        boundary.imports.insert(iface.module.clone(), funcs);
    }
    if !diags.is_empty() {
        return Err(frontend::Diagnostics(diags).into());
    }
    Ok((boundary, names))
}

/// A compound WIT type as the IR describes it: record fields and flags get jco's camelCase JS
/// keys (`max-size` → `maxSize`).
/// `site`: the core module and function name a `future`'s or `stream`'s intrinsics are imported with.
fn wit_type(
    t: &component::Ty,
    scalar: &dyn Fn(component::ValType) -> ir::AbiType,
    site: &(String, String),
) -> ir::WitType {
    use component::Ty;
    let sub = |t: &Ty| wit_type(t, scalar, site);
    match t {
        Ty::Scalar(v) => ir::WitType::Scalar(scalar(*v)),
        Ty::Option(t) => ir::WitType::Option(Box::new(sub(t))),
        Ty::Result { ok, err } => ir::WitType::Result {
            ok: ok.as_deref().map(|t| Box::new(sub(t))),
            err: err.as_deref().map(|t| Box::new(sub(t))),
        },
        Ty::Tuple(ts) => ir::WitType::Tuple(ts.iter().map(sub).collect()),
        Ty::Record(fs) => {
            ir::WitType::Record(fs.iter().map(|(n, t)| (camel_case(n), sub(t))).collect())
        }
        Ty::Enum(cs) => ir::WitType::Enum(cs.clone()),
        Ty::Flags(fs) => ir::WitType::Flags(fs.iter().map(|f| camel_case(f)).collect()),
        Ty::Variant(cs) => ir::WitType::Variant(
            cs.iter()
                .map(|(n, t)| (n.clone(), t.as_ref().map(sub)))
                .collect(),
        ),
        Ty::List(t) => ir::WitType::List(Box::new(sub(t))),
        Ty::Future { elem, index } => ir::WitType::Future(Box::new(ir::Payload {
            module: site.0.clone(),
            func: site.1.clone(),
            index: *index,
            elem: elem.as_deref().map(sub),
            stream: false,
        })),
        Ty::Stream { elem, index } => ir::WitType::Stream(Box::new(ir::Payload {
            module: site.0.clone(),
            func: site.1.clone(),
            index: *index,
            elem: Some(sub(elem)),
            stream: true,
        })),
    }
}

/// `max-size` → `maxSize`, `HTTP-code` → `httpCode`.
fn camel_case(wit: &str) -> String {
    let mut out = String::new();
    for (i, w) in wit.split('-').enumerate() {
        let w = w.to_lowercase();
        if i == 0 {
            out.push_str(&w);
        } else {
            let mut cs = w.chars();
            if let Some(c) = cs.next() {
                out.extend(c.to_uppercase());
                out.push_str(cs.as_str());
            }
        }
    }
    out
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
            let program =
                js2wasi3_lower::parse_entry(&alloc, &file, &source, false, &mut |p| std::fs::read_to_string(p))?;
            print!("{}", js2wasi3_lower::lower(&program)?);
        }
        Cmd::Build {
            entry,
            out,
            core,
            world,
            wit,
            command,
        } => {
            let source = read(&entry)?;
            let program =
                js2wasi3_lower::parse_entry(&alloc, &entry, &source, command, &mut |p| std::fs::read_to_string(p))?;
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
