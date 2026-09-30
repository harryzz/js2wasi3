//! Frontend: parse an ES module (oxc) and run semantic analysis.
//! Owns source text, spans and diagnostics. Knows nothing about Wasm.
//!
//! # Lifetimes
//! oxc allocates the AST in an arena. The caller owns the [`Allocator`] and the source
//! text, and [`Program<'a>`] borrows both:
//!
//! ```ignore
//! let alloc = Allocator::default();
//! let program = frontend::parse(&alloc, "add.js", &source)?;
//! ```
//!
//! This keeps the frontend free of self-referential structs and `unsafe`. The cost is
//! that every later stage runs inside the allocator's scope, which fits a one-shot compiler.
//!
//! # Positions
//! Diagnostics report 1-based `line:col`. Columns count Unicode scalar values (chars),
//! not bytes or UTF-16 units.

use std::fmt;

mod eval;
mod link;
mod packages;
mod using;
pub use eval::EvalMap;
pub use link::{Fs, Host, Linked, Options, SourceMap, link};

use oxc_ast::ast;
use oxc_parser::Parser;
use oxc_semantic::{Semantic, SemanticBuilder};
use oxc_span::{GetSpan, SourceType, Span};
use oxc_syntax::module_record::{ExportExportName, ExportLocalName, ModuleRecord};

pub use oxc_allocator::Allocator;

/// A parsed, semantically checked ES module.
pub struct Program<'a> {
    pub path: String,
    pub source: &'a str,
    /// Where the user's source ends in `source`: past it comes the built-ins prelude
    /// ([`parse_with_prelude`]), if any.
    pub user_end: u32,
    pub ast: &'a ast::Program<'a>,
    pub semantic: Semantic<'a>,
    pub exports: Vec<Export>,
    /// A program joined from several modules ([`link`]): where its parts came from.
    pub map: Option<SourceMap>,
    /// A module whose direct `eval` calls were spliced (T42): its file's text, and the offsets
    /// back to it.
    pub original: &'a str,
    pub evals: Option<EvalMap>,
}

/// One entry of the module's export list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
    /// Exported name. `"default"` for default exports, `"*"` for `export * from`.
    pub name: String,
    /// Local binding, when the export refers to one in this module.
    pub local: Option<String>,
    /// Module specifier for re-exports (`export { x } from "./y.js"`).
    pub from: Option<String>,
    /// The exported name was written as a string literal (`export { f as "my-f" }`).
    pub string_name: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: String,
    pub line: u32,
    pub col: u32,
    pub message: String,
    pub help: Option<String>,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}:{}: error: {}",
            self.path, self.line, self.col, self.message
        )?;
        if let Some(help) = &self.help {
            write!(f, "\n  help: {help}")?;
        }
        Ok(())
    }
}

/// All errors from one parse, in source order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostics(pub Vec<Diagnostic>);

impl fmt::Display for Diagnostics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, d) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{d}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Diagnostics {}

/// Parse `source` as an ES module and run semantic analysis with early-error checks.
pub fn parse<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
) -> Result<Program<'a>, Diagnostics> {
    // Regex literals are checked too (T27): an invalid pattern is an early SyntaxError.
    let options = oxc_parser::ParseOptions {
        parse_regular_expression: true,
        ..Default::default()
    };
    let ret = Parser::new(alloc, source, SourceType::mjs())
        .with_options(options)
        .parse();
    if ret.fatal_error || ret.diagnostics.has_errors() {
        return Err(convert(path, source, ret.diagnostics.into_vec()));
    }
    let exports = collect_exports(&ret.module_record, source);

    let ast = alloc.alloc(ret.program);
    // AST nodes: the lowering maps references to source positions (TDZ analysis, T12).
    let sem = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .with_build_nodes(true)
        .build(ast);
    if sem.diagnostics.has_errors() {
        return Err(convert(path, source, sem.diagnostics.into_vec()));
    }

    Ok(Program {
        path: path.to_string(),
        source,
        user_end: source.len() as u32,
        ast,
        semantic: sem.semantic,
        exports,
        map: None,
        original: source,
        evals: None,
    })
}

/// Parse `source`, then parse it again followed by `prelude` (JS source of built-ins, T21) as
/// one module. Errors in `source` are reported as if it had been parsed alone; errors in the
/// prelude (a conflict with a user name) point into the prelude, with the path `<builtins>`.
pub fn parse_with_prelude<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
    prelude: &str,
) -> Result<Program<'a>, Diagnostics> {
    parse_linked(alloc, path, source, None, prelude)
}

