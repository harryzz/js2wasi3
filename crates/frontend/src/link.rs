//! Multi-file ES modules (T41): load the module graph from the entry and join it into one source
//! (scope hoisting, as Rollup and esbuild do; `docs/design.md` D4).
//!
//! Each module is parsed and checked alone. Every `import` and `export … from` is resolved to the
//! exporting module's binding, each module's top-level bindings get names unique in the joined
//! source, module imports and dependencies' `export` keywords are deleted, and the modules are
//! concatenated in evaluation order (post-order DFS, the entry last). The result is one source for
//! [`crate::parse_with_prelude`], with a [`SourceMap`] back to the files.
//!
//! A binding keeps its name unless it clashes with another module's top-level name (the entry's
//! come first), a global some module reads, or a name bound in a nested scope of a module that
//! imports it (a replaced reference there would be captured). A new name is `<name>$<k>`, unused
//! anywhere in the program.
//!
//! The linker also writes code at the start (slice 2–3): namespace objects, each module's
//! `import.meta` object, and, when a command's dependency has top-level `await`, the call that runs
//! the modules as separate functions in the spec's async order ([`SourceMap::modules`]). A JSON
//! module (`with { type: "json" }`) is checked and loaded as `export default <the JSON>;`.
//! `import()` (slice 4) loads the modules its specifier patterns match (or `--include` names),
//! after the static graph; one of them that fails to parse or link rejects at run time.

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;

use crate::{Allocator, Diagnostic, Diagnostics, Program, line_col, node, packages, parse};

/// Where each part of a joined source came from.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    /// Each module's path and source.
    files: Vec<(String, String)>,
    /// Each module's spliced `eval` calls (T42): offsets in its text here → its file.
    evals: Vec<Option<crate::EvalMap>>,
    /// By joined offset: from `at` on, the text comes from `file` at `offset`. `copied`: later
    /// offsets move along; else it's inserted text, all of it at `offset`.
    segments: Vec<Segment>,
    /// A replaced reference (its joined offset) → the name written there (TDZ messages use it).
    pub refs: HashMap<u32, String>,
    /// A renamed binding's name → its name in its module (a function's `name`).
    pub names: HashMap<String, String>,
    /// The references to imports (their joined offsets): immutable bindings.
    pub imports: HashSet<u32>,
    /// The joined source starts with this much code the linker wrote (the namespace objects),
    /// which may call the prelude.
    pub generated: u32,
    /// With a dependency that has top-level `await` (slice 3): each module's joined range, in
    /// evaluation order. Each is its own function; the generated code runs them with
    /// `$$EvaluateModules` (the spec's async module evaluation). Empty otherwise.
    pub modules: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    at: u32,
    file: u32,
    offset: u32,
    copied: bool,
}

impl SourceMap {
    /// Is the text at this joined offset the linker's (`$$DynamicImport(` at an `import(`, a
    /// renamed identifier), not copied from a file?
    pub fn is_inserted(&self, offset: u32) -> bool {
        let k = self.segments.partition_point(|s| s.at <= offset);
        k > 0 && !self.segments[k - 1].copied
    }

    /// Is the text at this joined offset from one of node's built-in modules (T55), whose code may
    /// use the prelude?
    pub fn is_node_module(&self, offset: u32) -> bool {
        let k = self.segments.partition_point(|s| s.at <= offset);
        k > 0 && node::is_module(&self.files[self.segments[k - 1].file as usize].0)
    }

    /// The file, 1-based line and column of a joined offset.
    pub fn locate(&self, offset: u32) -> (&str, u32, u32) {
        let k = self.segments.partition_point(|s| s.at <= offset).saturating_sub(1);
        let s = self.segments[k];
        let at = if s.copied { s.offset + (offset - s.at) } else { s.offset };
        let at = match &self.evals.get(s.file as usize) {
            Some(Some(e)) => e.original(at),
            _ => at,
        };
        let (path, source) = &self.files[s.file as usize];
        let (line, col) = line_col(source, at);
        (path, line, col)
    }
}

/// The program's source: the entry's text as is (one file), or the joined modules.
pub struct Linked {
    pub source: String,
    pub map: Option<SourceMap>,
}

/// What the linker reads: files, and directories (for `import()` patterns and `--include`).
pub trait Host {
    fn read(&mut self, path: &str) -> std::io::Result<String>;
    /// A directory's entries: name, and whether it's a directory.
    fn list(&mut self, dir: &str) -> std::io::Result<Vec<(String, bool)>>;
}

/// The file system.
pub struct Fs;

impl Host for Fs {
    fn read(&mut self, path: &str) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
    fn list(&mut self, dir: &str) -> std::io::Result<Vec<(String, bool)>> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(if dir.is_empty() { "." } else { dir })? {
            let e = e?;
            let is_dir = std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
            out.push((e.file_name().to_string_lossy().into_owned(), is_dir));
        }
        out.sort();
        Ok(out)
    }
}

/// How to link: `command`: the program is a command even if the entry has exports (as node runs
/// it): they're dropped like a dependency's. `include`: globs of modules `import()` may load at run
/// time (`--include`, slice 4).
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub command: bool,
    pub include: Vec<String>,
    /// External libraries (components composed in afterwards) linked even where the program doesn't
    /// name their words (`--ext-lib`, T149/T150): `crypto`, `zlib`.
    pub ext_libs: Vec<String>,
}

/// Load the modules the entry imports (through `host`) and join them.
///
/// The helper components (T149 `crypto`, T150 `zlib`; see [`EXT_LIBS`]) are imported only by programs that use them: the graph is
/// linked without them first (their modules are empty stubs), and again with a library's module if a module that is not a
/// built-in names one of its words. `--ext-lib` enables a library from the start.
pub fn link(
    alloc: &Allocator,
    entry: &str,
    source: &str,
    options: &Options,
    host: &mut dyn Host,
) -> Result<Linked, Diagnostics> {
    let mut enabled: Vec<String> = options.ext_libs.clone();
    loop {
        let (linked, wanted) = link_pass(alloc, entry, source, options, host, &enabled)?;
        if wanted.is_empty() {
            return Ok(linked);
        }
        enabled.extend(wanted);
    }
}

/// External libraries: components that `build` composes into the program (`crates/js2wasi3/src/compose.rs`). Name (`--ext-lib`),
/// the built-in module that holds the library's code (`<module>_off` is an empty stand-in while the library is not linked), and
/// the words that make a program use it.
const EXT_LIBS: [(&str, &str, &[&str]); 4] = [
    ("crypto", "node:internal/crypto_kdf", &["pbkdf2", "scrypt", "hkdf"]),
    ("zlib", "node:internal/zlib_impl", &["zlib"]),
    ("brotli", "node:internal/brotli_impl", &["rotli"]),
    ("zstd", "node:internal/zstd_impl", &["zstd", "Zstd"]),
];

fn link_pass(
    alloc: &Allocator,
    entry: &str,
    source: &str,
    options: &Options,
    host: &mut dyn Host,
    enabled: &[String],
) -> Result<(Linked, Vec<String>), Diagnostics> {
    let command = options.command;
    let mut g = Graph {
        alloc,
        host,
        command,
        modules: Vec::new(),
        index: HashMap::new(),
        diags: Vec::new(),
        table: Vec::new(),
        include: options.include.clone(),
        lazy: false,
        pending: Vec::new(),
        poisoned: Vec::new(),
        bare_failed: Vec::new(),
        cjs_known: Vec::new(),
        ext_libs: enabled.to_vec(),
    };
    g.load(entry.to_string(), source)?;
    // Then what only `import()` loads (its errors reject at run time).
    g.lazy = true;
    g.includes()?;
    while !g.pending.is_empty() {
        let (key, path) = g.pending.remove(0);
        g.dynamic_target(key, path)?;
    }
    if !g.diags.is_empty() {
        return Err(sorted(g.diags));
    }
    let wanted: Vec<String> = EXT_LIBS
        .iter()
        .filter(|(name, _, words)| {
            !enabled.iter().any(|e| e == name)
                && g.modules.iter().any(|m| !node::is_module(&m.p.path) && words.iter().any(|w| m.p.source.contains(w)))
        })
        .map(|(name, _, _)| name.to_string())
        .collect();
    if !wanted.is_empty() {
        return Ok((Linked { source: String::new(), map: None }, wanted));
    }
    if g.modules.iter().all(|m| {
        m.requests.is_empty() && m.meta.is_empty() && m.dynamic.is_empty() && m.p.evals.is_none() && !m.cjs
    })
        && !(command && !g.modules[0].p.exports.is_empty())
    {
        return Ok((
            Linked {
                source: source.to_string(),
                map: None,
            },
            Vec::new(),
        ));
    }
    g.cjs_exports();
    if g.modules.iter().any(|m| m.cjs) {
        // `--include`d modules: a computed `require` finds them too.
        for (key, k) in g.table.clone() {
            if !key.contains('\n') && !g.modules[k].p.path.ends_with(".json") && !g.cjs_known.iter().any(|(p, _)| *p == key) {
                g.cjs_known.push((key, k));
            }
        }
    }
    g.join().map(|l| (l, Vec::new()))
}

/// A JSON module's source (slice 3): `export default <the JSON>;` (JSON is JS syntax), with each
/// `"__proto__"` key made computed so it stays an own property, as `JSON.parse` makes it. Or where
/// the text isn't JSON, and why.
fn json_module(text: &str, prefix: &str) -> Result<String, (usize, String)> {
    let (_, proto) = packages::parse_json(text)?;
    let mut out = String::from(prefix);
    let mut pos = 0;
    for (start, end) in proto {
        out.push_str(&text[pos..start]);
        out.push('[');
        out.push_str(&text[start..end]);
        out.push(']');
        pos = end;
    }
    out.push_str(&text[pos..]);
    out.push_str(";\n");
    Ok(out)
}

/// A CommonJS module's text is parsed inside node's wrapper (T54), so `return`, `arguments` and
/// `this` at its top level are the function's, and `exports`, `require`, `module`, `__filename`,
/// `__dirname` its parameters. The joined source names the function (`function cjs$k(…) {`).
const CJS_PRE: &str = "(function (exports, require, module, __filename, __dirname) {";
const CJS_POST: &str = "\n});";

/// Parse a CommonJS file inside [`CJS_PRE`]/[`CJS_POST`] (as strict code, like the rest of the
/// program: T54's measurement found no sloppy-only syntax in a real graph). Offsets map back
/// to the file through the program's `evals` map; a `#!` line becomes a comment.
fn parse_cjs<'a>(alloc: &'a Allocator, path: &str, source: &'a str) -> Result<Program<'a>, Diagnostics> {
    let body = match source.strip_prefix("#!") {
        Some(rest) => format!("//{rest}"),
        None => source.to_string(),
    };
    let wrapped: &'a str = alloc.alloc_str(&format!("{CJS_PRE}{body}{CJS_POST}"));
    let pre = CJS_PRE.len() as u32;
    let (mut p, evals) = crate::eval::parse_spliced(alloc, path, wrapped).map_err(|mut d| {
        // Valid sloppy-mode code (`with`, octal literals, …) that doesn't parse as strict code: say
        // why it's rejected.
        let sloppy = parse(alloc, path, wrapped).is_err() && {
            let ret = oxc_parser::Parser::new(alloc, wrapped, oxc_span::SourceType::script()).parse();
            !ret.fatal_error && !ret.diagnostics.has_errors() && {
                let ast = alloc.alloc(ret.program);
                !oxc_semantic::SemanticBuilder::new().with_check_syntax_error(true).build(ast).diagnostics.has_errors()
            }
        };
        for d in &mut d.0 {
            // The wrapper has no line break: only the first line's columns move.
            if d.line == 1 {
                d.col = d.col.saturating_sub(CJS_PRE.chars().count() as u32).max(1);
            }
            if sloppy {
                d.help = Some("node runs this CommonJS file as sloppy-mode code; js2wasi3 compiles CommonJS as strict code (planned: F232)".into());
            }
        }
        d
    })?;
    let layer = vec![
        crate::eval::Region::new(0, pre, 0, 0),
        crate::eval::Region::new(pre + source.len() as u32, CJS_POST.len() as u32, source.len() as u32, 0),
    ];
    p.evals = Some(crate::EvalMap::first(layer, evals));
    p.original = source;
    Ok(p)
}

/// How node runs a file: by its extension, else its package's `"type"`, else by its syntax.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Format {
    Esm,
    Cjs,
    /// A `.js` file without a `"type"`: CommonJS unless it has ESM syntax (node 24's detection).
    Ambiguous,
}

/// Does the module have ESM syntax (node's detection: `import`/`export` statements,
/// `import.meta`, top-level `await`)?
fn esm_syntax(p: &Program) -> bool {
    p.ast.body.iter().any(|s| s.is_module_declaration())
        || has_tla(p)
        || p.semantic.nodes().iter().any(|n| matches!(n.kind(), AstKind::ImportMeta(_)))
}

/// Does code parsed as a module observe that node would run it as CommonJS: its names
/// (`module`, `exports`, `require`, `__filename`, `__dirname`, `arguments`) or top-level `this`?
fn observes_cjs(p: &Program) -> bool {
    let names = ["module", "exports", "require", "__filename", "__dirname", "arguments"];
    let scoping = p.semantic.scoping();
    if scoping.root_unresolved_references().keys().any(|k| names.contains(&k.as_str())) {
        return true;
    }
    let nodes = p.semantic.nodes();
    nodes.iter().any(|n| {
        matches!(n.kind(), AstKind::ThisExpression(_))
            && !nodes.ancestors(n.id()).any(|a| matches!(a.kind(), AstKind::Function(_) | AstKind::Class(_)))
    })
}

/// The static `require` calls of a CommonJS module (T54): each `require("literal")` (or
/// `require.resolve("literal")`) of the wrapper's `require`, the specifier and where it's written,
/// in source order, once each; and the computed ones with their `import()`-style patterns.
fn cjs_requires(p: &Program) -> (Vec<(String, Span, bool)>, Vec<(Span, Option<Vec<Vec<Part>>>)>) {
    let mut computed = Vec::new();
    let Some(Statement::ExpressionStatement(s)) = p.ast.body.first() else { return (Vec::new(), computed) };
    let Expression::FunctionExpression(f) = s.expression.without_parentheses() else { return (Vec::new(), computed) };
    let Some(sym) = f.params.items.get(1).and_then(|b| b.pattern.get_binding_identifier()).map(|b| b.symbol_id()) else {
        return (Vec::new(), computed);
    };
    let scoping = p.semantic.scoping();
    let nodes = p.semantic.nodes();
    let mut out: Vec<(String, Span, bool)> = Vec::new();
    let wrapper = f.scope_id.get();
    for &r in scoping.get_resolved_reference_ids(sym) {
        let node = scoping.get_reference(r).node_id();
        // `require(x)`, or `require.resolve(x)`.
        let mut callee = nodes.kind(node).span();
        let mut parent = nodes.parent_id(node);
        if let AstKind::StaticMemberExpression(m) = nodes.kind(parent)
            && m.property.name == "resolve"
        {
            callee = m.span;
            parent = nodes.parent_id(parent);
        }
        let AstKind::CallExpression(c) = nodes.kind(parent) else { continue };
        if c.callee.span() != callee || c.arguments.len() != 1 {
            continue;
        }
        let Some(arg) = c.arguments[0].as_expression() else { continue };
        let spec = match arg.without_parentheses() {
            Expression::StringLiteral(s) => s.value.to_string(),
            Expression::TemplateLiteral(t) if t.expressions.is_empty() => {
                match t.quasis.first().and_then(|q| q.value.cooked) {
                    Some(s) => s.to_string(),
                    None => continue,
                }
            }
            // Computed (slice 4): the files T41's `import()` patterns match.
            _ => {
                let pats = patterns(p, arg, 0).map(|ps| ps.into_iter().map(merge).collect());
                computed.push((c.span, pats));
                continue;
            }
        };
        // Does it always run when the module does: not in a `try`, or a function the module
        // may not call?
        let always = !nodes.ancestors(parent).any(|a| match a.kind() {
            AstKind::TryStatement(_) => true,
            AstKind::Function(g) => g.scope_id.get() != wrapper,
            AstKind::ArrowFunctionExpression(_) => true,
            _ => false,
        });
        match out.iter_mut().find(|(s, ..)| *s == spec) {
            Some(o) => o.2 |= always,
            None => out.push((spec, arg.span(), always)),
        }
    }
    out.sort_by_key(|(_, span, _)| span.start);
    (out, computed)
}

