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

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;

use crate::{Allocator, Diagnostic, Diagnostics, Program, line_col, parse};

/// Where each part of a joined source came from.
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    /// Each module's path and source.
    files: Vec<(String, String)>,
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
    /// The file, 1-based line and column of a joined offset.
    pub fn locate(&self, offset: u32) -> (&str, u32, u32) {
        let k = self.segments.partition_point(|s| s.at <= offset).saturating_sub(1);
        let s = self.segments[k];
        let at = if s.copied { s.offset + (offset - s.at) } else { s.offset };
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

/// Load the modules the entry imports (through `read`) and join them. `command`: the program is
/// a command even if the entry has exports (as node runs it): they're dropped like a dependency's.
pub fn link(
    alloc: &Allocator,
    entry: &str,
    source: &str,
    command: bool,
    read: &mut dyn FnMut(&str) -> std::io::Result<String>,
) -> Result<Linked, Diagnostics> {
    let mut g = Graph {
        alloc,
        read,
        command,
        modules: Vec::new(),
        index: HashMap::new(),
        diags: Vec::new(),
    };
    g.load(entry.to_string(), source)?;
    if !g.diags.is_empty() {
        return Err(sorted(g.diags));
    }
    if g.modules.iter().all(|m| m.requests.is_empty() && m.meta.is_empty())
        && !(command && !g.modules[0].p.exports.is_empty())
    {
        return Ok(Linked {
            source: source.to_string(),
            map: None,
        });
    }
    g.join()
}

/// A JSON module's source (slice 3): `export default <the JSON>;` (JSON is JS syntax), with each
/// `"__proto__"` key made computed so it stays an own property, as `JSON.parse` makes it. Or where
/// the text isn't JSON, and why.
fn json_module(text: &str) -> Result<String, (usize, String)> {
    struct P<'s> {
        s: &'s [u8],
        at: usize,
        proto: Vec<(usize, usize)>,
    }
    impl P<'_> {
        fn ws(&mut self) {
            while self.at < self.s.len() && matches!(self.s[self.at], b' ' | b'\t' | b'\n' | b'\r') {
                self.at += 1;
            }
        }
        fn fail<T>(&self, what: &str) -> Result<T, (usize, String)> {
            let found = match self.s.get(self.at) {
                None => "the end of the text".to_string(),
                Some(_) => {
                    let c = std::str::from_utf8(&self.s[self.at..]).ok().and_then(|t| t.chars().next());
                    format!("`{}`", c.unwrap_or('?'))
                }
            };
            Err((self.at, format!("expected {what}, found {found}")))
        }
        fn eat(&mut self, c: u8) -> bool {
            let ok = self.s.get(self.at) == Some(&c);
            self.at += ok as usize;
            ok
        }
        fn digits(&mut self) -> usize {
            let start = self.at;
            while self.s.get(self.at).is_some_and(u8::is_ascii_digit) {
                self.at += 1;
            }
            self.at - start
        }
        /// A string; its value when it's a key.
        fn string(&mut self) -> Result<String, (usize, String)> {
            let mut value = Vec::new();
            self.at += 1;
            loop {
                match self.s.get(self.at) {
                    None => return self.fail("`\"`"),
                    Some(b'"') => break,
                    Some(c) if *c < 0x20 => return self.fail("a character (control characters must be escaped)"),
                    Some(b'\\') => {
                        self.at += 1;
                        match self.s.get(self.at) {
                            Some(b'u') => {
                                let hex = self.s.get(self.at + 1..self.at + 5).and_then(|h| std::str::from_utf8(h).ok());
                                let Some(u) = hex.and_then(|h| u16::from_str_radix(h, 16).ok()).filter(|_| hex.unwrap().bytes().all(|b| b.is_ascii_hexdigit())) else {
                                    self.at += 1;
                                    return self.fail("four hex digits");
                                };
                                value.push(u);
                                self.at += 5;
                            }
                            Some(c) if b"\"\\/bfnrt".contains(c) => {
                                value.push(*c as u16);
                                self.at += 1;
                            }
                            _ => return self.fail("an escape (`\\\"`, `\\n`, `\\u0041`, …)"),
                        }
                    }
                    Some(_) => {
                        let rest = std::str::from_utf8(&self.s[self.at..]).unwrap_or("");
                        let c = rest.chars().next().unwrap_or('\u{fffd}');
                        let mut buf = [0u16; 2];
                        value.extend_from_slice(c.encode_utf16(&mut buf));
                        self.at += c.len_utf8().max(1);
                    }
                }
            }
            self.at += 1;
            Ok(String::from_utf16_lossy(&value))
        }
        fn value(&mut self) -> Result<(), (usize, String)> {
            self.ws();
            match self.s.get(self.at) {
                Some(b'{') => {
                    self.at += 1;
                    self.ws();
                    if self.eat(b'}') {
                        return Ok(());
                    }
                    loop {
                        self.ws();
                        if self.s.get(self.at) != Some(&b'"') {
                            return self.fail("a property name in double quotes");
                        }
                        let start = self.at;
                        if self.string()? == "__proto__" {
                            self.proto.push((start, self.at));
                        }
                        self.ws();
                        if !self.eat(b':') {
                            return self.fail("`:`");
                        }
                        self.value()?;
                        self.ws();
                        if self.eat(b'}') {
                            return Ok(());
                        }
                        if !self.eat(b',') {
                            return self.fail("`,` or `}`");
                        }
                    }
                }
                Some(b'[') => {
                    self.at += 1;
                    self.ws();
                    if self.eat(b']') {
                        return Ok(());
                    }
                    loop {
                        self.value()?;
                        self.ws();
                        if self.eat(b']') {
                            return Ok(());
                        }
                        if !self.eat(b',') {
                            return self.fail("`,` or `]`");
                        }
                    }
                }
                Some(b'"') => self.string().map(|_| ()),
                Some(b'-' | b'0'..=b'9') => {
                    self.eat(b'-');
                    if !self.eat(b'0') && self.digits() == 0 {
                        return self.fail("a digit");
                    }
                    if self.eat(b'.') && self.digits() == 0 {
                        return self.fail("a digit");
                    }
                    if self.eat(b'e') || self.eat(b'E') {
                        let _ = self.eat(b'+') || self.eat(b'-');
                        if self.digits() == 0 {
                            return self.fail("a digit");
                        }
                    }
                    Ok(())
                }
                _ => {
                    for word in ["true", "false", "null"] {
                        if self.s[self.at..].starts_with(word.as_bytes()) {
                            self.at += word.len();
                            return Ok(());
                        }
                    }
                    self.fail("a JSON value")
                }
            }
        }
    }
    let mut p = P { s: text.as_bytes(), at: 0, proto: Vec::new() };
    p.value()?;
    p.ws();
    if p.at < text.len() {
        return p.fail("the end of the text");
    }
    let mut out = String::from("export default ");
    let mut pos = 0;
    for (start, end) in p.proto {
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
}

