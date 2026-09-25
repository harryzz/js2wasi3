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
    pub ast: &'a ast::Program<'a>,
    pub semantic: Semantic<'a>,
    pub exports: Vec<Export>,
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
    let ret = Parser::new(alloc, source, SourceType::mjs()).parse();
    if ret.fatal_error || ret.diagnostics.has_errors() {
        return Err(convert(path, source, ret.diagnostics.into_vec()));
    }
    let exports = collect_exports(&ret.module_record);

    let ast = alloc.alloc(ret.program);
    let sem = SemanticBuilder::new()
        .with_check_syntax_error(true)
        .build(ast);
    if sem.diagnostics.has_errors() {
        return Err(convert(path, source, sem.diagnostics.into_vec()));
    }

    Ok(Program {
        path: path.to_string(),
        source,
        ast,
        semantic: sem.semantic,
        exports,
    })
}

impl Program<'_> {
    /// 1-based line and column of a byte offset in this program's source.
    pub fn line_col(&self, offset: u32) -> (u32, u32) {
        line_col(self.source, offset)
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

// The variant name from `Debug` is the only kind name that doesn't need
// updating whenever oxc adds a statement kind. Used for dumps, not hot paths.
fn statement_kind(stmt: &ast::Statement) -> String {
    let dbg = format!("{stmt:?}");
    dbg.split(['(', ' ', '{'])
        .next()
        .unwrap_or("Statement")
        .to_string()
}

fn collect_exports(record: &ModuleRecord) -> Vec<Export> {
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