/// A CommonJS module's named exports as node's cjs-module-lexer finds them (T54), from the AST:
/// `exports.x =`, `module.exports.x =` (also `["x"]`), the keys of `module.exports = { a, b: c }`
/// (the lexer's rules for where the list ends), `Object.defineProperty(exports, "x", …)`
/// with a `value` or a getter returning an identifier or member; and the specifiers it
/// re-exports: `module.exports = require("…")`, `...require("…")` in that object,
/// `__exportStar(require("…"), exports)` / `__export(require("…"))` (tsc). Dead code counts, as
/// for the lexer.
fn cjs_exports(p: &Program) -> (Vec<String>, Vec<String>) {
    let (mut names, mut reexports) = (Vec::new(), Vec::new());
    let Some(Statement::ExpressionStatement(s)) = p.ast.body.first() else { return (names, reexports) };
    let Expression::FunctionExpression(f) = s.expression.without_parentheses() else { return (names, reexports) };
    let param = |i: usize| f.params.items.get(i).and_then(|b| b.pattern.get_binding_identifier()).map(|b| b.symbol_id());
    let (Some(exports), Some(require), Some(module)) = (param(0), param(1), param(2)) else { return (names, reexports) };
    let scoping = p.semantic.scoping();
    let is = |e: &Expression, sym: SymbolId| match e.without_parentheses() {
        Expression::Identifier(id) => scoping.get_reference(id.reference_id()).symbol_id() == Some(sym),
        _ => false,
    };
    let key = |m: &MemberExpression| -> Option<String> {
        match m {
            MemberExpression::StaticMemberExpression(s) => Some(s.property.name.to_string()),
            MemberExpression::ComputedMemberExpression(c) => match &c.expression {
                Expression::StringLiteral(l) => Some(l.value.to_string()),
                _ => None,
            },
            _ => None,
        }
    };
    // `module.exports`
    let is_module_exports = |e: &Expression| match e.without_parentheses().as_member_expression() {
        Some(m) => is(m.object(), module) && key(m).as_deref() == Some("exports"),
        None => false,
    };
    let is_exports = |e: &Expression| is(e, exports) || is_module_exports(e);
    // `require("…")`
    let required = |e: &Expression| -> Option<String> {
        let Expression::CallExpression(c) = e.without_parentheses() else { return None };
        if !is(&c.callee, require) || c.arguments.len() != 1 {
            return None;
        }
        match c.arguments[0].as_expression()?.without_parentheses() {
            Expression::StringLiteral(l) => Some(l.value.to_string()),
            _ => None,
        }
    };
    let property_key = |k: &PropertyKey| -> Option<String> {
        match k {
            PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
            PropertyKey::StringLiteral(l) => Some(l.value.to_string()),
            _ => None,
        }
    };
    for node in p.semantic.nodes().iter() {
        match node.kind() {
            AstKind::AssignmentExpression(a) if a.operator == AssignmentOperator::Assign => {
                let Some(target) = a.left.as_simple_assignment_target().and_then(|t| t.as_member_expression()) else {
                    continue;
                };
                if is_exports(target.object()) {
                    if let Some(k) = key(target) {
                        names.push(k);
                    }
                    continue;
                }
                if !(is(target.object(), module) && key(target).as_deref() == Some("exports")) {
                    continue;
                }
                if let Some(spec) = required(&a.right) {
                    reexports.push(spec);
                } else if let Expression::ObjectExpression(o) = a.right.without_parentheses() {
                    // The lexer's token rules: `a`, `a: b` and `...b` go on; a value that only
                    // starts with an identifier (`a: b.c`, `a: require(…)`) or a method `a() {}`
                    // is the last key; anything else ends the list before its key.
                    fn starts_with_identifier(e: &Expression) -> bool {
                        match e {
                            Expression::Identifier(_) => true,
                            Expression::StaticMemberExpression(m) => starts_with_identifier(&m.object),
                            Expression::ComputedMemberExpression(m) => starts_with_identifier(&m.object),
                            Expression::CallExpression(c) => starts_with_identifier(&c.callee),
                            _ => false,
                        }
                    }
                    for prop in &o.properties {
                        match prop {
                            ObjectPropertyKind::SpreadProperty(sp) => {
                                if let Some(spec) = required(&sp.argument) {
                                    reexports.push(spec);
                                } else if !matches!(sp.argument, Expression::Identifier(_)) {
                                    break;
                                }
                            }
                            ObjectPropertyKind::ObjectProperty(op) => {
                                let key = (!op.computed && op.kind == PropertyKind::Init).then(|| property_key(&op.key)).flatten();
                                let Some(k) = key else { break };
                                let (simple, last) = match &op.value {
                                    _ if op.shorthand => (true, false),
                                    Expression::FunctionExpression(f) if op.method => (!f.r#async && !f.generator, true),
                                    Expression::Identifier(_) => (true, false),
                                    v => (starts_with_identifier(v), true),
                                };
                                if simple {
                                    names.push(k);
                                }
                                if last || !simple {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            AstKind::CallExpression(c) => {
                let args: Vec<&Expression> = c.arguments.iter().filter_map(|a| a.as_expression()).collect();
                let callee = c.callee.without_parentheses();
                // `Object.defineProperty(exports, "x", { value } | { get() { return a.b; } })`
                if let Some(m) = callee.as_member_expression()
                    && matches!(m.object().without_parentheses(), Expression::Identifier(o) if o.name == "Object")
                    && key(m).as_deref() == Some("defineProperty")
                    && args.len() == 3
                    && is_exports(args[0])
                    && let Expression::StringLiteral(name) = args[1].without_parentheses()
                    && let Expression::ObjectExpression(desc) = args[2].without_parentheses()
                {
                    let simple_return = |body: &FunctionBody| {
                        matches!(&body.statements[..], [Statement::ReturnStatement(r)]
                            if r.argument.as_ref().is_some_and(|e| matches!(e.without_parentheses(),
                                Expression::Identifier(_) | Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_))))
                    };
                    let ok = desc.properties.iter().any(|prop| {
                        let ObjectPropertyKind::ObjectProperty(op) = prop else { return false };
                        match property_key(&op.key).as_deref() {
                            Some("value") => true,
                            Some("get") => match op.value.without_parentheses() {
                                // `get: function () { … }` or `get() { … }`.
                                Expression::FunctionExpression(g) => g.body.as_ref().is_some_and(|b| simple_return(b)),
                                _ => false,
                            },
                            _ => false,
                        }
                    });
                    if ok {
                        names.push(name.value.to_string());
                    }
                    continue;
                }
                // tsc: `__exportStar(require("x"), exports)`, `tslib.__exportStar(…)`, `__export(require("x"))`.
                let helper = match callee {
                    Expression::Identifier(id) => Some(id.name.as_str()),
                    Expression::StaticMemberExpression(s) => Some(s.property.name.as_str()),
                    _ => None,
                };
                let star = match helper {
                    Some("__exportStar") => args.len() == 2 && is_exports(args[1]),
                    Some("__export") => args.len() == 1,
                    _ => false,
                };
                if star && let Some(spec) = args.first().and_then(|a| required(a)) {
                    reexports.push(spec);
                }
            }
            _ => {}
        }
    }
    names.retain(|n| n != "default");
    names.sort();
    names.dedup();
    reexports.dedup();
    (names, reexports)
}

/// The specifier patterns of an `import()` argument known at build time: string literals,
/// template literals and `+` (their other parts match anything), both sides of `?:`, `||` and
/// `??`, and `const`s initialized with those. `None`: unknown.
fn patterns(p: &Program, e: &Expression, depth: u32) -> Option<Vec<Vec<Part>>> {
    if depth > 8 {
        return None;
    }
    match e.without_parentheses() {
        Expression::StringLiteral(s) => Some(vec![vec![Part::Lit(s.value.to_string())]]),
        Expression::TemplateLiteral(t) => {
            let mut pat = Vec::new();
            for (i, q) in t.quasis.iter().enumerate() {
                pat.push(Part::Lit(q.value.cooked?.to_string()));
                if i < t.expressions.len() {
                    pat.push(Part::Wild);
                }
            }
            Some(vec![pat])
        }
        Expression::BinaryExpression(b) if b.operator == BinaryOperator::Addition => {
            let side = |e: &Expression| patterns(p, e, depth + 1).unwrap_or_else(|| vec![vec![Part::Wild]]);
            let (l, r) = (side(&b.left), side(&b.right));
            let mut out = Vec::new();
            for a in &l {
                for b in &r {
                    out.push(a.iter().chain(b).cloned().collect());
                }
            }
            Some(out)
        }
        Expression::ConditionalExpression(c) => {
            let mut out = patterns(p, &c.consequent, depth + 1)?;
            out.extend(patterns(p, &c.alternate, depth + 1)?);
            Some(out)
        }
        Expression::LogicalExpression(l) if l.operator != LogicalOperator::And => {
            let mut out = patterns(p, &l.left, depth + 1)?;
            out.extend(patterns(p, &l.right, depth + 1)?);
            Some(out)
        }
        Expression::Identifier(id) => {
            let scoping = p.semantic.scoping();
            let sym = scoping.get_reference(id.reference_id()).symbol_id()?;
            if !scoping.symbol_flags(sym).is_const_variable() {
                return None;
            }
            let decl = scoping.symbol_declaration(sym);
            let AstKind::VariableDeclarator(d) = p.semantic.nodes().kind(decl) else { return None };
            patterns(p, d.init.as_ref()?, depth + 1)
        }
        _ => None,
    }
}

/// Adjacent texts joined, adjacent wildcards merged.
fn merge(pat: Vec<Part>) -> Vec<Part> {
    let mut out: Vec<Part> = Vec::new();
    for part in pat {
        match (out.last_mut(), part) {
            (Some(Part::Lit(a)), Part::Lit(b)) => a.push_str(&b),
            (Some(Part::Wild), Part::Wild) => {}
            (_, part) => out.push(part),
        }
    }
    out
}

fn is_relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/')
}

/// A pattern the build can look up: it starts with a relative or absolute path.
fn is_relative_pattern(pat: &[Part]) -> bool {
    matches!(pat.first(), Some(Part::Lit(t)) if is_relative(t))
}

fn is_module_file(name: &str) -> bool {
    name.ends_with(".js") || name.ends_with(".mjs") || name.ends_with(".cjs") || name.ends_with(".json")
}

/// Does `name` match a path segment's pattern?
fn glob_match(seg: &[Part], name: &str) -> bool {
    match seg.split_first() {
        None => name.is_empty(),
        Some((Part::Lit(t), rest)) => name.strip_prefix(t.as_str()).is_some_and(|n| glob_match(rest, n)),
        Some((Part::Wild, rest)) => (0..=name.len()).filter(|k| name.is_char_boundary(*k)).any(|k| glob_match(rest, &name[k..])),
    }
}

/// A path made absolute against the working directory (lexically: symlinks stay, as the
/// run-time lookup joins paths the same way).
/// A directory (`a/b/`, or empty for the working directory) made absolute, ending with `/`.
pub(crate) fn absolute_dir(dir: &str) -> String {
    let abs = if dir.is_empty() { absolute(".") } else { absolute(dir) };
    if abs.ends_with('/') { abs } else { format!("{abs}/") }
}

fn absolute(path: &str) -> String {
    if path.starts_with('/') {
        return normalize(path);
    }
    let cwd = std::env::current_dir().map_or_else(|_| "/".to_string(), |d| d.to_string_lossy().into_owned());
    normalize(&format!("{cwd}/{path}"))
}

/// A module's absolute path (symlinks resolved, as node does) and its directory.
fn abs_file_dir(path: &str) -> (String, String) {
    if node::is_module(path) {
        // A built-in (T55): `require.resolve("path")` is `"path"`, as in node.
        return (path["node:".len()..].to_string(), ".".to_string());
    }
    let abs = std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .map_or_else(|_| path.to_string(), |p| p.to_string_lossy().into_owned());
    let dir = match abs.rfind('/') {
        Some(0) => "/".to_string(),
        Some(k) => abs[..k].to_string(),
        None => ".".to_string(),
    };
    (abs, dir)
}

/// node's `url.pathToFileURL(path)` for an absolute POSIX path.
fn file_url(path: &str) -> String {
    let mut out = String::from("file://");
    for b in path.bytes() {
        if b <= b' ' || b >= 0x7e || b"\"#%<>?`{}^|[]\\".contains(&b) {
            out.push_str(&format!("%{b:02X}"));
        } else {
            out.push(b as char);
        }
    }
    out
}

/// A reference to one of node's globals that a built-in module provides (`node::GLOBALS`).
#[derive(Debug, Clone, Copy)]
struct GlobalRef {
    span: Span,
    /// `globalThis.X` read (T71: srvx's `globalThis.URL`), not the identifier `X`.
    member: bool,
}

/// The references to the undeclared global `name`, and reads of `globalThis.name` (not assignment
/// targets) when the module declares no `name` at its top level.
fn global_refs(p: &Program, name: &str, cjs: bool) -> Vec<GlobalRef> {
    let scoping = p.semantic.scoping();
    let nodes = p.semantic.nodes();
    let mut out: Vec<GlobalRef> = scoping
        .root_unresolved_references()
        .get(name)
        .map(|refs| {
            refs.iter()
                .map(|&r| GlobalRef { span: nodes.kind(scoping.get_reference(r).node_id()).span(), member: false })
                .collect()
        })
        .unwrap_or_default();
    // Declared at the top level (a CommonJS module's top level is its wrapper function's scope).
    let root = scoping.root_scope_id();
    if scoping.symbol_ids().any(|sym| {
        let sc = scoping.symbol_scope_id(sym);
        scoping.symbol_name(sym) == name && (sc == root || (cjs && scoping.scope_parent_id(sc) == Some(root)))
    }) {
        return out;
    }
    if let Some(refs) = scoping.root_unresolved_references().get("globalThis") {
        for &r in refs {
            let id = scoping.get_reference(r).node_id();
            let parent = nodes.parent_node(id);
            if let AstKind::StaticMemberExpression(m) = parent.kind()
                && m.property.name == name
                && matches!(&m.object, Expression::Identifier(o) if o.span == nodes.kind(id).span())
                && !matches!(nodes.parent_kind(parent.id()), AstKind::AssignmentTargetWithDefault(_))
                && !is_assignment_target(nodes, parent.id())
            {
                out.push(GlobalRef { span: m.span, member: true });
            }
        }
    }
    out
}

/// Is the member expression at `id` written to (`=`, `+=`, `++`, `delete`, a destructuring target)?
fn is_assignment_target(nodes: &oxc_semantic::AstNodes, id: oxc_semantic::NodeId) -> bool {
    match nodes.parent_kind(id) {
        AstKind::AssignmentExpression(a) => a.left.span() == nodes.kind(id).span(),
        AstKind::UpdateExpression(_) => true,
        AstKind::UnaryExpression(u) => u.operator == UnaryOperator::Delete,
        AstKind::AssignmentTargetPropertyProperty(_)
        | AstKind::ArrayAssignmentTarget(_) | AstKind::ObjectAssignmentTarget(_) => true,
        _ => false,
    }
}

/// Does the module have top-level `await` (outside any function)?
fn has_tla(p: &Program) -> bool {
    let nodes = p.semantic.nodes();
    nodes.iter().any(|node| {
        let top = match node.kind() {
            AstKind::AwaitExpression(_) => true,
            AstKind::ForOfStatement(f) => f.r#await,
            AstKind::VariableDeclaration(d) => d.kind == VariableDeclarationKind::AwaitUsing,
            _ => false,
        };
        top && !nodes.ancestors(node.id()).any(|a| {
            matches!(a.kind(), AstKind::Function(_) | AstKind::ArrowFunctionExpression(_))
        })
    })
}

fn sorted(mut d: Vec<Diagnostic>) -> Diagnostics {
    d.sort_by(|a, b| (&a.path, a.line, a.col).cmp(&(&b.path, b.line, b.col)));
    Diagnostics(d)
}

/// What a module binding resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Binding {
    /// A top-level binding of a module.
    Sym(usize, SymbolId),
    /// The synthetic binding of an `export default` without a name.
    Default(usize),
    /// A module's namespace object (slice 2).
    Namespace(usize),
    /// A module's `import.meta` object (slice 3).
    Meta(usize),
    /// A CommonJS module's function (T54).
    Cjs(usize),
    /// An ES module's view of a CommonJS module (T54): its `module.exports` once it has run
    /// (`default`, `'module.exports'`), and a named export (index into [`Module::cjs_names`]).
    CjsValue(usize),
    CjsNamed(usize, usize),
    /// The table of CommonJS modules (T54).
    CjsTable,
}

#[derive(Debug, Clone)]
enum Name {
    Name(String),
    Namespace,
}

struct Import {
    module: usize,
    name: Name,
    specifier: String,
    span: Span,
}

struct Module<'a> {
    p: Program<'a>,
    /// Requested modules, in source order.
    requests: Vec<usize>,
    /// Import bindings of other modules (not WIT interfaces).
    imports: HashMap<SymbolId, Import>,
    /// `export …` of this module's own bindings: exported name → binding.
    locals: Vec<(String, Binding)>,
    /// `export { x as y } from`, `export * as y from`: exported name → (module, name), and the
    /// specifier and where it's written.
    indirect: Vec<(String, usize, Name, String, Span)>,
    /// `export * from`.
    stars: Vec<usize>,
    /// The request of each `import`/`export … from` statement (by its start).
    targets: HashMap<u32, usize>,
    /// Has top-level `await` (or `for await`).
    tla: bool,
    /// Where it uses `import.meta`.
    meta: Vec<Span>,
    /// Its `import()` expressions.
    dynamic: Vec<Dynamic>,
    /// Loaded only for `import()` (after the static graph): its errors reject that `import()` at
    /// run time, as in node, instead of failing the build.
    lazy: bool,
    /// Such a module's error: the error's class, message and node's `code` (or empty).
    failed: Option<Failed>,
    /// A CommonJS module (T54), parsed inside [`CJS_PRE`]/[`CJS_POST`].
    cjs: bool,
    /// Its static `require`s: the specifier and the module, or the error `require` throws at run
    /// time: node's `MODULE_NOT_FOUND` (`None`), or a code and message.
    cjs_requests: Vec<(String, Result<usize, Option<(String, String)>>)>,
    /// Its named exports for ES modules, as node's cjs-module-lexer finds them, with those of
    /// the modules it re-exports (T54).
    cjs_names: Vec<String>,
    /// Imported by an ES module (or by `import()`): its exports are read once it has run.
    esm_imported: bool,
    /// T56/T61: its references to node's globals that are built-in modules' exports
    /// (`node::GLOBALS`: the index there, and the unresolved references), and the modules they name
    /// (the index, the module): an ES module's are replaced by the export; a CommonJS module gets
    /// `const X = require("node:x").X;` first.
    global_refs: Vec<(usize, Vec<GlobalRef>)>,
    global_modules: Vec<(usize, usize)>,
}

struct Graph<'a, 'r> {
    alloc: &'a Allocator,
    host: &'r mut dyn Host,
    command: bool,
    modules: Vec<Module<'a>>,
    index: HashMap<String, usize>,
    diags: Vec<Diagnostic>,
    /// What `import()` can load (slice 4): the module's absolute path (the lookup key) and index.
    table: Vec<(String, usize)>,
    /// `--include` globs.
    include: Vec<String>,
    /// Loading modules only `import()` reaches (after the static graph).
    lazy: bool,
    /// What `import()` can load, not looked at yet.
    pending: Vec<(String, String)>,
    /// By module: loaded only for `import()` and fails, or imports one that does (then not in the
    /// program: its `import()` rejects with that error).
    poisoned: Vec<Option<Failed>>,
    /// `import()` of a package that doesn't resolve: its key and error.
    bare_failed: Vec<(String, Failed)>,
    /// What a computed `require` can load (T54 slice 4): absolute path → module.
    cjs_known: Vec<(String, usize)>,
    /// The enabled [`EXT_LIBS`] (the others' modules are empty stubs). See [`link`].
    ext_libs: Vec<String>,
}

/// A module that fails to load for `import()`: the error's class, message and node's `code`.
type Failed = (String, String, String);

/// A piece of an `import()` specifier known at build time: text, or any text without `/`.
#[derive(Debug, Clone, PartialEq)]
enum Part {
    Lit(String),
    Wild,
}

/// An `import()` (slice 4): where it is, where its specifier starts, and whether the build knows
/// the modules it can load.
struct Dynamic {
    span: Span,
    arg: u32,
}

/// A specifier resolved against the importing file.
enum Request {
    File(String),
    /// A WIT interface (`ns:pkg/iface`): an import of the user world (T20), not a module.
    Wit,
}

impl<'a> Graph<'a, '_> {
    fn diag(&mut self, m: usize, span: Span, message: String, help: Option<String>) {
        if self.modules[m].lazy {
            // Its `import()` rejects with the first error (slice 4).
            if self.modules[m].failed.is_none() {
                // Messages end with node's code: `… (ERR_MODULE_NOT_FOUND)`.
                let (message, code) = match message.rsplit_once(" (ERR_") {
                    Some((m, c)) if c.ends_with(')') => (m.to_string(), format!("ERR_{}", &c[..c.len() - 1])),
                    _ => (message, String::new()),
                };
                self.modules[m].failed = Some(match message.strip_prefix("SyntaxError: ") {
                    Some(rest) => ("SyntaxError".into(), rest.into(), code),
                    None => ("Error".into(), message, code),
                });
            }
            return;
        }
        let p = &self.modules[m].p;
        let (line, col) = p.line_col(span.start);
        self.diags.push(Diagnostic {
            path: p.path.clone(),
            line,
            col,
            message,
            help,
        });
    }

    /// A module's text: a file, or a node built-in's shipped source (T55, `node:path`).
    fn read(&mut self, path: &str) -> std::io::Result<String> {
        match node::source(path) {
            Some(s) => Ok(s.to_string()),
            None => self.host.read(path),
        }
    }

    /// How node runs the file at `path` (T54). node's built-in modules are CommonJS (T55), except
    /// those importing WASI interfaces (T59, `node::is_esm`).
    fn format(&mut self, path: &str) -> Format {
        if node::is_module(path) {
            return if node::is_esm(path) {
                Format::Esm
            } else {
                Format::Cjs
            };
        }
        if path.ends_with(".mjs") {
            return Format::Esm;
        }
        if path.ends_with(".cjs") {
            return Format::Cjs;
        }
        match packages::package_type(self.host, &path[..=path.rfind('/').unwrap_or(0)]).as_deref() {
            Some("module") => Format::Esm,
            Some("commonjs") => Format::Cjs,
            _ => Format::Ambiguous,
        }
    }

    /// Parse a file as node would run it: an ES module or CommonJS (T54). The entry, when node
    /// would run it as CommonJS but nothing in it can tell, stays an ES module (the same bytes as
    /// before T54).
    fn parse_as(&mut self, path: &str, source: &'a str) -> Result<(Program<'a>, bool), Diagnostics> {
        let esm = |alloc| {
            crate::eval::parse_spliced(alloc, path, source).map(|(mut p, evals)| {
                p.evals = evals;
                p
            })
        };
        match self.format(path) {
            Format::Esm => esm(self.alloc).map(|p| (p, false)),
            Format::Cjs => parse_cjs(self.alloc, path, source).map(|p| (p, true)),
            Format::Ambiguous => match esm(self.alloc) {
                Ok(p) if esm_syntax(&p) || (self.modules.is_empty() && !observes_cjs(&p)) => Ok((p, false)),
                Ok(_) => parse_cjs(self.alloc, path, source).map(|p| (p, true)),
                // Not a module (`return` at the top level): CommonJS if it parses as that.
                Err(d) => parse_cjs(self.alloc, path, source).map(|p| (p, true)).map_err(|c| {
                    if c.0.iter().any(|d| d.help.is_some()) { c } else { d }
                }),
            },
        }
    }

    fn load(&mut self, path: String, source: &str) -> Result<usize, Diagnostics> {
        let source: &'a str = self.alloc.alloc_str(source);
        let (p, failed, cjs) = match self.parse_as(&path, source) {
            Ok((p, cjs)) => (p, None, cjs),
            // A module only `import()` loads: an empty stand-in that rejects.
            Err(d) if self.lazy => {
                let message = d.0.first().map_or_else(String::new, |d| d.message.clone());
                (parse(self.alloc, &path, "")?, Some(("SyntaxError".to_string(), message, String::new())), false)
            }
            // Reported with the rest of the graph's errors (T62): an empty CommonJS stand-in keeps
            // the build going so that one run lists them all.
            Err(d) => {
                self.diags.extend(d.0);
                (parse_cjs(self.alloc, &path, "")?, None, true)
            }
        };
        self.add(path, p, failed, cjs)
    }

    /// A JSON file `require` loads (T54): CommonJS `module.exports = <the JSON>;`.
    fn load_cjs_json(&mut self, path: String, text: &str) -> Result<Option<usize>, Diagnostics> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        match json_module(text, "module.exports = ") {
            Ok(source) => {
                let source: &'a str = self.alloc.alloc_str(&source);
                let p = parse_cjs(self.alloc, &path, source)?;
                self.add(path, p, None, true).map(Some)
            }
            Err((at, what)) => {
                let (line, col) = line_col(text, at as u32);
                self.diags.push(Diagnostic {
                    path,
                    line,
                    col,
                    message: format!("invalid JSON: {what}"),
                    help: None,
                });
                Ok(None)
            }
        }
    }

    fn add(&mut self, path: String, p: Program<'a>, failed: Option<Failed>, cjs: bool) -> Result<usize, Diagnostics> {
        let m = self.modules.len();
        self.index.insert(path, m);
        let ast = p.ast;
        let tla = has_tla(&p);
        let meta = p
            .semantic
            .nodes()
            .iter()
            .filter_map(|n| match n.kind() {
                AstKind::ImportMeta(m) => Some(m.span),
                _ => None,
            })
            .collect();
        self.modules.push(Module {
            p,
            requests: Vec::new(),
            imports: HashMap::new(),
            locals: Vec::new(),
            indirect: Vec::new(),
            stars: Vec::new(),
            targets: HashMap::new(),
            tla,
            meta,
            dynamic: Vec::new(),
            lazy: self.lazy,
            failed,
            cjs: false,
            cjs_requests: Vec::new(),
            cjs_names: Vec::new(),
            esm_imported: false,
            global_refs: Vec::new(),
            global_modules: Vec::new(),
        });
        self.modules[m].global_refs = node::GLOBALS
            .iter()
            .enumerate()
            .map(|(g, (name, ..))| (g, global_refs(&self.modules[m].p, name, cjs)))
            .filter(|(_, refs)| !refs.is_empty())
            .collect();
        if cjs {
            self.modules[m].cjs = true;
            if let Some(&span) = self.modules[m].meta.first() {
                self.diag(m, span, "SyntaxError: Cannot use 'import.meta' outside a module (node runs this file as CommonJS)".into(), None);
            }
            self.cjs_requests(m)?;
            self.dynamic_imports(m)?;
            return Ok(m);
        }
        for stmt in &ast.body {
            let (span, source, with) = match stmt {
                Statement::ImportDeclaration(i) => {
                    if i.phase.is_some() {
                        self.diag(m, i.span, "unsupported: import phases (`import defer`, `import source`) (planned: not scheduled)".into(), None);
                    }
                    (i.span, &i.source, &i.with_clause)
                }
                Statement::ExportFromDeclaration(e) => (e.span, &e.source, &e.with_clause),
                Statement::ExportAllDeclaration(e) => (e.span, &e.source, &e.with_clause),
                _ => continue,
            };
            // Import attributes: node knows only `type: "json"` (slice 3: JSON modules).
            let mut json = false;
            let mut bad = false;
            for a in with.iter().flat_map(|w| w.with_entries.iter()) {
                let key = match &a.key {
                    ImportAttributeKey::Identifier(k) => k.name.as_str(),
                    ImportAttributeKey::StringLiteral(k) => k.value.as_str(),
                };
                if key == "type" && a.value.value == "json" {
                    json = true;
                } else {
                    let message = format!("import attribute `{key}: {:?}` isn't supported (node knows only `type: \"json\"`)", a.value.value.as_str());
                    self.diag(m, a.span, message, None);
                    bad = true;
                }
            }
            if bad {
                continue;
            }
            let Some(target) = self.request(m, source, json)? else {
                continue;
            };
            self.modules[m].requests.push(target);
            self.modules[m].targets.insert(span.start, target);
        }
        for (g, refs) in self.modules[m].global_refs.clone() {
            // node's `Buffer` global (T56) and the others (T61): like an import of the export.
            let (_, module, _) = node::GLOBALS[g];
            if let Some(Request::File(path)) = self.resolve(m, module, refs[0].span) {
                let k = match self.index.get(&path) {
                    Some(&k) => k,
                    None => {
                        let text = self.read(&path).expect("a built-in module");
                        self.load(path, &text)?
                    }
                };
                if !self.modules[m].requests.contains(&k) {
                    self.modules[m].requests.push(k);
                }
                self.modules[m].global_modules.push((g, k));
            }
        }
        self.dynamic_imports(m)?;
        self.scan(m);
        Ok(m)
    }

    /// CommonJS module `m`'s static `require`s (T54): each specifier resolved with node's CommonJS
    /// algorithm and its module loaded. One that isn't found throws when it runs, as in node.
    fn cjs_requests(&mut self, m: usize) -> Result<(), Diagnostics> {
        let (mut statics, computed) = cjs_requires(&self.modules[m].p);
        self.cjs_computed(m, computed)?;
        for (g, refs) in &self.modules[m].global_refs {
            // node's `Buffer` global (T56) and the others (T61): the wrapper requires them first.
            statics.push((node::GLOBALS[*g].1.to_string(), refs[0].span, false));
        }
        for (spec, span, always) in statics {
            // node's `require` also takes `.` and `..` as relative, and as directories (T62).
            let dot = spec == "." || spec == ".." || spec.ends_with("/.") || spec.ends_with("/..");
            let target = if is_relative(&spec) || spec == "." || spec == ".." {
                let from = &self.modules[m].p.path;
                let dir = &from[..from.rfind('/').map_or(0, |k| k + 1)];
                let joined = if spec.starts_with('/') { spec.clone() } else { format!("{dir}{spec}") };
                packages::load_as_file_or_directory(self.host, &normalize(&joined), spec.ends_with('/') || dot).ok_or(None)
            } else if let Some(path) = node::path(&spec) {
                Ok(match EXT_LIBS.iter().find(|(name, module, _)| *module == path && !self.ext_libs.iter().any(|e| e == name)) {
                    Some(_) => format!("{path}_off"),
                    None => path,
                })
            } else if packages::is_builtin(spec.strip_prefix("node:").unwrap_or(&spec)) || spec.starts_with("node:") {
                // A build error where it always runs; elsewhere (`try { require("util") } catch`,
                // optional features) thrown when it runs, as `import()` of one does.
                if always {
                    self.diag(m, span, format!("unsupported: node's built-in module `{spec}` (planned: F238)"), None);
                    continue;
                }
                let message = if packages::is_builtin(spec.strip_prefix("node:").unwrap_or(&spec)) {
                    format!("node's built-in module '{spec}' isn't available in js2wasi3 (planned: F238)")
                } else {
                    format!("No such built-in module: {spec}")
                };
                Err(Some(packages::Failure { code: "ERR_UNKNOWN_BUILTIN_MODULE", message }))
            } else {
                let from = &self.modules[m].p.path;
                let dir = from[..from.rfind('/').map_or(0, |k| k + 1)].to_string();
                packages::resolve_require(self.host, &dir, &spec).map_err(|f| (f.code != "MODULE_NOT_FOUND").then_some(f))
            };
            let path = match target {
                Ok(path) => path,
                // Not found, or another resolution error (`exports`): thrown when it runs.
                Err(f) => {
                    let failed = f.map(|f| (f.code.to_string(), f.message));
                    self.modules[m].cjs_requests.push((spec, Err(failed)));
                    continue;
                }
            };
            if path.ends_with(".node") {
                self.diag(m, span, format!("unsupported: native addon `{path}` (a `.node` file)"), None);
                continue;
            }
            let k = match self.index.get(&path) {
                Some(&k) => Some(k),
                None => match self.read(&path) {
                    Ok(text) if path.ends_with(".json") => self.load_cjs_json(path.clone(), &text)?,
                    Ok(text) => Some(self.load(path.clone(), &text)?),
                    Err(_) => None,
                },
            };
            self.modules[m].cjs_requests.push((spec, k.ok_or(None)));
        }
        Ok(())
    }

    /// A `require` whose specifier is computed (slice 4): the files its patterns match (relative to
    /// the module, as for `import()`) are loaded and looked up by absolute path when it runs; for
    /// one the build can't work out, `--include` names the modules.
    fn cjs_computed(&mut self, m: usize, computed: Vec<(Span, Option<Vec<Vec<Part>>>)>) -> Result<(), Diagnostics> {
        let from = self.modules[m].p.path.clone();
        let dir = from[..from.rfind('/').map_or(0, |k| k + 1)].to_string();
        for (span, pats) in computed {
            // Unlike `import()`, not a build error: CommonJS code does this on paths that never
            // run (optional plugins, native addons); when it runs it finds only what the build has
            // (`--include`), else node's `MODULE_NOT_FOUND` with a hint.
            let _ = span;
            let Some(pats) = pats.filter(|ps| ps.iter().all(|p| is_relative_pattern(p))) else { continue };
            for mut pat in pats {
                if let [Part::Lit(first), ..] = &pat[..]
                    && !first.starts_with('/')
                {
                    pat[0] = Part::Lit(format!("{dir}{first}"));
                }
                // node adds `.js`/`.json` when the pattern has no extension.
                let mut paths = self.expand(&pat);
                if paths.is_empty() && !matches!(pat.last(), Some(Part::Lit(t)) if t.ends_with(".js") || t.ends_with(".json") || t.ends_with(".cjs") || t.ends_with(".mjs")) {
                    for ext in [".js", ".json"] {
                        let mut with = pat.clone();
                        with.push(Part::Lit(ext.into()));
                        paths.extend(self.expand(&merge(with)));
                    }
                }
                for path in paths {
                    let k = match self.index.get(&path) {
                        Some(&k) => Some(k),
                        None => match self.host.read(&path) {
                            Ok(text) if path.ends_with(".json") => self.load_cjs_json(path.clone(), &text)?,
                            Ok(text) => Some(self.load(path.clone(), &text)?),
                            Err(_) => None,
                        },
                    };
                    if let Some(k) = k {
                        let key = absolute(&path);
                        if !self.cjs_known.iter().any(|(p, _)| *p == key) {
                            self.cjs_known.push((key, k));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Module `m`'s `import()` expressions: the modules each can load join the graph and the table;
    /// one whose specifier the build can't work out needs `--include`.
    fn dynamic_imports(&mut self, m: usize) -> Result<(), Diagnostics> {
        let p = &self.modules[m].p;
        let mut found = Vec::new();
        for node in p.semantic.nodes().iter() {
            let AstKind::ImportExpression(i) = node.kind() else { continue };
            if i.phase.is_some() {
                found.push((i.span, 0, Err("unsupported: import phases (`import.source()`, `import.defer()`) (planned: not scheduled)")));
                continue;
            }
            let arg = i.source.span().start;
            let pats = patterns(p, &i.source, 0).map(|ps| ps.into_iter().map(merge).collect::<Vec<_>>());
            found.push((i.span, arg, Ok(pats)));
        }
        for (span, arg, pats) in found {
            let pats = match pats {
                Err(message) => {
                    self.diag(m, span, message.into(), None);
                    continue;
                }
                Ok(p) => p,
            };
            self.modules[m].dynamic.push(Dynamic { span, arg });
            let known = pats.filter(|ps| ps.iter().all(|p| is_relative_pattern(p) || matches!(p[..], [Part::Lit(_)])));
            let Some(pats) = known else {
                if self.include.is_empty() {
                    self.diag(
                        m,
                        span,
                        "`import()` of a module the build can't work out (only string literals, `const`s, `?:` and template or `+` patterns starting with `./`, `../` or `/`)".into(),
                        Some("name the modules it may load with `--include <glob>` (e.g. `--include 'plugins/*.js'`)".into()),
                    );
                }
                continue;
            };
            let from = &self.modules[m].p.path;
            let dir = match from.rfind('/') {
                Some(k) => from[..=k].to_string(),
                None => String::new(),
            };
            for pat in pats {
                let [Part::Lit(first), ..] = &pat[..] else { continue };
                if !is_relative(first) {
                    // A package (slice 5): resolved now, looked up by the importer's directory
                    // and the specifier. A `node:` or other URL specifier rejects when it runs,
                    // unless it's a built-in js2wasi3 has (T55).
                    let builtin = if pat.len() == 1 { node::path(first) } else { None };
                    if let Some(path) = builtin {
                        // Keyed by the name without `node:` (the run time strips it too).
                        let name = first.strip_prefix("node:").unwrap_or(first);
                        self.pending.push((format!("{}\n{name}", absolute_dir(&dir)), path));
                    } else if pat.len() == 1 && !first.contains(':') {
                        let key = format!("{}\n{first}", absolute_dir(&dir));
                        let from = absolute(&self.modules[m].p.path);
                        let failed = |message: String, code: &str| ("Error".to_string(), message, code.to_string());
                        if packages::is_builtin(first) {
                            let message = format!("node's built-in module '{first}' isn't available in js2wasi3");
                            self.bare_failed.push((key, failed(message, "ERR_UNKNOWN_BUILTIN_MODULE")));
                        } else {
                            match packages::resolve(self.host, &dir, first) {
                                Ok(path) => self.pending.push((key, path)),
                                Err(f) => self.bare_failed.push((key, failed(format!("{} imported from {from}", f.message), f.code))),
                            }
                        }
                    }
                    continue;
                }
                let mut full = pat.clone();
                if !first.starts_with('/') {
                    full[0] = Part::Lit(format!("{dir}{first}"));
                }
                let paths = self.expand(&full);
                self.pending.extend(paths.into_iter().map(|p| (absolute(&p), p)));
            }
        }
        Ok(())
    }

    /// Load `path` for `import()` (a JSON module by its extension) and add it to the table; a file
    /// that isn't there is left out (its `import()` rejects, as in node).
    fn dynamic_target(&mut self, key: String, path: String) -> Result<(), Diagnostics> {
        let json = path.ends_with(".json");
        let k = match self.index.get(&path) {
            Some(&k) => k,
            None => match self.read(&path) {
                Ok(text) => match self.load_file(path.clone(), &text, json)? {
                    Some(k) => k,
                    None => return Ok(()),
                },
                Err(_) => return Ok(()),
            },
        };
        if !self.table.iter().any(|(t, _)| *t == key) {
            self.table.push((key, k));
        }
        Ok(())
    }

    /// The `--include` globs' modules (relative to the working directory).
    fn includes(&mut self) -> Result<(), Diagnostics> {
        for glob in self.include.clone() {
            let mut pat = Vec::new();
            for (i, piece) in glob.split('*').enumerate() {
                if i > 0 {
                    pat.push(Part::Wild);
                }
                if !piece.is_empty() {
                    pat.push(Part::Lit(piece.to_string()));
                }
            }
            let paths = self.expand(&pat);
            if paths.is_empty() {
                self.diags.push(Diagnostic {
                    path: glob.clone(),
                    line: 1,
                    col: 1,
                    message: "--include: the glob matches no module (`.js`, `.mjs`, `.cjs` or `.json` file)".into(),
                    help: None,
                });
            }
            self.pending.extend(paths.into_iter().map(|p| (absolute(&p), p)));
        }
        Ok(())
    }

    /// The module files a pattern names: `*` (`Part::Wild`) matches within one path segment, and
    /// a segment `**` any number of directories. Only `.js`, `.mjs`, `.cjs` and `.json` files.
    fn expand(&mut self, pat: &[Part]) -> Vec<String> {
        // Split into segments at `/`.
        let mut segs: Vec<Vec<Part>> = vec![Vec::new()];
        for part in pat {
            match part {
                Part::Wild => segs.last_mut().unwrap().push(Part::Wild),
                Part::Lit(t) => {
                    for (i, piece) in t.split('/').enumerate() {
                        if i > 0 {
                            segs.push(Vec::new());
                        }
                        if !piece.is_empty() {
                            segs.last_mut().unwrap().push(Part::Lit(piece.to_string()));
                        }
                    }
                }
            }
        }
        let absolute = segs.len() > 1 && segs[0].is_empty();
        let mut out = Vec::new();
        let start = if absolute { "/".to_string() } else { String::new() };
        let segs: Vec<Vec<Part>> = segs.into_iter().filter(|s| !s.is_empty()).collect();
        self.walk(start, &segs, &mut out);
        out.sort();
        out.dedup();
        out
    }

    fn walk(&mut self, dir: String, segs: &[Vec<Part>], out: &mut Vec<String>) {
        let Some((seg, rest)) = segs.split_first() else { return };
        let join = |name: &str| {
            if dir.is_empty() || dir.ends_with('/') {
                format!("{dir}{name}")
            } else {
                format!("{dir}/{name}")
            }
        };
        if let [Part::Lit(name)] = &seg[..] {
            let path = normalize(&join(name));
            if !rest.is_empty() {
                self.walk(path, rest, out);
            } else if is_module_file(name) {
                out.push(path);
            }
            return;
        }
        // `**` (from `--include`): this directory and every one below.
        let doublestar = seg.len() == 2 && seg.iter().all(|p| *p == Part::Wild);
        let Ok(entries) = self.host.list(&dir) else { return };
        if doublestar {
            for (name, is_dir) in entries {
                if is_dir && !name.starts_with('.') && name != "node_modules" {
                    self.walk(join(&name), segs, out);
                }
            }
            return self.walk(dir, rest, out);
        }
        for (name, is_dir) in entries {
            if name.starts_with('.') || !glob_match(seg, &name) {
                continue;
            }
            if rest.is_empty() {
                if !is_dir && is_module_file(&name) {
                    out.push(normalize(&join(&name)));
                }
            } else if is_dir {
                self.walk(join(&name), rest, out);
            }
        }
    }

    /// Load a file's text as a module (`json`: a JSON module, checked first).
    fn load_file(&mut self, path: String, text: &str, json: bool) -> Result<Option<usize>, Diagnostics> {
        if !json {
            return self.load(path, text).map(Some);
        }
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        match json_module(text, "export default ") {
            Ok(source) => self.load(path, &source).map(Some),
            Err((at, what)) if self.lazy => {
                let m = self.load(path, "")?;
                let (line, col) = line_col(text, at as u32);
                self.modules[m].failed = Some(("SyntaxError".into(), format!("invalid JSON at {line}:{col}: {what}"), String::new()));
                Ok(Some(m))
            }
            Err((at, what)) => {
                let (line, col) = line_col(text, at as u32);
                self.diags.push(Diagnostic {
                    path,
                    line,
                    col,
                    message: format!("invalid JSON: {what}"),
                    help: None,
                });
                Ok(None)
            }
        }
    }

    /// Resolve and load `source`, requested by module `m`: `None` for a WIT interface or an error.
    /// `json`: imported `with { type: "json" }` (a JSON module).
    fn request(&mut self, m: usize, source: &StringLiteral, json: bool) -> Result<Option<usize>, Diagnostics> {
        let spec = source.value.as_str();
        let path = match self.resolve(m, spec, source.span) {
            Some(Request::File(path)) => path,
            _ => return Ok(None),
        };
        // node decides by the extension (`.json`), and wants the attribute to agree.
        if json != path.ends_with(".json") {
            let message = if json {
                format!("module `{spec}` is not of type \"json\" (node's ERR_IMPORT_ATTRIBUTE_TYPE_INCOMPATIBLE)")
            } else {
                format!("module `{spec}` needs an import attribute of \"type: json\" (node's ERR_IMPORT_ATTRIBUTE_MISSING)")
            };
            let help = (!json).then(|| format!("`import … from \"{spec}\" with {{ type: \"json\" }}`"));
            self.diag(m, source.span, message, help);
            return Ok(None);
        }
        if let Some(&k) = self.index.get(&path) {
            return Ok(Some(k));
        }
        match self.read(&path) {
            Ok(text) => self.load_file(path, &text, json),
            Err(e) => {
                let help = (!spec.ends_with(".js") && !spec.ends_with(".mjs"))
                    .then(|| format!("node needs the file's extension: `{spec}.js`?"));
                self.diag(
                    m,
                    source.span,
                    format!("cannot find module `{spec}` ({path}: {e})"),
                    help,
                );
                Ok(None)
            }
        }
    }

    fn resolve(&mut self, m: usize, spec: &str, span: Span) -> Option<Request> {
        if spec.starts_with("./") || spec.starts_with("../") || spec.starts_with('/') {
            let from = &self.modules[m].p.path;
            let dir = match from.rfind('/') {
                Some(k) => &from[..=k],
                None => "",
            };
            let joined = if spec.starts_with('/') {
                spec.to_string()
            } else {
                format!("{dir}{spec}")
            };
            return Some(Request::File(normalize(&joined)));
        }
        if let Some(path) = node::path(spec) {
            return Some(Request::File(path));
        }
        if let Some(name) = spec.strip_prefix("node:") {
            self.diag(
                m,
                span,
                format!("unsupported: node's built-in module `node:{name}` (planned: F238)"),
                None,
            );
            return None;
        }
        if spec.contains(':') {
            return Some(Request::Wit);
        }
        if packages::is_builtin(spec) {
            self.diag(
                m,
                span,
                format!("unsupported: node's built-in module `{spec}` (planned: F238)"),
                None,
            );
            return None;
        }
        // A package (slice 5), or `#name` from the package's `imports`.
        let from = self.modules[m].p.path.clone();
        let dir = match from.rfind('/') {
            Some(k) => &from[..=k],
            None => "",
        };
        match packages::resolve(self.host, dir, spec) {
            Ok(path) => Some(Request::File(path)),
            Err(f) => {
                self.diag(m, span, format!("{} imported from {from} ({})", f.message, f.code), None);
                None
            }
        }
    }

    /// Record module `m`'s import bindings and exports.
    fn scan(&mut self, m: usize) {
        let ast = self.modules[m].p.ast;
        for stmt in &ast.body {
            let target = self.modules[m].targets.get(&stmt.span().start).copied();
            match stmt {
                Statement::ImportDeclaration(i) => {
                    let Some(module) = target else { continue };
                    for s in i.specifiers.iter().flatten() {
                        let (local, name) = match s {
                            ImportDeclarationSpecifier::ImportSpecifier(s) => {
                                (&s.local, Name::Name(s.imported.name().to_string()))
                            }
                            ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                                (&s.local, Name::Name("default".into()))
                            }
                            ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                                (&s.local, Name::Namespace)
                            }
                        };
                        let import = Import {
                            module,
                            name,
                            specifier: i.source.value.to_string(),
                            span: s.span(),
                        };
                        self.modules[m].imports.insert(local.symbol_id(), import);
                    }
                }
                Statement::ExportFromDeclaration(e) => {
                    let Some(module) = target else { continue };
                    for s in &e.specifiers {
                        let name = Name::Name(s.local.name().to_string());
                        let spec = e.source.value.to_string();
                        self.modules[m].indirect.push((s.exported.name().to_string(), module, name, spec, s.span));
                    }
                }
                Statement::ExportAllDeclaration(e) => {
                    let Some(module) = target else { continue };
                    match &e.exported {
                        Some(n) => {
                            let spec = e.source.value.to_string();
                            self.modules[m].indirect.push((n.name().to_string(), module, Name::Namespace, spec, e.span));
                        }
                        None => self.modules[m].stars.push(module),
                    }
                }
                Statement::ExportDefaultDeclaration(e) => {
                    let id = match &e.declaration {
                        ExportDefaultDeclarationKind::FunctionDeclaration(f) => f.id.as_ref(),
                        ExportDefaultDeclarationKind::ClassDeclaration(c) => c.id.as_ref(),
                        _ => None,
                    };
                    let b = match id {
                        Some(id) => Binding::Sym(m, id.symbol_id()),
                        None => Binding::Default(m),
                    };
                    self.modules[m].locals.push(("default".into(), b));
                }
                _ => {}
            }
        }
        // `export function f`, `export const a`, `export { a as b }`: from the export list
        // (`export default …` was handled above).
        let p = &self.modules[m].p;
        let scoping = p.semantic.scoping();
        let defaults: Vec<Span> = ast
            .body
            .iter()
            .filter(|s| matches!(s, Statement::ExportDefaultDeclaration(_)))
            .map(|s| s.span())
            .collect();
        let mut locals = Vec::new();
        for e in &p.exports {
            if e.from.is_some() || defaults.iter().any(|d| d.start <= e.span.start && e.span.end <= d.end) {
                continue;
            }
            let sym = e.local.as_deref().and_then(|l| scoping.get_root_binding(l.into()));
            if let Some(sym) = sym {
                locals.push((e.name.clone(), Binding::Sym(m, sym)));
            }
        }
        self.modules[m].locals.extend(locals);
    }

    /// What ES modules see of each CommonJS module (T54): `default` and `'module.exports'` are its
    /// `module.exports`, and the names node's lexer finds (with its re-exports') its properties.
    fn cjs_exports(&mut self) {
        let n = self.modules.len();
        let mut own: Vec<(Vec<String>, Vec<usize>)> = vec![(Vec::new(), Vec::new()); n];
        for (m, o) in own.iter_mut().enumerate() {
            if !self.modules[m].cjs {
                continue;
            }
            let (names, reexports) = match node::exports(&self.modules[m].p.path) {
                Some(names) => (names.iter().map(|n| n.to_string()).collect(), Vec::new()),
                None => cjs_exports(&self.modules[m].p),
            };
            let targets = reexports
                .iter()
                .filter_map(|spec| self.modules[m].cjs_requests.iter().find(|(s, _)| s == spec).and_then(|(_, t)| t.as_ref().ok().copied()))
                .collect();
            *o = (names, targets);
        }
        for m in 0..n {
            if !self.modules[m].cjs {
                for k in self.modules[m].requests.clone() {
                    self.modules[k].esm_imported = true;
                }
                continue;
            }
            let mut names = Vec::new();
            let mut todo = vec![m];
            let mut seen = vec![m];
            while let Some(k) = todo.pop() {
                names.extend(own[k].0.iter().cloned());
                for &t in &own[k].1 {
                    if !seen.contains(&t) {
                        seen.push(t);
                        todo.push(t);
                    }
                }
            }
            names.retain(|n| n != "default" && n != "module.exports");
            names.sort();
            names.dedup();
            let mut locals = vec![("default".to_string(), Binding::CjsValue(m))];
            // A node built-in's namespace has no `'module.exports'` (T55).
            if !node::is_module(&self.modules[m].p.path) {
                locals.push(("module.exports".to_string(), Binding::CjsValue(m)));
            }
            locals.extend(names.iter().enumerate().map(|(i, name)| (name.clone(), Binding::CjsNamed(m, i))));
            self.modules[m].locals = locals;
            self.modules[m].cjs_names = names;
        }
        for &(_, k) in &self.table {
            self.modules[k].esm_imported = true;
        }
    }

    /// ResolveExport: the binding module `m` exports as `name`; `Err` if star exports conflict.
    fn resolve_export(
        &self,
        m: usize,
        name: &str,
        seen: &mut Vec<(usize, String)>,
    ) -> Result<Option<Binding>, ()> {
        if seen.iter().any(|(k, n)| *k == m && n == name) {
            return Ok(None);
        }
        seen.push((m, name.to_string()));
        let module = &self.modules[m];
        if let Some((_, b)) = module.locals.iter().find(|(n, _)| n == name) {
            return self.follow(*b, seen);
        }
        if let Some((_, t, n, _, _)) = module.indirect.iter().find(|(n, ..)| n == name) {
            return match n {
                Name::Name(n) => self.resolve_export(*t, n, seen),
                Name::Namespace => Ok(Some(Binding::Namespace(*t))),
            };
        }
        if name == "default" {
            return Ok(None);
        }
        let mut found = None;
        for &t in &module.stars {
            if let Some(b) = self.resolve_export(t, name, seen)? {
                match found {
                    Some(f) if f != b => return Err(()),
                    _ => found = Some(b),
                }
            }
        }
        Ok(found)
    }

    /// A local binding, or what it imports.
    fn follow(&self, b: Binding, seen: &mut Vec<(usize, String)>) -> Result<Option<Binding>, ()> {
        let Binding::Sym(m, sym) = b else { return Ok(Some(b)) };
        match self.modules[m].imports.get(&sym) {
            Some(Import { module, name: Name::Name(n), .. }) => self.resolve_export(*module, n, seen),
            Some(Import { module, name: Name::Namespace, .. }) => Ok(Some(Binding::Namespace(*module))),
            None => Ok(Some(b)),
        }
    }

    /// GetExportedNames: every name module `m` exports, in a stable order.
    fn exported_names(&self, m: usize, seen: &mut Vec<usize>) -> Vec<String> {
        if seen.contains(&m) {
            return Vec::new();
        }
        seen.push(m);
        let module = &self.modules[m];
        let mut names: Vec<String> = module.locals.iter().map(|(n, _)| n.clone()).collect();
        names.extend(module.indirect.iter().map(|(n, ..)| n.clone()));
        for &t in &module.stars {
            for n in self.exported_names(t, seen) {
                if n != "default" && !names.contains(&n) {
                    names.push(n);
                }
            }
        }
        names
    }

    fn join(mut self) -> Result<Linked, Diagnostics> {
        // Every import binding → what it names.
        let mut binds: HashMap<(usize, SymbolId), Binding> = HashMap::new();
        let mut errors = Vec::new();
        for m in 0..self.modules.len() {
            let mut imports: Vec<_> = self.modules[m].imports.iter().collect();
            imports.sort_by_key(|(_, i)| i.span.start);
            for (&sym, i) in imports {
                let found = match &i.name {
                    Name::Namespace => Ok(Some(Binding::Namespace(i.module))),
                    Name::Name(n) => self.resolve_export(i.module, n, &mut Vec::new()),
                };
                let name = match &i.name {
                    Name::Name(n) => n.as_str(),
                    Name::Namespace => "*",
                };
                match found {
                    Ok(Some(b)) => {
                        binds.insert((m, sym), b);
                    }
                    Ok(None) if self.modules[i.module].cjs => errors.push((
                        m,
                        i.span,
                        format!(
                            "SyntaxError: Named export '{name}' not found. The requested module '{}' is a CommonJS module, which may not support all module.exports as named exports.",
                            i.specifier
                        ),
                    )),
                    Ok(None) => errors.push((
                        m,
                        i.span,
                        format!(
                            "SyntaxError: The requested module '{}' does not provide an export named '{name}'",
                            i.specifier
                        ),
                    )),
                    Err(()) => errors.push((
                        m,
                        i.span,
                        format!(
                            "SyntaxError: The requested module '{}' contains conflicting star exports for name '{name}'",
                            i.specifier
                        ),
                    )),
                }
            }
        }
        // `export { x } from` must name something (InitializeEnvironment's check).
        for m in 0..self.modules.len() {
            for (exported, _, name, spec, span) in &self.modules[m].indirect {
                let Name::Name(name) = name else { continue };
                let what = match self.resolve_export(m, exported, &mut Vec::new()) {
                    Ok(Some(_)) => continue,
                    Ok(None) => "does not provide an export named",
                    Err(()) => "contains conflicting star exports for name",
                };
                errors.push((m, *span, format!("SyntaxError: The requested module '{spec}' {what} '{name}'")));
            }
        }
        // The entry's exports (the WIT exports of a library), resolved.
        let mut entry_exports = Vec::new();
        for name in self.exported_names(0, &mut Vec::new()) {
            match self.resolve_export(0, &name, &mut Vec::new()) {
                Ok(Some(Binding::Namespace(_))) if !self.command => errors.push((
                    0,
                    Span::new(0, 0),
                    format!("unsupported: exporting a module namespace object as `{name}` (planned: T41 slice 2)"),
                )),
                Ok(Some(b)) => entry_exports.push((name, b)),
                // An ambiguous star export isn't exported (and one that can't be found is a
                // missing export, reported where it's imported).
                _ => {}
            }
        }
        for (m, span, message) in errors {
            self.diag(m, span, message, None);
        }
        if !self.diags.is_empty() {
            return Err(sorted(self.diags));
        }
        // A module `import()` loads fails if it or a module it imports fails (the first found).
        fn poison(g: &Graph, m: usize, seen: &mut Vec<bool>) -> Option<Failed> {
            if seen[m] {
                return None;
            }
            seen[m] = true;
            if let Some(f) = &g.modules[m].failed {
                return Some(f.clone());
            }
            g.modules[m].requests.iter().find_map(|&r| poison(g, r, seen))
        }
        self.poisoned = (0..self.modules.len())
            .map(|m| poison(&self, m, &mut vec![false; self.modules.len()]))
            .collect();

        let namespaces = self.namespaces(&binds);
        let names = self.names(&binds, &namespaces);
        let order = self.order();
        let mut map = SourceMap::default();
        let mut out = String::new();
        // Each module's `import.meta` (slice 3): node's properties, from the file's absolute path.
        for m in 0..self.modules.len() {
            if let Some(name) = names.get(&Binding::Meta(m)).filter(|_| self.poisoned[m].is_none()) {
                let path = &self.modules[m].p.path;
                let abs = std::fs::canonicalize(path)
                    .or_else(|_| std::path::absolute(path))
                    .map_or_else(|_| path.clone(), |p| p.to_string_lossy().into_owned());
                let dir = match abs.rfind('/') {
                    Some(0) => "/",
                    Some(k) => &abs[..k],
                    None => ".",
                };
                out.push_str(&format!(
                    "const {name} = $$ImportMeta({}, {}, {}, {});\n",
                    js_string(&file_url(&abs)),
                    js_string(&abs),
                    js_string(dir),
                    m == 0
                ));
            }
        }
        // The namespace objects, made before any module runs (their getters read the bindings).
        for (t, exports) in &namespaces {
            let keys: Vec<String> = exports.iter().map(|(k, _)| js_string(k)).collect();
            let getters: Vec<String> = exports.iter().map(|(_, b)| format!("() => {}", names[b])).collect();
            out.push_str(&format!(
                "const {} = $$ModuleNamespace([{}], [{}]);\n",
                names[&Binding::Namespace(*t)],
                keys.join(", "),
                getters.join(", ")
            ));
        }
        let command = self.command || self.modules[0].p.exports.is_empty();
        let dynamic = self.modules.iter().any(|m| !m.dynamic.is_empty());
        // T54: `require` of an ES module evaluates it (synchronously) with the same algorithm.
        let require_esm = self.modules.iter().any(|m| m.cjs_requests.iter().any(|(_, t)| t.as_ref().is_ok_and(|t| !self.modules[*t].cjs)))
            || self.cjs_known.iter().any(|(_, k)| !self.modules[*k].cjs);
        let evaluate = (command && self.modules[1..].iter().any(|m| m.tla)) || dynamic || require_esm;
        let at: HashMap<usize, usize> = order.iter().enumerate().map(|(k, m)| (*m, k)).collect();
        // The CommonJS modules (T54): [function, filename, dirname, requests, module, parent],
        // requests being [specifier, index] (-1: not found); `module` and `parent` are set when
        // it's first required.
        let cjs: Vec<usize> = (0..self.modules.len()).filter(|&m| self.modules[m].cjs).collect();
        if !cjs.is_empty() {
            let rows: Vec<String> = cjs
                .iter()
                .map(|&m| {
                    let (file, dir) = abs_file_dir(&self.modules[m].p.path);
                    let reqs: Vec<String> = self.modules[m]
                        .cjs_requests
                        .iter()
                        .map(|(spec, t)| match t {
                            // An ES module: [specifier, -3, its evaluation index, namespace, filename].
                            Ok(t) if !self.modules[*t].cjs => format!(
                                "[{}, -3, {}, {}, {}]",
                                js_string(spec),
                                at[t],
                                names[&Binding::Namespace(*t)],
                                js_string(&abs_file_dir(&self.modules[*t].p.path).0)
                            ),
                            Ok(t) => format!("[{}, {}]", js_string(spec), cjs.iter().position(|c| c == t).expect("a CommonJS module")),
                            Err(None) => format!("[{}, -1]", js_string(spec)),
                            Err(Some((code, message))) => {
                                format!("[{}, -1, {}, {}]", js_string(spec), js_string(code), js_string(message))
                            }
                        })
                        .collect();
                    format!(
                        "[{}, {}, {}, [{}], undefined, -1]",
                        names[&Binding::Cjs(m)],
                        js_string(&file),
                        js_string(&dir),
                        reqs.join(", ")
                    )
                })
                .collect();
            out.push_str(&format!("const {} = [{}];\n", names[&Binding::CjsTable], rows.join(", ")));
            // What a computed `require` finds (slice 4): [absolute path, index] or an ES module's row.
            let known: Vec<String> = self
                .cjs_known
                .iter()
                .filter(|(_, k)| self.poisoned[*k].is_none())
                .map(|(key, k)| {
                    if self.modules[*k].cjs {
                        format!("[{}, {}]", js_string(key), cjs.iter().position(|c| c == k).expect("a CommonJS module"))
                    } else {
                        format!("[{}, -3, {}, {}, {}]", js_string(key), at[k], names[&Binding::Namespace(*k)], js_string(key))
                    }
                })
                .collect();
            if !known.is_empty() {
                out.push_str(&format!("$$cjsKnown([{}]);\n", known.join(", ")));
            }
        }
        // A dependency with top-level `await` (slice 3, commands only: a library's `init` can't
        // wait) or an `import()` (slice 4): every module becomes a function, run by the spec's
        // algorithm; `import()` evaluates the modules it loads the same way.
        if evaluate {
            let tla: Vec<&str> = order.iter().map(|m| if self.modules[*m].tla { "true" } else { "false" }).collect();
            let requests: Vec<String> = order
                .iter()
                .map(|m| {
                    let mut r: Vec<usize> = Vec::new();
                    for t in &self.modules[*m].requests {
                        if !r.contains(&at[t]) {
                            r.push(at[t]);
                        }
                    }
                    let r: Vec<String> = r.iter().map(|k| k.to_string()).collect();
                    format!("[{}]", r.join(", "))
                })
                .collect();
            // For `import()`: each importer's directory and file (absolute), and the table.
            let paths: Vec<String> = order
                .iter()
                .map(|m| {
                    if self.modules[*m].dynamic.is_empty() {
                        return "null".into();
                    }
                    let abs = absolute(&self.modules[*m].p.path);
                    let dir = &abs[..=abs.rfind('/').unwrap_or(0)];
                    format!("[{}, {}]", js_string(dir), js_string(&abs))
                })
                .collect();
            let failed = |key: &str, json: bool, (class, message, code): &Failed| {
                format!(
                    "[{}, -1, {json}, null, {}, {}, {}]",
                    js_string(key),
                    js_string(class),
                    js_string(message),
                    js_string(code)
                )
            };
            let mut table: Vec<String> = self
                .table
                .iter()
                .map(|(key, k)| {
                    let json = self.modules[*k].p.path.ends_with(".json");
                    match &self.poisoned[*k] {
                        Some(f) => failed(key, json, f),
                        None => format!("[{}, {}, {json}, {}]", js_string(key), at[k], names[&Binding::Namespace(*k)]),
                    }
                })
                .collect();
            table.extend(self.bare_failed.iter().map(|(key, f)| failed(key, false, f)));
            out.push_str(&format!(
                "{}$$EvaluateModules(__intrinsic_module_bodies(), [{}], [{}], [{}], [{}], {}, {});\n",
                if command { "await " } else { "" },
                tla.join(", "),
                requests.join(", "),
                paths.join(", "),
                table.join(", "),
                at[&0],
                !command
            ));
        }
        map.generated = out.len() as u32;
        if !out.is_empty() {
            map.segments.push(Segment {
                at: 0,
                file: 0,
                offset: 0,
                copied: false,
            });
        }
        for &m in &order {
            let mut edits = self.edits(m, &binds, &names, &entry_exports);
            // `import(x, o)` → `$$DynamicImport(<importer>, x, o)` (slice 4).
            for d in &self.modules[m].dynamic {
                edits.push(Edit::new(d.span.start, d.arg, format!("$$DynamicImport({}, ", at[&m])));
            }
            edits.sort_by_key(|e| (e.start, e.end));
            if !out.is_empty() {
                // A module's last statement may rely on ASI: end it.
                map.segments.push(Segment {
                    at: out.len() as u32,
                    file: m as u32,
                    offset: 0,
                    copied: false,
                });
                out.push_str("\n;\n");
            }
            let start = out.len() as u32;
            let source = self.modules[m].p.source;
            let mut pos = 0;
            for e in edits {
                if e.start < pos {
                    continue; // inside a deleted statement
                }
                map.segments.push(Segment {
                    at: out.len() as u32,
                    file: m as u32,
                    offset: pos,
                    copied: true,
                });
                out.push_str(&source[pos as usize..e.start as usize]);
                map.segments.push(Segment {
                    at: out.len() as u32,
                    file: m as u32,
                    offset: e.start,
                    copied: false,
                });
                if let Some((at, name)) = e.written {
                    let at = out.len() as u32 + at;
                    if e.import {
                        map.imports.insert(at);
                    }
                    map.refs.insert(at, name);
                }
                out.push_str(&e.text);
                pos = e.end;
            }
            map.segments.push(Segment {
                at: out.len() as u32,
                file: m as u32,
                offset: pos,
                copied: true,
            });
            out.push_str(&source[pos as usize..]);
            // A CommonJS module an ES module imports runs where ESM evaluation reaches it; its
            // named exports are what `module.exports` has then (node's snapshot).
            if self.modules[m].cjs && self.modules[m].esm_imported {
                map.segments.push(Segment {
                    at: out.len() as u32,
                    file: m as u32,
                    offset: 0,
                    copied: false,
                });
                let value = &names[&Binding::CjsValue(m)];
                let k = cjs.iter().position(|&c| c == m).expect("a CommonJS module");
                out.push_str(&format!("\nconst {value} = $$cjsImport({}, {k});\n", names[&Binding::CjsTable]));
                for (i, name) in self.modules[m].cjs_names.iter().enumerate() {
                    out.push_str(&format!("const {} = $$cjsNamed({value}, {});\n", names[&Binding::CjsNamed(m, i)], js_string(name)));
                }
            }
            // A CommonJS entry runs as node's main module.
            if m == 0 && self.modules[0].cjs {
                map.segments.push(Segment {
                    at: out.len() as u32,
                    file: 0,
                    offset: 0,
                    copied: false,
                });
                out.push_str(&format!("\n$$cjsMain({}, {});\n", names[&Binding::CjsTable], cjs.iter().position(|&c| c == 0).expect("the entry")));
            }
            if evaluate {
                map.modules.push((start, out.len() as u32));
            }
        }
        for (b, name) in &names {
            let original = match *b {
                Binding::Sym(m, s) => self.modules[m].p.semantic.scoping().symbol_name(s),
                Binding::Default(_) => "default",
                Binding::Namespace(_) | Binding::Meta(_) | Binding::Cjs(_) | Binding::CjsTable => continue,
                Binding::CjsValue(_) | Binding::CjsNamed(..) => continue,
            };
            if original != name {
                map.names.insert(name.clone(), original.to_string());
            }
        }
        map.files = self
            .modules
            .iter()
            .map(|m| (m.p.path.clone(), m.p.original.to_string()))
            .collect();
        map.evals = self.modules.iter().map(|m| m.p.evals.clone()).collect();
        Ok(Linked {
            source: out,
            map: Some(map),
        })
    }

    /// Evaluation order: post-order DFS from the entry, requests in source order.
    fn order(&self) -> Vec<usize> {
        fn visit(g: &Graph, m: usize, seen: &mut Vec<bool>, out: &mut Vec<usize>) {
            if seen[m] {
                return;
            }
            seen[m] = true;
            for &t in &g.modules[m].requests {
                visit(g, t, seen, out);
            }
            // What a CommonJS module `require`s runs when it's required: anywhere in the source.
            for (_, t) in &g.modules[m].cjs_requests {
                if let Ok(t) = t {
                    visit(g, *t, seen, out);
                }
            }
            out.push(m);
        }
        let mut seen = vec![false; self.modules.len()];
        let mut out = Vec::new();
        visit(self, 0, &mut seen, &mut out);
        // Modules only a computed `require` loads (T54).
        for &(_, k) in &self.cjs_known {
            visit(self, k, &mut seen, &mut out);
        }
        // Modules only `import()` loads (slice 4): after the entry, each after its dependencies.
        for &(_, k) in &self.table {
            if self.poisoned[k].is_none() {
                visit(self, k, &mut seen, &mut out);
            }
        }
        out
    }

    /// A unique name for each module's top-level bindings (the entry's first).
    /// The namespace objects the program needs (`import * as ns`, `export * as ns`, and those
    /// they export): each module's exports, by name in code-unit order, and their bindings.
    fn namespaces(&self, binds: &HashMap<(usize, SymbolId), Binding>) -> Vec<(usize, Vec<(String, Binding)>)> {
        let mut todo: Vec<usize> = binds
            .iter()
            .filter(|((m, _), _)| self.poisoned[*m].is_none())
            .filter_map(|(_, b)| match b {
                Binding::Namespace(t) => Some(*t),
                _ => None,
            })
            .collect();
        // Every module `import()` can load (slice 4).
        todo.extend(self.table.iter().map(|(_, k)| *k).filter(|k| self.poisoned[*k].is_none()));
        // Every ES module a CommonJS module requires (T54).
        for m in &self.modules {
            todo.extend(m.cjs_requests.iter().filter_map(|(_, t)| t.as_ref().ok().copied()).filter(|t| !self.modules[*t].cjs));
        }
        todo.extend(self.cjs_known.iter().map(|(_, k)| *k).filter(|k| !self.modules[*k].cjs && self.poisoned[*k].is_none()));
        todo.sort();
        todo.dedup();
        let mut out: Vec<(usize, Vec<(String, Binding)>)> = Vec::new();
        while let Some(t) = todo.pop() {
            if out.iter().any(|(k, _)| *k == t) {
                continue;
            }
            let mut names = self.exported_names(t, &mut Vec::new());
            names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let mut exports = Vec::new();
            for name in names {
                // An ambiguous name isn't exported.
                if let Ok(Some(b)) = self.resolve_export(t, &name, &mut Vec::new()) {
                    if let Binding::Namespace(u) = b {
                        todo.push(u);
                    }
                    exports.push((name, b));
                }
            }
            out.push((t, exports));
        }
        out.sort_by_key(|(t, _)| *t);
        out
    }

    fn names(
        &self,
        binds: &HashMap<(usize, SymbolId), Binding>,
        namespaces: &[(usize, Vec<(String, Binding)>)],
    ) -> HashMap<Binding, String> {
        let mut globals = HashSet::new();
        let mut taken = HashSet::new();
        let mut inner: Vec<HashSet<String>> = Vec::new();
        for m in &self.modules {
            let scoping = m.p.semantic.scoping();
            let root = scoping.root_scope_id();
            globals.extend(scoping.root_unresolved_references().keys().map(|k| k.to_string()));
            let mut names = HashSet::new();
            for s in scoping.symbol_ids() {
                let name = scoping.symbol_name(s).to_string();
                if scoping.symbol_scope_id(s) != root {
                    names.insert(name.clone());
                }
                taken.insert(name);
            }
            inner.push(names);
        }
        taken.extend(globals.iter().cloned());
        let mut importers: HashMap<Binding, Vec<usize>> = HashMap::new();
        for (&(m, _), &b) in binds {
            importers.entry(b).or_default().push(m);
        }
        let mut used = HashSet::new();
        let mut names = HashMap::new();
        for m in 0..self.modules.len() {
            let scoping = self.modules[m].p.semantic.scoping();
            let root = scoping.root_scope_id();
            let mut own: Vec<(Binding, &str, u32)> = scoping
                .get_bindings(root)
                .values()
                .filter(|s| !self.modules[m].imports.contains_key(s))
                .map(|&s| (Binding::Sym(m, s), scoping.symbol_name(s), scoping.symbol_span(s).start))
                .collect();
            own.sort_by_key(|b| b.2);
            if self.modules[m].locals.iter().any(|(_, b)| *b == Binding::Default(m)) {
                own.push((Binding::Default(m), "default", u32::MAX));
            }
            for (b, original, _) in own {
                let clash = |c: &str| {
                    used.contains(c)
                        || globals.contains(c)
                        || c.starts_with("$$")
                        || importers.get(&b).is_some_and(|i| i.iter().any(|&k| inner[k].contains(c)))
                };
                let mut name = original.to_string();
                if matches!(b, Binding::Default(_)) || clash(&name) {
                    let base = original.trim_start_matches('$');
                    name = (1..)
                        .map(|k| format!("{base}${k}"))
                        .find(|c| !clash(c) && !taken.contains(c))
                        .expect("a free name");
                }
                used.insert(name.clone());
                names.insert(b, name);
            }
        }
        for (t, _) in namespaces {
            let name = (1..)
                .map(|k| format!("namespace${k}"))
                .find(|c| !used.contains(c) && !taken.contains(c) && !globals.contains(c))
                .expect("a free name");
            used.insert(name.clone());
            names.insert(Binding::Namespace(*t), name);
        }
        let fresh = |base: &str, used: &mut HashSet<String>| {
            let name = (1..)
                .map(|k| format!("{base}${k}"))
                .find(|c| !used.contains(c) && !taken.contains(c) && !globals.contains(c))
                .expect("a free name");
            used.insert(name.clone());
            name
        };
        for m in 0..self.modules.len() {
            if self.modules[m].cjs {
                names.insert(Binding::Cjs(m), fresh("cjs", &mut used));
                names.entry(Binding::CjsTable).or_insert_with(|| fresh("modules", &mut used));
                if self.modules[m].esm_imported {
                    names.insert(Binding::CjsValue(m), fresh("exports", &mut used));
                    for (i, name) in self.modules[m].cjs_names.iter().enumerate() {
                        let base = if is_identifier_name(name) && !name.starts_with('$') { name.as_str() } else { "export" };
                        names.insert(Binding::CjsNamed(m, i), fresh(base, &mut used));
                    }
                }
            }
        }
        for m in 0..self.modules.len() {
            if self.modules[m].meta.is_empty() || self.modules[m].cjs {
                continue;
            }
            let name = (1..)
                .map(|k| format!("meta${k}"))
                .find(|c| !used.contains(c) && !taken.contains(c) && !globals.contains(c))
                .expect("a free name");
            used.insert(name.clone());
            names.insert(Binding::Meta(m), name);
        }
        names
    }

    /// The text edits of module `m`, by position.
    fn edits(
        &self,
        m: usize,
        binds: &HashMap<(usize, SymbolId), Binding>,
        names: &HashMap<Binding, String>,
        entry_exports: &[(String, Binding)],
    ) -> Vec<Edit> {
        let module = &self.modules[m];
        let p = &module.p;
        // A command's entry loses its exports like a dependency.
        let (source, entry) = (p.source, m == 0 && !self.command);
        let scoping = p.semantic.scoping();
        let nodes = p.semantic.nodes();
        let mut edits = Vec::new();
        let del = |edits: &mut Vec<Edit>, start: u32, end: u32| edits.push(Edit::new(start, end, String::new()));
        if module.cjs {
            // The wrapper becomes a (hoisted) function declaration.
            let end = source.len() as u32;
            let mut header = format!("function {}(exports, require, module, __filename, __dirname) {{", names[&Binding::Cjs(m)]);
            for (g, refs) in &module.global_refs {
                let (name, module, export) = node::GLOBALS[*g];
                header.push_str(&format!("const {name} = require(\"{module}\").{export};"));
                // `globalThis.X` reads the same binding (T71).
                for r in refs.iter().filter(|r| r.member) {
                    edits.push(Edit::new(r.span.start, r.span.end, name.to_string()));
                }
            }
            edits.push(Edit::new(0, CJS_PRE.len() as u32, header));
            edits.push(Edit::new(end - CJS_POST.len() as u32, end, "\n}".into()));
            edits.sort_by_key(|e| (e.start, e.end));
            return edits;
        }

        if let Some(h) = &p.ast.hashbang {
            del(&mut edits, h.span.start, h.span.end);
        }
        let mut stars_done = false;
        for stmt in &p.ast.body {
            let span = stmt.span();
            match stmt {
                Statement::ImportDeclaration(_) if module.targets.contains_key(&span.start) => {
                    del(&mut edits, span.start, span.end)
                }
                Statement::ExportDeclaration(d) if !entry => {
                    del(&mut edits, span.start, d.declaration.span().start)
                }
                Statement::ExportDeclaration(d) => {
                    // The entry keeps its exports: a renamed one is exported by its new name.
                    let renamed: Vec<String> = module
                        .locals
                        .iter()
                        .filter_map(|(exported, b)| {
                            let Binding::Sym(_, sym) = b else { return None };
                            let at = scoping.symbol_span(*sym).start;
                            let name = &names[b];
                            (span.start <= at && at < span.end).then(|| format!("{name} as {exported}"))
                        })
                        .collect();
                    if renamed.iter().any(|r| r.split(" as ").next() != r.split(" as ").nth(1)) {
                        del(&mut edits, span.start, d.declaration.span().start);
                        edits.push(Edit::new(span.end, span.end, format!("\nexport {{ {} }};", renamed.join(", "))));
                    }
                }
                Statement::ExportDefaultDeclaration(d) if entry => {
                    // A binding for the entry's default, which a module importing it names.
                    let name = names.get(&Binding::Default(m)).map_or("", String::as_str);
                    match &d.declaration {
                        ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.id.is_none() => {
                            let at = f.params.span.start;
                            let space = if source[..at as usize].ends_with(char::is_whitespace) { "" } else { " " };
                            edits.push(Edit::new(at, at, format!("{space}{name}")));
                        }
                        ExportDefaultDeclarationKind::ClassDeclaration(c) if c.id.is_none() => {
                            let at = c.span.start + "class".len() as u32;
                            edits.push(Edit::new(at, at, format!(" {name}")));
                        }
                        ExportDefaultDeclarationKind::FunctionDeclaration(_)
                        | ExportDefaultDeclarationKind::ClassDeclaration(_) => {}
                        other => {
                            let e = other.as_expression().expect("an expression");
                            edits.push(Edit::new(span.start, e.span().start, format!("const {name} = ")));
                            edits.push(Edit::new(e.span().end, span.end, format!(";\nexport {{ {name} as default }};")));
                        }
                    }
                }
                Statement::ExportNamedDeclaration(_)
                | Statement::ExportFromDeclaration(_)
                | Statement::ExportAllDeclaration(_)
                    if !entry =>
                {
                    del(&mut edits, span.start, span.end)
                }
                Statement::ExportNamedDeclaration(d) => {
                    for s in &d.specifiers {
                        let ModuleExportName::IdentifierReference(r) = &s.local else { continue };
                        let Some(sym) = scoping.get_reference(r.reference_id()).symbol_id() else {
                            continue;
                        };
                        let b = binds.get(&(m, sym)).copied().unwrap_or(Binding::Sym(m, sym));
                        let name = &names[&b];
                        if name != r.name.as_str() {
                            let exported = s.exported.span().source_text(source);
                            edits.push(Edit::new(s.span.start, s.span.end, format!("{name} as {exported}")));
                        }
                    }
                }
                Statement::ExportFromDeclaration(d) => {
                    let list: Vec<String> = d
                        .specifiers
                        .iter()
                        .filter_map(|s| {
                            let exported = s.exported.name();
                            let (_, b) = entry_exports.iter().find(|(n, _)| n == exported.as_str())?;
                            Some(format!("{} as {}", names[b], s.exported.span().source_text(source)))
                        })
                        .collect();
                    edits.push(Edit::new(span.start, span.end, format!("export {{ {} }};", list.join(", "))));
                }
                Statement::ExportAllDeclaration(_) if stars_done => del(&mut edits, span.start, span.end),
                Statement::ExportAllDeclaration(_) => {
                    // Every name the entry exports through a star, at the first `export *`.
                    stars_done = true;
                    let list: Vec<String> = entry_exports
                        .iter()
                        .filter(|(n, _)| {
                            !module.locals.iter().any(|(l, _)| l == n)
                                && !module.indirect.iter().any(|(l, ..)| l == n)
                        })
                        .map(|(n, b)| {
                            let exported = if is_identifier_name(n) { n.clone() } else { format!("{n:?}") };
                            format!("{} as {exported}", names[b])
                        })
                        .collect();
                    edits.push(Edit::new(span.start, span.end, format!("export {{ {} }};", list.join(", "))));
                }
                Statement::ExportDefaultDeclaration(d) if !entry => {
                    let name = names.get(&Binding::Default(m)).map_or("", String::as_str);
                    match &d.declaration {
                        ExportDefaultDeclarationKind::FunctionDeclaration(f) => {
                            del(&mut edits, span.start, f.span.start);
                            if f.id.is_none() {
                                let at = f.params.span.start;
                                let space = if source[..at as usize].ends_with(char::is_whitespace) { "" } else { " " };
                                edits.push(Edit::new(at, at, format!("{space}{name}")));
                            }
                        }
                        ExportDefaultDeclarationKind::ClassDeclaration(c) => {
                            del(&mut edits, span.start, c.span.start);
                            if c.id.is_none() {
                                let at = c.span.start + "class".len() as u32;
                                edits.push(Edit::new(at, at, format!(" {name}")));
                            }
                        }
                        other => {
                            let e = other.as_expression().expect("an expression");
                            edits.push(Edit::new(span.start, e.span().start, format!("const {name} = ")));
                            edits.push(Edit::new(e.span().end, span.end, ";".into()));
                        }
                    }
                }
                _ => {}
            }
        }

        // Identifiers written `{ x }` (a property and a binding at once) and `import { f }`.
        let mut shorthand = HashSet::new();
        let mut import_shorthand = HashSet::new();
        for node in nodes.iter() {
            match node.kind() {
                AstKind::ObjectProperty(o) if o.shorthand => {
                    shorthand.insert(o.key.span().start);
                }
                AstKind::BindingProperty(o) if o.shorthand => {
                    shorthand.insert(o.key.span().start);
                }
                AstKind::AssignmentTargetPropertyIdentifier(o) => {
                    shorthand.insert(o.binding.span.start);
                }
                AstKind::ImportSpecifier(s) if s.imported.span() == s.local.span => {
                    import_shorthand.insert(s.local.span.start);
                }
                _ => {}
            }
        }
        let ident = |edits: &mut Vec<Edit>, span: Span, original: &str, name: &str, reference: bool| {
            let (text, at) = if name == original {
                (name.to_string(), 0)
            } else if shorthand.contains(&span.start) {
                (format!("{original}: {name}"), original.len() as u32 + 2)
            } else if import_shorthand.contains(&span.start) {
                (format!("{original} as {name}"), original.len() as u32 + 4)
            } else {
                (name.to_string(), 0)
            };
            let mut e = Edit::new(span.start, span.end, text);
            if reference {
                e.written = Some((at, original.to_string()));
            }
            edits.push(e);
        };
        let root = scoping.root_scope_id();
        for &sym in scoping.get_bindings(root).values() {
            let original = scoping.symbol_name(sym);
            let name = match binds.get(&(m, sym)) {
                Some(b) => &names[b],
                None if module.imports.contains_key(&sym) => continue, // a namespace (rejected)
                None => {
                    let name = &names[&Binding::Sym(m, sym)];
                    if name == original {
                        continue;
                    }
                    ident(&mut edits, scoping.symbol_span(sym), original, name, false);
                    for r in scoping.symbol_redeclarations(sym) {
                        ident(&mut edits, r.span, original, name, false);
                    }
                    name
                }
            };
            for &r in scoping.get_resolved_reference_ids(sym) {
                let node = scoping.get_reference(r).node_id();
                if matches!(nodes.parent_kind(node), AstKind::ExportSpecifier(_)) {
                    continue; // rewritten or deleted with its statement
                }
                let import = binds.contains_key(&(m, sym));
                if name != original || import {
                    ident(&mut edits, nodes.kind(node).span(), original, name, true);
                    edits.last_mut().expect("just pushed").import = import;
                }
            }
        }
        for span in &module.meta {
            edits.push(Edit::new(span.start, span.end, names[&Binding::Meta(m)].clone()));
        }
        for &(g, k) in &module.global_modules {
            let (global, _, export) = node::GLOBALS[g];
            let i = self.modules[k].cjs_names.iter().position(|n| n == export).expect("the built-in exports it");
            let name = &names[&Binding::CjsNamed(k, i)];
            let refs = &module.global_refs.iter().find(|(h, _)| *h == g).expect("its references").1;
            for r in refs {
                if r.member {
                    edits.push(Edit::new(r.span.start, r.span.end, name.clone()));
                } else {
                    ident(&mut edits, r.span, global, name, true);
                }
            }
        }
        edits.sort_by_key(|e| (e.start, e.end));
        edits
    }
}

struct Edit {
    start: u32,
    end: u32,
    text: String,
    /// A reference: where its identifier starts in `text`, and the name written in the source.
    written: Option<(u32, String)>,
    /// The reference names an import (assigning to it throws).
    import: bool,
}

impl Edit {
    fn new(start: u32, end: u32, text: String) -> Self {
        Edit {
            start,
            end,
            text,
            written: None,
            import: false,
        }
    }
}

/// `s` as a JS string literal.
fn js_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32))
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn is_identifier_name(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c == '$' || c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '$' || c == '_' || c.is_alphanumeric())
}

/// `a/./b/../c.js` → `a/c.js` (lexically; `..` past the start stays).
pub(crate) fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." => {}
            ".." if parts.last().is_some_and(|p| *p != ".." && !p.is_empty()) => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    let mut out = parts.join("/");
    if out.is_empty() {
        out.push('.');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Files in memory.
    struct Mem(HashMap<String, String>);

    impl Host for Mem {
        fn read(&mut self, path: &str) -> std::io::Result<String> {
            self.0.get(path).cloned().ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        }
        fn list(&mut self, dir: &str) -> std::io::Result<Vec<(String, bool)>> {
            let prefix = if dir.is_empty() || dir.ends_with('/') { dir.to_string() } else { format!("{dir}/") };
            let mut out: Vec<(String, bool)> = Vec::new();
            for path in self.0.keys() {
                let Some(rest) = path.strip_prefix(&prefix) else { continue };
                let entry = match rest.split_once('/') {
                    Some((d, _)) => (d.to_string(), true),
                    None => (rest.to_string(), false),
                };
                if !out.contains(&entry) {
                    out.push(entry);
                }
            }
            out.sort();
            if out.is_empty() {
                return Err(std::io::Error::from(std::io::ErrorKind::NotFound));
            }
            Ok(out)
        }
    }

    fn join_with(files: &[(&str, &str)], include: &[&str]) -> Result<Linked, Diagnostics> {
        let alloc = Allocator::default();
        let mut host = Mem(files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect());
        let options = Options {
            command: false,
            include: include.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        };
        link(&alloc, files[0].0, files[0].1, &options, &mut host)
    }

    fn join(files: &[(&str, &str)]) -> Result<Linked, Diagnostics> {
        join_with(files, &[])
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize("a/./b/../c.js"), "a/c.js");
        assert_eq!(normalize("./x.js"), "x.js");
        assert_eq!(normalize("../x.js"), "../x.js");
        assert_eq!(normalize("/a/../b.js"), "/b.js");
    }

    #[test]
    fn one_file_is_unchanged() {
        let l = join(&[("m.js", "console.log(1)")]).unwrap();
        assert_eq!(l.source, "console.log(1)");
        assert!(l.map.is_none());
    }

    #[test]
    fn joins_in_evaluation_order_and_renames() {
        let l = join(&[
            ("d/main.js", "import { f as g, x } from './a.js';\nconst x2 = 1; let y = 2;\nconsole.log(g(), x, { x }, y);\n"),
            ("d/a.js", "let y = 3;\nexport const x = y;\nexport function f() { return y; }\n"),
        ])
        .unwrap();
        assert_eq!(
            l.source,
            "let y$1 = 3;\nconst x = y$1;\nfunction f() { return y$1; }\n\n;\n\nconst x2 = 1; let y = 2;\nconsole.log(f(), x, { x }, y);\n"
        );
        let map = l.map.unwrap();
        assert_eq!(map.names["y$1"], "y");
        let at = l.source.find("f(),").unwrap() as u32;
        assert_eq!(map.refs[&at], "g");
        assert_eq!(map.locate(at), ("d/main.js", 3, 13));
        assert_eq!(map.locate(l.source.find("return").unwrap() as u32), ("d/a.js", 3, 23));
    }

    #[test]
    fn shorthand_and_default() {
        let l = join(&[
            ("main.js", "import d, { v } from './a.js';\nfunction h(v) { return v; }\nconsole.log(d(), { v }, h);\n"),
            ("a.js", "export default function () { return 1; }\nexport let v = 1;\n"),
        ])
        .unwrap();
        // `v` is a parameter in main.js, so a.js's `v` gets a new name there.
        assert_eq!(
            l.source,
            "function default$1() { return 1; }\nlet v$1 = 1;\n\n;\n\nfunction h(v) { return v; }\nconsole.log(default$1(), { v: v$1 }, h);\n"
        );
    }

    #[test]
    fn missing_export_and_file() {
        let err = join(&[("main.js", "import { y } from './a.js';\n"), ("a.js", "export const x = 1;\n")])
            .err()
            .unwrap()
            .to_string();
        assert!(
            err.starts_with("main.js:1:10: error: SyntaxError: The requested module './a.js' does not provide an export named 'y'"),
            "{err}"
        );
        let err = join(&[("main.js", "import './b';\n")]).err().unwrap().to_string();
        assert!(err.starts_with("main.js:1:8: error: cannot find module `./b`"), "{err}");
        assert!(err.contains("help: node needs the file's extension: `./b.js`?"), "{err}");
    }

    #[test]
    fn async_dependency_makes_module_functions() {
        let l = join(&[
            ("main.js", "import './a.js';\nimport './b.js';\nconsole.log(1);\n"),
            ("a.js", "await 0;\n"),
            ("b.js", "import './a.js';\nasync function f() { await 1; }\n"),
        ])
        .unwrap();
        assert!(
            l.source.starts_with("$$EvaluateModules(__intrinsic_module_bodies(), [true, false, false], [[], [0], [0, 1]], [null, null, null], [], 2, true);\n")
                || l.source.starts_with("await $$EvaluateModules(__intrinsic_module_bodies(), [true, false, false], [[], [0], [0, 1]], [null, null, null], [], 2, false);\n"),
            "{}",
            l.source
        );
        let map = l.map.unwrap();
        let texts: Vec<&str> = map.modules.iter().map(|(a, b)| &l.source[*a as usize..*b as usize]).collect();
        assert_eq!(texts, ["await 0;\n", "\nasync function f() { await 1; }\n", "\n\nconsole.log(1);\n"]);
        // `await` only inside a function: no module functions.
        let l = join(&[("main.js", "import './b.js';\n"), ("b.js", "export async function f() { await 1; }\n")]).unwrap();
        assert!(l.map.unwrap().modules.is_empty());
    }

    #[test]
    fn import_meta_is_a_generated_binding() {
        let l = join(&[("main.js", "const meta$1 = 0;\nconsole.log(import.meta.url, meta$1);\n")]).unwrap();
        let first = l.source.lines().next().unwrap();
        assert!(first.starts_with("const meta$2 = $$ImportMeta(\"file:///"), "{first}");
        assert!(first.contains("/main.js\", \"/") && first.ends_with(", true);"), "{first}");
        assert!(l.source.ends_with("console.log(meta$2.url, meta$1);\n"), "{}", l.source);
        assert_eq!(file_url("/a b/é#?.js"), "file:///a%20b/%C3%A9%23%3F.js");
    }

    #[test]
    fn json_modules() {
        let l = join(&[
            ("main.js", "import d from './d.json' with { type: 'json' };\nconsole.log(d);\n"),
            ("d.json", "{\"__proto__\": [1, 2.5e-3, \"\\u0041\"], \"\\u005f_proto__\": null}"),
        ])
        .unwrap();
        assert!(
            l.source.starts_with("const default$1 = {[\"__proto__\"]: [1, 2.5e-3, \"\\u0041\"], [\"\\u005f_proto__\"]: null};"),
            "{}",
            l.source
        );
        for (text, at, what) in [
            ("{\"a\": 1,}", 8, "expected a property name in double quotes, found `}`"),
            ("[01]", 2, "expected `,` or `]`, found `1`"),
            ("'x'", 0, "expected a JSON value, found `'`"),
            ("\"a\tb\"", 2, "expected a character (control characters must be escaped), found `\t`"),
            ("1 2", 2, "expected the end of the text, found `2`"),
        ] {
            assert_eq!(json_module(text, "export default "), Err((at, what.to_string())), "{text}");
        }
        let err = |files: &[(&str, &str)]| join(files).err().unwrap().to_string();
        let e = err(&[("main.js", "import d from './d.json';\n"), ("d.json", "1")]);
        assert!(e.contains("needs an import attribute of \"type: json\""), "{e}");
        let e = err(&[("main.js", "import d from './a.js' with { type: 'json' };\n"), ("a.js", "")]);
        assert!(e.contains("is not of type \"json\""), "{e}");
        let e = err(&[("main.js", "import d from './d.json' with { type: 'css' };\n"), ("d.json", "1")]);
        assert!(e.contains("import attribute `type: \"css\"` isn't supported"), "{e}");
        let e = err(&[("main.js", "import d from './d.json' with { type: 'json' };\n"), ("d.json", "\n [1,]")]);
        assert!(e.starts_with("d.json:2:5: error: invalid JSON: expected a JSON value, found `]`"), "{e}");
    }

    #[test]
    fn dynamic_import_patterns() {
        let files = [
            ("app/main.js", "const k = './lib/a.js';\nimport(k);\nimport(`./loc/${x}.js`);\nimport(c ? './lib/b.js' : './missing.js');\nimport('./lib/' + y);\nimport('pkg');\n"),
            ("app/lib/a.js", "export const a = 1;\n"),
            ("app/lib/b.js", "import './c.js';\n"),
            ("app/lib/c.js", ""),
            ("app/lib/notes.txt", ""),
            ("app/loc/en.js", ""),
            ("app/loc/fr.js", ""),
            ("app/loc/sub/de.js", ""),
        ];
        let l = join(&files).unwrap();
        let first = l.source.lines().find(|l| l.contains("$$EvaluateModules")).unwrap();
        // The table: every module a pattern names (not `notes.txt`, not the subdirectory), by
        // absolute path; `./missing.js` and `pkg` reject at run time.
        let keys: Vec<&str> = first.split('"').filter(|k| k.starts_with('/')).collect();
        let tail = |k: &&str| k.split("/app/").nth(1).unwrap_or(k).to_string();
        let mut keys: Vec<String> = keys.iter().map(tail).collect();
        keys.sort();
        // (`""` and `main.js`: the importer's directory and file; `pkg`: not found, rejects.)
        assert_eq!(keys, ["", "\\u000apkg", "lib/a.js", "lib/b.js", "lib/c.js", "loc/en.js", "loc/fr.js", "main.js"], "{first}");
        assert!(first.contains("\"Cannot find package 'pkg' imported from "), "{first}");
        assert!(l.source.contains("\n$$DynamicImport(0, k);\n$$DynamicImport(0, `./loc/${x}.js`);\n"), "{}", l.source);
        // Unknown specifiers need `--include`.
        let err = join(&[("m.js", "import(f());\n")]).err().unwrap().to_string();
        assert!(err.starts_with("m.js:1:1: error: `import()` of a module the build can't work out"), "{err}");
        assert!(err.contains("--include <glob>"), "{err}");
        let l = join_with(&[("m.js", "import(f());\n"), ("p/a.js", ""), ("p/q/b.json", "1"), ("p/q/r/c.js", "")], &["p/**/*.js"]).unwrap();
        assert!(l.source.contains("/p/a.js\""), "{}", l.source);
        assert!(l.source.contains("/p/q/r/c.js\""), "{}", l.source);
        assert!(!l.source.contains("b.json"), "{}", l.source);
        let err = join_with(&[("m.js", "import(f());\n")], &["nothing/*.js"]).err().unwrap().to_string();
        assert!(err.contains("the glob matches no module"), "{err}");
    }

    #[test]
    fn glob_segments() {
        let w = Part::Wild;
        let lit = |s: &str| Part::Lit(s.into());
        assert!(glob_match(&[lit("a"), w.clone(), lit(".js")], "ab.c.js"));
        assert!(!glob_match(&[lit("a"), w.clone(), lit(".js")], "b.js"));
        assert!(glob_match(&[w.clone()], ""));
        assert_eq!(merge(vec![lit("./a"), lit("/"), w.clone(), w, lit(".js")]), vec![lit("./a/"), Part::Wild, lit(".js")]);
    }

    #[test]
    fn packages() {
        let files = [
            ("/w/app/main.js", "import a from 'p';\nimport b from 'p/sub/x';\nimport c from '@s/q';\nimport d from '#here';\nconsole.log(a, b, c, d);\n"),
            ("/w/app/package.json", r##"{"name": "app", "imports": {"#here": "./here.js"}}"##),
            ("/w/app/here.js", "export default 4;\n"),
            ("/w/node_modules/p/package.json", r#"{"name": "p", "exports": {".": {"import": "./i.js", "default": "./d.js"}, "./sub/*": "./s/*.js"}}"#),
            ("/w/node_modules/p/i.js", "export default 1;\n"),
            ("/w/node_modules/p/s/x.js", "export default 2;\n"),
            ("/w/app/node_modules/@s/q/package.json", r#"{"name": "@s/q", "main": "lib"}"#),
            ("/w/app/node_modules/@s/q/lib/index.js", "export default 3;\n"),
        ];
        let l = join(&files).unwrap();
        for want in ["/w/node_modules/p/i.js", "/w/node_modules/p/s/x.js", "/w/app/node_modules/@s/q/lib/index.js", "/w/app/here.js"] {
            let map = l.map.as_ref().unwrap();
            assert!(map.files.iter().any(|(p, _)| p == want), "{want} not loaded");
        }
        let err = |main: &str, extra: &[(&str, &str)]| {
            let mut files = vec![("/w/app/main.js", main)];
            files.extend_from_slice(&files_of(extra));
            join(&files).err().unwrap().to_string()
        };
        fn files_of<'a>(f: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
            f.to_vec()
        }
        let pj = ("/w/node_modules/p/package.json", r#"{"exports": {"./a": "./a.js"}}"#);
        let e = err("import 'p/b';\n", &[pj]);
        assert!(e.contains("Package subpath './b' is not defined by \"exports\" in /w/node_modules/p/package.json imported from /w/app/main.js (ERR_PACKAGE_PATH_NOT_EXPORTED)"), "{e}");
        let e = err("import 'nope';\nimport 'worker_threads';\n", &[]);
        assert!(e.contains("main.js:1:8: error: Cannot find package 'nope' imported from /w/app/main.js (ERR_MODULE_NOT_FOUND)"), "{e}");
        assert!(e.contains("main.js:2:8: error: unsupported: node's built-in module `worker_threads`"), "{e}");
        // A package without `"type"` whose file has no ESM syntax is CommonJS (T54).
        let l = join(&[
            ("/w/app/main.js", "import c from 'c';\nconsole.log(c);\n"),
            ("/w/node_modules/c/package.json", "{}"),
            ("/w/node_modules/c/index.js", "module.exports = 1;\n"),
        ])
        .unwrap();
        assert!(l.source.contains("const exports$1 = $$cjsImport(modules$1, 0);"), "{}", l.source);
        assert!(l.source.contains("console.log(exports$1);"), "{}", l.source);
        // `"type": "module"`: the same file is ESM (and `module` is an undeclared global).
        let ok = join(&[
            ("/w/app/main.js", "import 'c';\n"),
            ("/w/node_modules/c/package.json", r#"{"type": "module"}"#),
            ("/w/node_modules/c/index.js", "module.exports = 1;\n"),
        ]);
        assert!(ok.is_ok());
    }

    #[test]
    fn node_builtins() {
        // T55: `x` and `node:x` are one CommonJS module at `node:x`, whose named exports come from
        // `node.rs`'s list (no `'module.exports'`); its code may use the prelude.
        let l = join(&[(
            "/w/main.js",
            "import p, { join } from 'node:path';\nimport q from 'path';\nimport { env } from 'process';\nconsole.log(p === q, join, env);\n",
        )])
        .unwrap();
        assert_eq!(l.source.matches("function cjs$").count(), 2, "{}", l.source);
        assert!(l.source.contains("\"path\", \".\""), "{}", l.source);
        assert!(!l.source.contains("'module.exports'") && !l.source.contains("\"module.exports\""), "{}", l.source);
        let map = l.map.unwrap();
        let at = l.source.find("$$errInvalidArgType").unwrap() as u32;
        assert!(map.is_node_module(at) && !map.is_node_module(l.source.find("console.log(").unwrap() as u32));
        // `require` of one js2wasi3 lacks: a build error where it always runs, else node's error.
        let e = join(&[("/w/main.js", "import 'worker_threads';\n")]).err().unwrap().to_string();
        assert!(e.contains("unsupported: node's built-in module `worker_threads` (planned: F238)"), "{e}");
        let l = join(&[("/w/main.cjs", "try { require('node:nope'); } catch {}\n")]).unwrap();
        assert!(l.source.contains("No such built-in module: node:nope"), "{}", l.source);
    }

    #[test]
    fn buffer_global() {
        // T56: an ES module's `Buffer` names `node:buffer`'s export; a CommonJS module requires it
        // first; a program without it doesn't load the module.
        let l = join(&[("/w/main.js", "const b = { Buffer };\nconsole.log(Buffer.from('x'), typeof Buffer);\n")]).unwrap();
        assert!(l.source.contains("{ Buffer: Buffer$1 }") && l.source.contains("console.log(Buffer$1.from('x'), typeof Buffer$1)"), "{}", l.source);
        let l = join(&[("/w/main.cjs", "console.log(Buffer.alloc(1));\n")]).unwrap();
        assert!(l.source.contains("{const Buffer = require(\"node:buffer\").Buffer;console.log(Buffer.alloc(1));"), "{}", l.source);
        assert!(join(&[("/w/main.js", "let Buffer = 1;\nconsole.log(Buffer);\n")]).unwrap().map.is_none());
    }

    #[test]
    fn commonjs_detection() {
        // An entry node runs as CommonJS but that can't tell stays an ES module (same bytes).
        let l = join(&[("/w/main.js", "console.log(1);\n")]).unwrap();
        assert!(l.map.is_none());
        // Its CommonJS names make it CommonJS: a function run as the main module.
        let l = join(&[("/w/main.js", "console.log(typeof require);\n")]).unwrap();
        assert!(l.source.contains("function cjs$1(exports, require, module, __filename, __dirname) {console.log"), "{}", l.source);
        assert!(l.source.contains("$$cjsMain(modules$1, 0);"), "{}", l.source);
        // `return` at the top level parses only as CommonJS.
        assert!(join(&[("/w/main.js", "return;\n")]).unwrap().source.contains("cjs$1"));
        // ESM syntax, or `"type": "module"`: an ES module (`module` is an undeclared global).
        assert!(join(&[("/w/main.js", "export {};\nmodule.x = 1;\n")]).unwrap().map.is_none());
        let l = join(&[("/w/main.js", "module.x = 1;\n"), ("/w/package.json", r#"{"type": "module"}"#)]).unwrap();
        assert!(l.map.is_none());
        // `.cjs` and `"type": "commonjs"` are CommonJS; ESM syntax there doesn't parse.
        let e = join(&[("/w/main.cjs", "import x from './y.js';\n")]).err().unwrap().to_string();
        assert!(e.contains("main.cjs:1:1: error:"), "{e}");
        let e = join(&[("/w/main.js", "with (a) {}\n")]).err().unwrap().to_string();
        assert!(e.contains("main.js:1:1: error:") && e.contains("(planned: F232)"), "{e}");
    }

    #[test]
    fn commonjs_requires() {
        let l = join(&[
            ("/w/main.js", "const a = require('./a'), b = require(`./b.json`), c = require('./c');\nrequire.resolve('./d');\nrequire(x);\n"),
            ("/w/a.js", "#!/usr/bin/env node\nexports.a = require('./a.js');\n"),
            ("/w/b.json", r#"{"__proto__": 1}"#),
            ("/w/d/index.js", ""),
        ])
        .unwrap();
        let s = &l.source;
        assert!(s.contains("[[\"./a\", 1], [\"./b.json\", 2], [\"./c\", -1], [\"./d\", 3]]"), "{s}");
        assert!(s.contains("function cjs$2(exports, require, module, __filename, __dirname) {//"), "{s}");
        assert!(s.contains("{module.exports = {[\"__proto__\"]: 1};"), "{s}");
        let map = l.map.unwrap();
        // Offsets in a wrapper map back to the file.
        let at = s.find("exports.a").unwrap() as u32;
        assert_eq!(map.locate(at), ("/w/a.js", 2, 1));
        let e = join(&[("/w/main.js", "require('worker_threads');\n")]).err().unwrap().to_string();
        assert!(e.contains("main.js:1:9: error: unsupported: node's built-in module `worker_threads`"), "{e}");
        // T60: only node's exact built-in names: `process/` is a package (readable-stream's).
        let l = join(&[("/w/main.js", "require('process/');\n")]).unwrap();
        assert!(l.source.contains(r#"["process/", -1]"#), "{}", l.source);
        // Packages: the `require` conditions, LOAD_AS_FILE/DIRECTORY without `exports`; errors
        // other than not found are thrown with node's code.
        let l = join(&[
            ("/w/app/main.js", "require('p'); require('p/x'); require('q'); require('q/sub'); require('r'); require('nope');\n"),
            ("/w/node_modules/p/package.json", r#"{"exports": {".": {"import": "./i.js", "require": "./r.js"}}}"#),
            ("/w/node_modules/p/r.js", ""),
            ("/w/node_modules/q/package.json", r#"{"main": "lib/main"}"#),
            ("/w/node_modules/q/lib/main.js", ""),
            ("/w/node_modules/q/sub.json", "{}"),
            ("/w/app/node_modules/r.js", ""),
        ])
        .unwrap();
        let s = &l.source;
        assert!(s.contains(r#"[["p", 1], ["p/x", -1, "ERR_PACKAGE_PATH_NOT_EXPORTED", "Package subpath './x' is not defined by \"exports\" in /w/node_modules/p/package.json"], ["q", 2], ["q/sub", 3], ["r", 4], ["nope", -1]]"#), "{s}");
        assert!(s.contains("\"/w/node_modules/q/sub.json\""), "{s}");
        assert!(s.contains("\"/w/app/node_modules/r.js\""), "{s}");
    }

    #[test]
    fn commonjs_export_names() {
        let names = |src: &str| {
            let alloc = Allocator::default();
            let p = parse_cjs(&alloc, "/w/x.js", alloc.alloc_str(src)).unwrap();
            cjs_exports(&p)
        };
        // cjs-module-lexer's rules (checked against node 24).
        let (n, r) = names("const a = 1, c = 2, f = 3;\nmodule.exports = { a, b: c, 'd': a, e: 1, f };\n");
        assert_eq!((n, r), (vec!["a".into(), "b".into(), "d".into()], vec![]));
        let (n, _) = names("const x = {};\nmodule.exports = { a: x.y, b: x };\nif (false) exports.dead = 1;\nexports['d-e'] = 2;\n");
        assert_eq!(n, ["a", "d-e", "dead"]);
        let (n, _) = names("module.exports = { get a() { return 1; }, b: 2 };\nmodule.exports = { m() {}, k: 1 };\n");
        assert_eq!(n, ["m"]);
        let (n, r) = names("Object.defineProperty(exports, '__esModule', { value: true });\nObject.defineProperty(exports, 'g', { enumerable: true, get: function () { return m.g; } });\nObject.defineProperty(exports, 'h', { get() { return 1; } });\n__exportStar(require('./s'), exports);\nmodule.exports = { ...require('./t'), ...rest };\n");
        assert_eq!((n, r), (vec!["__esModule".into(), "g".into()], vec!["./s".into(), "./t".into()]));
        let (n, r) = names("exports.default = 1;\nmodule.exports = require('./u');\n");
        assert_eq!((n, r), (vec![], vec!["./u".into()]));
    }
}