struct Graph<'a, 'r> {
    alloc: &'a Allocator,
    read: &'r mut dyn FnMut(&str) -> std::io::Result<String>,
    command: bool,
    modules: Vec<Module<'a>>,
    index: HashMap<String, usize>,
    diags: Vec<Diagnostic>,
}

/// A specifier resolved against the importing file.
enum Request {
    File(String),
    /// A WIT interface (`ns:pkg/iface`): an import of the user world (T20), not a module.
    Wit,
}

impl<'a> Graph<'a, '_> {
    fn diag(&mut self, m: usize, span: Span, message: String, help: Option<String>) {
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

    fn load(&mut self, path: String, source: &str) -> Result<usize, Diagnostics> {
        let source: &'a str = self.alloc.alloc_str(source);
        let p = parse(self.alloc, &path, source)?;
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
        });
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
        self.scan(m);
        Ok(m)
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
        match (self.read)(&path) {
            Ok(text) if json => {
                let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                match json_module(text) {
                    Ok(source) => self.load(path, &source).map(Some),
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
            Ok(text) => self.load(path, &text).map(Some),
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
        if let Some(name) = spec.strip_prefix("node:") {
            self.diag(
                m,
                span,
                format!("unsupported: node's built-in module `node:{name}` (planned: not scheduled)"),
                None,
            );
            return None;
        }
        if spec.contains(':') {
            return Some(Request::Wit);
        }
        self.diag(
            m,
            span,
            format!("unsupported: package import `{spec}` (a bare specifier: node_modules) (planned: T41 slice 5)"),
            Some("import files by relative path, e.g. `./lib.js`".into()),
        );
        None
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

        let namespaces = self.namespaces(&binds);
        let names = self.names(&binds, &namespaces);
        let order = self.order();
        let mut map = SourceMap::default();
        let mut out = String::new();
        // Each module's `import.meta` (slice 3): node's properties, from the file's absolute path.
        for m in 0..self.modules.len() {
            if let Some(name) = names.get(&Binding::Meta(m)) {
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
        // A dependency with top-level `await` (slice 3, commands only: a library's `init` can't
        // wait): every module becomes a function, run by the spec's algorithm.
        let evaluate = (self.command || self.modules[0].p.exports.is_empty())
            && self.modules[1..].iter().any(|m| m.tla);
        if evaluate {
            let at: HashMap<usize, usize> = order.iter().enumerate().map(|(k, m)| (*m, k)).collect();
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
            out.push_str(&format!(
                "await $$EvaluateModules(__intrinsic_module_bodies(), [{}], [{}]);\n",
                tla.join(", "),
                requests.join(", ")
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
            let edits = self.edits(m, &binds, &names, &entry_exports);
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
            if evaluate {
                map.modules.push((start, out.len() as u32));
            }
        }
        for (b, name) in &names {
            let original = match *b {
                Binding::Sym(m, s) => self.modules[m].p.semantic.scoping().symbol_name(s),
                Binding::Default(_) => "default",
                Binding::Namespace(_) | Binding::Meta(_) => continue,
            };
            if original != name {
                map.names.insert(name.clone(), original.to_string());
            }
        }
        map.files = self
            .modules
            .iter()
            .map(|m| (m.p.path.clone(), m.p.source.to_string()))
            .collect();
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
            out.push(m);
        }
        let mut seen = vec![false; self.modules.len()];
        let mut out = Vec::new();
        visit(self, 0, &mut seen, &mut out);
        out
    }

    /// A unique name for each module's top-level bindings (the entry's first).
    /// The namespace objects the program needs (`import * as ns`, `export * as ns`, and those
    /// they export): each module's exports, by name in code-unit order, and their bindings.
    fn namespaces(&self, binds: &HashMap<(usize, SymbolId), Binding>) -> Vec<(usize, Vec<(String, Binding)>)> {
        let mut todo: Vec<usize> = binds
            .values()
            .filter_map(|b| match b {
                Binding::Namespace(t) => Some(*t),
                _ => None,
            })
            .collect();
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
        for m in 0..self.modules.len() {
            if self.modules[m].meta.is_empty() {
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
fn normalize(path: &str) -> String {
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

    fn join(files: &[(&str, &str)]) -> Result<Linked, Diagnostics> {
        let alloc = Allocator::default();
        let map: HashMap<String, String> =
            files.iter().map(|(p, s)| (p.to_string(), s.to_string())).collect();
        let mut read = |p: &str| {
            map.get(p)
                .cloned()
                .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
        };
        link(&alloc, files[0].0, files[0].1, false, &mut read)
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
            l.source.starts_with("await $$EvaluateModules(__intrinsic_module_bodies(), [true, false, false], [[], [0], [0, 1]]);\n"),
            "{}",
            l.source
        );
        let map = l.map.unwrap();
        let texts: Vec<&str> = map.modules.iter().map(|(a, b)| &l.source[*a as usize..*b as usize]).collect();
        assert_eq!(texts, ["await 0;\n", "\nasync function f() { await 1; }\n", "\n\nconsole.log(1);\n"]);
        // `await` only inside a function: no module functions.
        let l = join(&[("main.js", "import './b.js';\n"), ("b.js", "async function f() { await 1; }\n")]).unwrap();
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
            assert_eq!(json_module(text), Err((at, what.to_string())), "{text}");
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
}