/// [`parse_with_prelude`] of a program [`link`] joined (`map`: where its parts came from).
pub fn parse_linked<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
    map: Option<SourceMap>,
    prelude: &str,
) -> Result<Program<'a>, Diagnostics> {
    let remap = |mut d: Diagnostics| {
        if let Some(map) = &map {
            for d in &mut d.0 {
                if d.path == path {
                    let offset = offset_of(source, d.line, d.col);
                    let (file, line, col) = map.locate(offset);
                    (d.path, d.line, d.col) = (file.to_string(), line, col);
                }
            }
        }
        d
    };
    parse(alloc, path, source).map_err(remap)?;
    let user_end = source.len() as u32;
    let mut all = String::with_capacity(source.len() + 1 + prelude.len());
    all.push_str(source);
    // A trailing line comment in the user's source must not swallow the prelude.
    all.push('\n');
    all.push_str(prelude);
    let all: &'a str = alloc.alloc_str(&all);
    let mut program = parse(alloc, path, all).map_err(|mut d| {
        for d in &mut d.0 {
            let (line, _) = line_col(all, user_end + 1);
            if d.line >= line {
                d.path = "<builtins>".to_string();
                d.line -= line - 1;
                d.help = Some(format!(
                    "{path}: a name starting with `$$` clashes with a built-in: rename it"
                ));
            }
        }
        remap(d)
    })?;
    program.user_end = user_end;
    program.map = map;
    Ok(program)
}

/// The byte offset of a 1-based line and column (in chars).
fn offset_of(source: &str, line: u32, col: u32) -> u32 {
    let start: usize = source.split_inclusive('\n').take(line as usize - 1).map(str::len).sum();
    let within: usize = source[start..].chars().take(col as usize - 1).map(char::len_utf8).sum();
    (start + within) as u32
}

impl<'a> Program<'a> {
    /// Is this byte offset in the built-ins prelude (not the user's source)?
    pub fn in_prelude(&self, offset: u32) -> bool {
        offset > self.user_end
    }

    /// The text inside the JSDoc comment (`/** ... */`, without the delimiters) that directly
    /// precedes the token starting at `offset`, if any.
    pub fn jsdoc(&self, offset: u32) -> Option<&'a str> {
        self.ast
            .comments
            .iter()
            .rev()
            .find(|c| c.is_leading() && c.is_jsdoc() && c.attached_to == offset)
            .map(|c| c.content_span().source_text(self.source))
    }

    /// 1-based line and column of a byte offset in this program's source.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        match &self.evals {
            Some(e) => line_col(self.original, e.original(offset)),
            None => line_col(self.source, offset),
        }
    }

    /// The file, 1-based line and column of a byte offset, for diagnostics: `<builtins>` in the
    /// prelude, the module's file in a joined program.
    pub fn locate(&self, offset: u32) -> (String, u32, u32) {
        if self.in_prelude(offset) {
            let (line, col) = line_col(self.source, offset);
            let first = line_col(self.source, self.user_end + 1).0;
            return ("<builtins>".to_string(), line - (first - 1), col);
        }
        match &self.map {
            Some(map) => {
                let (path, line, col) = map.locate(offset);
                (path.to_string(), line, col)
            }
            None => {
                let (line, col) = line_col(self.source, offset);
                (self.path.clone(), line, col)
            }
        }
    }

    /// The name a binding has in its module's source: renamed ones differ in a joined program.
    pub fn original_name<'n>(&'n self, name: &'n str) -> &'n str {
        self.map
            .as_ref()
            .and_then(|m| m.names.get(name))
            .map_or(name, String::as_str)
    }

    /// Is this offset in code the module linker wrote (namespace objects, T41), which may use
    /// the prelude?
    pub fn is_generated(&self, offset: u32) -> bool {
        !self.in_prelude(offset) && self.map.as_ref().is_some_and(|m| offset < m.generated || m.is_inserted(offset))
    }

    /// Does the reference at this offset name an import (an immutable binding, T41)?
    pub fn is_import(&self, offset: u32) -> bool {
        self.map.as_ref().is_some_and(|m| m.imports.contains(&offset))
    }

    /// The name written at a reference replaced by [`link`] (an import's local name), if any.
    pub fn written_name(&self, offset: u32) -> Option<&str> {
        self.map.as_ref()?.refs.get(&offset).map(String::as_str)
    }

    /// Top-level statements as `(line, col, kind)`, e.g. `(1, 1, "ExportNamedDeclaration")`.
    pub fn top_level(&self) -> Vec<(u32, u32, String)> {
        self.ast
            .body
            .iter()
            .map(|stmt| {
                let (line, col) = self.line_col(stmt.span().start);
                (line, col, statement_kind(stmt))
            })
            .collect()
    }
}

/// 1-based line and column (in chars) of a byte offset.
pub fn line_col(source: &str, offset: u32) -> (u32, u32) {
    let offset = (offset as usize).min(source.len());
    let before = &source[..offset];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let line = before.matches('\n').count() + 1;
    let col = before[line_start..].chars().count() + 1;
    (line as u32, col as u32)
}

