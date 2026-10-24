mod compose;

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
    /// T131: the whole-program type analysis; one line per site: offset, kind, operand types.
    Types {
        file: String,
        /// Analyse the program as the compiler lowers it (prelude appended, modules linked).
        #[arg(long)]
        lowered: bool,
    },
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
        /// Modules `import()` may load whose specifiers the build can't work out (a glob relative
        /// to the working directory: `*` within a path segment, `**` any directories). Repeatable.
        #[arg(long, value_name = "GLOB")]
        include: Vec<String>,
        /// Link an external library's component import even where the program doesn't name its
        /// functions (`crypto`: `pbkdf2`, `scrypt`, `hkdf`; `zlib`: the deflate family; `brotli`; `zstd`); `build` composes the helper in.
        /// Repeatable.
        #[arg(long, value_name = "LIB", value_parser = ["crypto", "zlib", "brotli", "zstd"])]
        ext_lib: Vec<String>,
        /// What an unhandled promise rejection does, as node's flag: `throw` (the default: the
        /// program fails with the reason) or `none` (ignored; test262 hosts don't fail on them).
        #[arg(long, value_name = "MODE", default_value = "throw", value_parser = ["throw", "none"])]
        unhandled_rejections: String,
        /// MiB of GC heap to reserve at start (T98): a throwaway allocation grows wasmtime's heap
        /// once, so collections are rarer (wasmtime's default heap is small; a reservation costs
        /// about twice its size in memory). 0: none. As `wasmtime -O gc-heap-initial-size`, which
        /// still works on top.
        #[arg(long, value_name = "MIB", default_value_t = 16, value_parser = clap::value_parser!(u32).range(0..=1024))]
        gc_reserve: u32,
        /// Count the hits and misses of the fast paths and print them to stderr at exit (T112;
        /// for measuring, slower).
        #[arg(long)]
        count: bool,
        /// Run every regex in the prelude's VM, none compiled to Wasm (T114; for comparing the two).
        #[arg(long)]
        regex_vm: bool,
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
    let ty = abi_scalar;
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
    import_boundary(&mut boundary, &world.imports, false);
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

/// A WIT scalar's IR boundary type.
fn abi_scalar(t: component::ValType) -> ir::AbiType {
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
}

/// The imported interfaces' functions (T20 slice 5), by interface name with and without the version.
/// `blocking` (T59, a command's WASI imports): an `async func` is sync-lowered: the call blocks
/// until it returns, and gives its value (not a promise).
fn import_boundary(
    boundary: &mut js2wasi3_lower::Boundary,
    imports: &[component::WorldInterface],
    blocking: bool,
) {
    let ty = abi_scalar;
    for iface in imports {
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
            // A blocking call is sync-lowered; its streams are read and written to the end around
            // it, its returned futures read. A future argument would need the call to wait for a
            // value it can't get: not supported.
            let future_param = |t: &component::Ty| {
                fn has(t: &component::Ty) -> bool {
                    use component::Ty as T;
                    match t {
                        T::Future { .. } => true,
                        T::Option(t) | T::List(t) => has(t),
                        T::Result { ok, err } => [ok, err].into_iter().flatten().any(|t| has(t)),
                        T::Tuple(ts) => ts.iter().any(has),
                        T::Record(fs) => fs.iter().any(|(_, t)| has(t)),
                        T::Variant(cs) => cs.iter().flat_map(|(_, t)| t).any(has),
                        _ => false,
                    }
                }
                has(t)
            };
            let blocked =
                Err("unsupported: a future argument in a command's import (planned: later)".to_string());
            let sig = match sig {
                Ok(w) if blocking && w.params.iter().any(|(_, t)| future_param(t)) => &blocked,
                sig => sig,
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
            let is_async = is_async && !blocking;
            funcs.push(js2wasi3_lower::WorldImport {
                // Resource functions (`[method]r.f`, T59) are imported by their WIT names.
                js_name: if name.starts_with('[') {
                    name.clone()
                } else {
                    camel_case(name)
                },
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
                        blocking,
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
                js2wasi3_lower::parse_entry(&alloc, &file, &source, &Default::default(), &mut frontend::Fs)?;
            print!("{}", js2wasi3_lower::lower(&program)?);
        }
        Cmd::Types { file, lowered } => {
            let source = read(&file)?;
            let t = std::time::Instant::now();
            let r = if lowered {
                let program =
                    js2wasi3_lower::parse_entry(&alloc, &file, &source, &Default::default(), &mut frontend::Fs)?;
                js2wasi3_types::analyze(program.ast, &program.semantic, program.user_end)
            } else {
                js2wasi3_types::analyze_source(&source).map_err(|e| anyhow::anyhow!("{file}: {e}"))?
            };
            eprintln!(
                "types: {} sites, {} one-target method sites, {} one-constructor field sites, {} number-only keys, {} rounds, {} instances, {} allocations, {} steps, {} ms",
                r.sites.len(),
                r.methods.len(),
                r.fields.len(),
                r.num_fields.len(),
                r.rounds,
                r.instances,
                r.allocs,
                r.steps,
                t.elapsed().as_millis()
            );
            for s in r.sites {
                let j = |v: &Vec<String>| if v.is_empty() { "-".to_string() } else { v.join(",") };
                println!("{}:{}\t{}\t{}|{}\t{}|{}", s.offset, s.end, s.cat, s.groups.0, s.groups.1, j(&s.tags.0), j(&s.tags.1));
            }
        }
        Cmd::Build {
            entry,
            out,
            core,
            world,
            wit,
            command,
            include,
            ext_lib,
            unhandled_rejections,
            gc_reserve,
            count,
            regex_vm,
        } => {
            let ignore_unhandled = unhandled_rejections == "none";
            let source = read(&entry)?;
            let program =
                js2wasi3_lower::parse_entry(
                &alloc,
                &entry,
                &source,
                &frontend::Options { command, include, ext_libs: ext_lib },
                &mut frontend::Fs,
            )?;
            if let Some(wit) = wit {
                if program.exports.is_empty() {
                    anyhow::bail!(
                        "--wit: {entry} has no exports, so it's a command (its world is wasi:cli/command)"
                    );
                }
                let user = component::load_world(Path::new(&wit), world.as_deref())?;
                let (boundary, names) = match_world(&program, &user)?;
                let mut ir = js2wasi3_lower::lower_with(&program, Some(&boundary))?;
                ir.ignore_unhandled = ignore_unhandled;
                ir.gc_reserve_mib = gc_reserve;
                ir.count = count;
            ir.regex_vm = regex_vm;
                ir.regex_vm = regex_vm;
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
                let bytes = compose::helpers(bytes)?;
                std::fs::write(&out, bytes).with_context(|| format!("writing {out}"))?;
                return Ok(());
            }
            let world = match world {
                Some(w) => component::WorldName::parse(&w)?,
                None => component::WorldName::from_path(Path::new(&entry)),
            };
            // A command may import WASI interfaces directly (T59).
            // Only the interfaces it names: a boundary's types decide some of the runtime.
            let named = program.import_sources();
            let wasi = if (command || program.exports.is_empty()) && !named.is_empty() {
                let mut ifaces = component::wasi_world()?.imports;
                ifaces.retain(|i| {
                    let plain = i.module.split_once('@').map_or(i.module.as_str(), |(p, _)| p);
                    named.contains(&i.module.as_str()) || named.contains(&plain)
                });
                let mut b = js2wasi3_lower::Boundary { waits: true, ..Default::default() };
                // `wasi:sockets` (T66) and `wasi:http` (T72) are non-blocking: their `async func`s
                // return promises, their streams are async iterables, waited for by `run`'s event
                // loop. The others block.
                let (sockets, others): (Vec<_>, Vec<_>) = ifaces.into_iter().partition(|i| {
                    i.module.starts_with("wasi:sockets/") || i.module.starts_with("wasi:http/")
                });
                import_boundary(&mut b, &others, true);
                import_boundary(&mut b, &sockets, false);
                Some(b)
            } else {
                None
            };
            let mut ir = js2wasi3_lower::lower_with(&program, wasi.as_ref())?;
            ir.ignore_unhandled = ignore_unhandled;
            ir.gc_reserve_mib = gc_reserve;
            ir.count = count;
            ir.regex_vm = regex_vm;
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
            // T149/T150: a program importing a helper component (crypto, zlib) gets it composed in.
            let bytes = compose::helpers(bytes)?;
            std::fs::write(&out, bytes).with_context(|| format!("writing {out}"))?;
        }
    }
    Ok(())
}