/// Variant name of a statement, e.g. `"WhileStatement"`.
// The variant name from `Debug` is the only kind name that doesn't need
// updating whenever oxc adds a statement kind. Used for dumps, not hot paths.
pub fn statement_kind(stmt: &ast::Statement) -> String {
    let dbg = format!("{stmt:?}");
    dbg.split(['(', ' ', '{'])
        .next()
        .unwrap_or("Statement")
        .to_string()
}

fn collect_exports(record: &ModuleRecord, source: &str) -> Vec<Export> {
    let entries = record
        .local_export_entries
        .iter()
        .chain(record.indirect_export_entries.iter())
        .chain(record.star_export_entries.iter());
    let mut exports: Vec<Export> = entries
        .map(|e| Export {
            name: match &e.export_name {
                ExportExportName::Name(n) => n.name.to_string(),
                ExportExportName::Default(_) => "default".to_string(),
                ExportExportName::Null => "*".to_string(),
            },
            local: match &e.local_name {
                ExportLocalName::Name(n) | ExportLocalName::Default(n) => Some(n.name.to_string()),
                ExportLocalName::Null => None,
            },
            from: e.module_request.as_ref().map(|m| m.name.to_string()),
            string_name: match &e.export_name {
                ExportExportName::Name(n) => {
                    matches!(
                        source.as_bytes().get(n.span.start as usize),
                        Some(b'"' | b'\'')
                    )
                }
                _ => false,
            },
            span: e.span,
        })
        .collect();
    exports.sort_by_key(|e| e.span.start);
    exports
}

fn convert(path: &str, source: &str, diags: Vec<oxc_diagnostics::OxcDiagnostic>) -> Diagnostics {
    let mut out: Vec<Diagnostic> = diags
        .iter()
        .map(|d| {
            // Primary label if any, else the last one: for multi-label errors such as a
            // redeclaration, oxc lists the earlier site first and the offending site last.
            let offset = d
                .labels
                .iter()
                .find(|l| l.primary())
                .or(d.labels.last())
                .map_or(0, |l| l.offset());
            let (line, col) = line_col(source, offset);
            Diagnostic {
                path: path.to_string(),
                line,
                col,
                message: d.message.to_string(),
                help: d.help.as_ref().map(|h| h.to_string()),
            }
        })
        .collect();
    out.sort_by_key(|d| (d.line, d.col));
    Diagnostics(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADD_JS: &str = include_str!("../../../tests/fixtures/add.js");

    #[test]
    fn add_js_exports_add() {
        let alloc = Allocator::default();
        let p = parse(&alloc, "add.js", ADD_JS).expect("add.js parses");
        assert_eq!(p.exports.len(), 1);
        assert_eq!(p.exports[0].name, "add");
        assert_eq!(p.exports[0].local.as_deref(), Some("add"));
        assert_eq!(p.exports[0].from, None);
        assert_eq!(p.top_level(), vec![(1, 1, "ExportDeclaration".to_string())]);
    }

    #[test]
    fn syntax_error_has_line_and_col() {
        let alloc = Allocator::default();
        let src = "export function f() {\n  return 1 +;\n}\n";
        let err = parse(&alloc, "bad.js", src).err().expect("must fail");
        let d = &err.0[0];
        assert_eq!((d.line, d.col), (2, 13), "{err}");
        assert!(err.to_string().starts_with("bad.js:2:13: error:"), "{err}");
    }

    #[test]
    fn early_error_from_semantic() {
        // Parser accepts this. The semantic check reports the redeclaration.
        let alloc = Allocator::default();
        let src = "let x = 1;\nlet x = 2;\n";
        let err = parse(&alloc, "redecl.js", src).err().expect("must fail");
        assert_eq!((err.0[0].line, err.0[0].col), (2, 5), "{err}");
    }

    #[test]
    fn export_forms() {
        let alloc = Allocator::default();
        let src = "const a = 1; export { a as b };\nexport default function f() {}\nexport * from './m.js';\nexport { c } from './n.js';\n";
        let p = parse(&alloc, "forms.js", src).expect("parses");
        let got: Vec<_> = p
            .exports
            .iter()
            .map(|e| (e.name.as_str(), e.local.as_deref(), e.from.as_deref()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("b", Some("a"), None),
                ("default", Some("f"), None),
                ("*", None, Some("./m.js")),
                ("c", None, Some("./n.js")),
            ]
        );
    }

    #[test]
    fn default_expression_has_no_local() {
        let alloc = Allocator::default();
        let p = parse(&alloc, "d.js", "export default 5;\n").expect("parses");
        assert_eq!(p.exports[0].name, "default");
        assert_eq!(p.exports[0].local, None);
    }

    #[test]
    fn line_col_counts_chars() {
        assert_eq!(line_col("ab\ncé\nx", 7), (3, 1));
        assert_eq!(line_col("ab\ncé\nx", 3), (2, 1));
        assert_eq!(line_col("é1", 2), (1, 2));
    }
}
