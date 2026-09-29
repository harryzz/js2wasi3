//! Direct `eval` of code known at build time (T42, `docs/design.md` D6).
//!
//! `eval(S)` where `eval` is the global and `S` a string literal (or a template without
//! substitutions) is compiled ahead of time: the call is replaced by an arrow function called at
//! once, whose body is S. That is strict eval code: its own `var` scope, and the caller's bindings,
//! `this`, `super`, `new.target` and `arguments`. When the call's value is used, S is rewritten to
//! return its completion value (the spec's UpdateEmpty rules: `if`, loops, `switch` and `try` start
//! from `undefined`; declarations don't count, a `finally` block only when it completes abruptly).
//!
//! S is first parsed alone as a strict Script (its syntax errors, `return`, `import`/`export`);
//! the module with S in place is then parsed with its early errors, which checks S in the caller's
//! context (`super()` outside a derived constructor, `new.target` outside a function, `arguments`
//! in a class field: PerformEval's extra rules come out the same). A call whose code has an early
//! error becomes `throw new SyntaxError(…)` at run time, as in node (with oxc's message). `eval`
//! calls in S are spliced the same way in the next round. `eval()` and `eval(<non-string literal>)`
//! give their argument. Other direct `eval` calls are build errors (F191).
//!
//! The linker splices each module before it parses it, so its renames reach the eval'd code;
//! [`EvalMap`] takes offsets in the spliced text back to the file.

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType, Span};

use oxc_semantic::SemanticBuilder;

use crate::{Allocator, Diagnostic, Diagnostics, Program, line_col, offset_of, parse};

/// A replaced span: `[at, at + len)` in the new text was `[orig, orig + orig_len)` in the old.
#[derive(Debug, Clone, Copy)]
struct Region {
    at: u32,
    len: u32,
    orig: u32,
    orig_len: u32,
}

/// Offsets in a module's spliced text → offsets in its file. An offset in spliced code maps to
/// the `eval` call it came from.
#[derive(Debug, Clone, Default)]
pub struct EvalMap {
    /// One per round, in order.
    layers: Vec<Vec<Region>>,
}

impl EvalMap {
    pub fn original(&self, mut offset: u32) -> u32 {
        for layer in self.layers.iter().rev() {
            let mut shift: i64 = 0;
            let mut hit = None;
            for r in layer {
                if offset < r.at {
                    break;
                }
                if offset < r.at + r.len {
                    hit = Some(r.orig);
                    break;
                }
                shift += r.len as i64 - r.orig_len as i64;
            }
            offset = hit.unwrap_or((offset as i64 - shift) as u32);
        }
        offset
    }
}

/// Rounds of nested `eval` (eval'd code calling `eval` with a literal).
const MAX_ROUNDS: usize = 16;

/// What a call compiles.
enum Arg {
    /// Direct eval code.
    Code(String),
    /// Indirect eval code (`(0, eval)("…")`, `globalThis.eval("…")`): global, sloppy code.
    Global(String),
    /// `Function(p1, …, body)` / `new Function(…)`: the parameters joined with `,`, the body.
    Function(String, String),
    /// The argument's source text: a value that isn't a string (`eval(1)`), or `undefined`.
    Value(String),
}

struct Call {
    span: Span,
    arg: Arg,
    /// Arguments after the first, evaluated and dropped (`eval` only).
    extra: Vec<Span>,
    /// Is the call's value used (else no completion value is needed)?
    used: bool,
}

/// Parse a module, splicing its direct `eval` calls of code known at build time. The map is
/// `None` when there are none (the program is [`parse`]d as is).
pub(crate) fn parse_spliced<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
) -> Result<(Program<'a>, Option<EvalMap>), Diagnostics> {
    let mut p = parse(alloc, path, source)?;
    let mut map = EvalMap::default();
    let diags = |map: &EvalMap, errors: Vec<(Span, String)>| {
        let d = errors
            .into_iter()
            .map(|(span, message)| {
                let (line, col) = line_col(source, map.original(span.start));
                let help = message.contains("known at build time").then(|| {
                    "only code known at build time can be compiled: string literals (F191)"
                        .to_string()
                });
                Diagnostic {
                    path: path.to_string(),
                    line,
                    col,
                    message,
                    help,
                }
            })
            .collect::<Vec<_>>();
        let mut d = d;
        d.sort_by_key(|d| (d.line, d.col));
        Diagnostics(d)
    };
    for _ in 0..MAX_ROUNDS {
        let (calls, mut errors) = find(&p);
        if calls.is_empty() {
            if !errors.is_empty() {
                return Err(diags(&map, errors));
            }
            break;
        }
        // Errors found before splicing are at offsets in `p`: map them through this round too.
        let (next, layer, more) = splice(alloc, path, &p, &calls)?;
        let at_new = |at: u32| {
            let mut shift: i64 = 0;
            for r in &layer {
                if at < r.orig {
                    break;
                }
                shift += r.len as i64 - r.orig_len as i64;
            }
            (at as i64 + shift) as u32
        };
        for (span, _) in &mut errors {
            *span = Span::sized(at_new(span.start), 0);
        }
        errors.extend(more);
        map.layers.push(layer);
        if !errors.is_empty() {
            return Err(diags(&map, errors));
        }
        p = next;
    }
    p.original = source;
    Ok((p, (!map.layers.is_empty()).then_some(map)))
}

/// A string known at build time: a string literal or a template without substitutions.
fn literal(e: &Expression) -> Option<String> {
    match e.without_parentheses() {
        Expression::StringLiteral(s) => Some(s.value.to_string()),
        Expression::TemplateLiteral(t) if t.expressions.is_empty() => t
            .quasis
            .first()
            .and_then(|q| q.value.cooked)
            .map(|s| s.to_string()),
        _ => None,
    }
}

/// The `eval` and `Function` calls in `p`, and the ones whose code isn't known at build time.
fn find(p: &Program) -> (Vec<Call>, Vec<(Span, String)>) {
    let scoping = p.semantic.scoping();
    let nodes = p.semantic.nodes();
    let global = |e: &Expression, name: &str| match e.without_parentheses() {
        Expression::Identifier(id) => {
            id.name == name
                && scoping
                    .get_reference(id.reference_id())
                    .symbol_id()
                    .is_none()
        }
        _ => false,
    };
    let mut calls = Vec::new();
    let mut errors = Vec::new();
    for node in nodes.iter() {
        let (span, callee, args, optional) = match node.kind() {
            AstKind::CallExpression(c) => (c.span, &c.callee, &c.arguments, c.optional),
            AstKind::NewExpression(n) => (n.span, &n.callee, &n.arguments, false),
            _ => continue,
        };
        let is_new = matches!(node.kind(), AstKind::NewExpression(_));
        let used = !matches!(
            nodes.parent_kind(node.id()),
            AstKind::ExpressionStatement(_)
        );
        if global(callee, "Function") {
            // `Function(…)` and `new Function(…)` (slice 3).
            let texts: Option<Vec<String>> = args
                .iter()
                .map(|a| a.as_expression().and_then(literal))
                .collect();
            let Some(mut texts) = texts else {
                errors.push((
                    span,
                    "unsupported: `Function` of code not known at build time".to_string(),
                ));
                continue;
            };
            let body = texts.pop().unwrap_or_default();
            calls.push(Call {
                span,
                arg: Arg::Function(texts.join(","), body),
                extra: Vec::new(),
                used,
            });
            continue;
        }
        if is_new {
            continue;
        }
        // Direct: `eval(…)`; indirect: `(0, eval)(…)`, `globalThis.eval(…)`, `eval?.(…)`.
        let direct = global(callee, "eval") && !optional;
        let indirect = !direct
            && match callee.without_parentheses() {
                Expression::Identifier(_) => global(callee, "eval"),
                Expression::SequenceExpression(s) => {
                    s.expressions.last().is_some_and(|e| global(e, "eval"))
                }
                Expression::StaticMemberExpression(m) => {
                    m.property.name == "eval"
                        && (global(&m.object, "globalThis") || global(&m.object, "global"))
                }
                _ => false,
            };
        if !direct && !indirect {
            continue;
        }
        if args.iter().any(Argument::is_spread) {
            errors.push((
                span,
                "unsupported: `eval` with a spread argument".to_string(),
            ));
            continue;
        }
        let arg = match args.first().and_then(Argument::as_expression) {
            None => Arg::Value("undefined".to_string()),
            Some(e) => match (literal(e), e.without_parentheses()) {
                (Some(s), _) if direct => Arg::Code(s),
                (Some(s), _) => Arg::Global(s),
                (
                    None,
                    Expression::NumericLiteral(_)
                    | Expression::BooleanLiteral(_)
                    | Expression::NullLiteral(_)
                    | Expression::BigIntLiteral(_),
                ) => Arg::Value(e.span().source_text(p.source).to_string()),
                _ => {
                    errors.push((
                        span,
                        "unsupported: `eval` of code not known at build time".to_string(),
                    ));
                    continue;
                }
            },
        };
        calls.push(Call {
            span,
            arg,
            extra: args.iter().skip(1).map(GetSpan::span).collect(),
            used,
        });
    }
    // In source order; a call inside another's extra arguments waits for the next round.
    calls.sort_by_key(|c| c.span.start);
    let mut end = 0;
    calls.retain(|c| {
        let keep = c.span.start >= end;
        if keep {
            end = c.span.end;
        }
        keep
    });
    (calls, errors)
}

/// Replace the calls; a call whose code fails to parse in place throws a SyntaxError instead.
/// Also the build errors, by offset in the new text: code `Function` or indirect `eval` can't
/// compile, and references from it to the module's bindings (it sees only globals in node).
#[allow(clippy::type_complexity)]
fn splice<'a>(
    alloc: &'a Allocator,
    path: &str,
    p: &Program<'a>,
    calls: &[Call],
) -> Result<(Program<'a>, Vec<Region>, Vec<(Span, String)>), Diagnostics> {
    let text = p.source;
    let completion = fresh_name(text, calls);
    let mut failed = vec![None; calls.len()];
    let mut pieces: Vec<(String, bool)> = calls
        .iter()
        .enumerate()
        .map(|(i, c)| match generate(alloc, text, c, &completion) {
            Ok(piece) => piece,
            Err(message) => {
                failed[i] = Some(message);
                ("(void 0)".to_string(), true)
            }
        })
        .collect();
    loop {
        let mut out = String::with_capacity(text.len());
        let mut regions = Vec::new();
        let mut pos = 0;
        for (c, (piece, _)) in calls.iter().zip(&pieces) {
            out.push_str(&text[pos..c.span.start as usize]);
            regions.push(Region {
                at: out.len() as u32,
                len: piece.len() as u32,
                orig: c.span.start,
                orig_len: c.span.size(),
            });
            out.push_str(piece);
            pos = c.span.end as usize;
        }
        out.push_str(&text[pos..]);
        let out: &'a str = alloc.alloc_str(&out);
        let d = match parse(alloc, path, out) {
            Ok(next) => {
                let mut errors: Vec<(Span, String)> = failed
                    .iter()
                    .zip(&regions)
                    .filter_map(|(f, r)| Some((Span::sized(r.at, 0), f.clone()?)))
                    .collect();
                errors.extend(module_refs(&next, calls, &regions));
                return Ok((next, regions, errors));
            }
            Err(d) => d,
        };
        // Early errors in spliced code: that call throws.
        let mut changed = false;
        for diag in &d.0 {
            let at = offset_of(out, diag.line, diag.col);
            let Some(i) = regions.iter().position(|r| r.at <= at && at < r.at + r.len) else {
                return Err(d);
            };
            if !pieces[i].1 {
                pieces[i] = (throw_syntax_error(&diag.message), true);
                changed = true;
            }
        }
        if !changed {
            return Err(d);
        }
    }
}

/// A name for the completion value, used nowhere in the text or the eval'd code.
fn fresh_name(text: &str, calls: &[Call]) -> String {
    (0..)
        .map(|k| format!("eval$completion{k}"))
        .find(|n| {
            !text.contains(n.as_str())
                && calls
                    .iter()
                    .all(|c| !matches!(&c.arg, Arg::Code(s) if s.contains(n.as_str())))
        })
        .expect("a fresh name")
}

/// `(() => { throw new SyntaxError(message); })()`.
fn throw_syntax_error(message: &str) -> String {
    format!(
        "((() => {{ throw new SyntaxError({}); }})())",
        json_string(message)
    )
}

/// References from `Function` or indirect `eval` code to bindings declared outside it: node runs
/// that code in the global scope, where the module's bindings aren't.
fn module_refs(p: &Program, calls: &[Call], regions: &[Region]) -> Vec<(Span, String)> {
    let scoping = p.semantic.scoping();
    let global: Vec<&Region> = calls
        .iter()
        .zip(regions)
        .filter(|(c, _)| matches!(c.arg, Arg::Function(..) | Arg::Global(_)))
        .map(|(_, r)| r)
        .collect();
    let mut out = Vec::new();
    if global.is_empty() {
        return out;
    }
    for node in p.semantic.nodes().iter() {
        let AstKind::IdentifierReference(id) = node.kind() else {
            continue;
        };
        let inside = |r: &&Region, at: u32| r.at <= at && at < r.at + r.len;
        let Some(r) = global.iter().find(|r| inside(r, id.span.start)) else {
            continue;
        };
        if let Some(sym) = scoping.get_reference(id.reference_id()).symbol_id()
            && !inside(r, scoping.symbol_span(sym).start)
        {
            out.push((
                id.span,
                format!(
                    "unsupported: `{}` in `Function` or indirect `eval` code names a binding of the module (in node that code sees only globals)",
                    id.name
                ),
            ));
        }
    }
    out
}

/// The text replacing one call, and whether it throws a SyntaxError. Or why it can't compile.
fn generate(
    alloc: &Allocator,
    text: &str,
    c: &Call,
    completion: &str,
) -> Result<(String, bool), String> {
    let extra: String = c
        .extra
        .iter()
        .map(|s| format!("{}, ", s.source_text(text)))
        .collect();
    let (code, throws) = match &c.arg {
        Arg::Value(v) => (format!("({v})"), false),
        Arg::Code(s) => match compile(alloc, s, c.used.then_some(completion), false)? {
            Ok(body) => (format!("((() => {{\n{body}\n}})())"), false),
            Err(message) => (throw_syntax_error(&message), true),
        },
        Arg::Global(s) => match compile(alloc, s, c.used.then_some(completion), true)? {
            Ok(body) => (
                format!("((function () {{\n{body}\n}}).call(globalThis))"),
                false,
            ),
            Err(message) => (throw_syntax_error(&message), true),
        },
        Arg::Function(params, body) => {
            match function(alloc, params, body, &format!("{completion}t")) {
                Ok(f) => (format!("({f})"), false),
                Err(message) => (throw_syntax_error(&message), true),
            }
        }
    };
    Ok(if extra.is_empty() {
        (code, throws)
    } else {
        (format!("({extra}{code})"), throws)
    })
}

/// `Function(params, body)`'s function: `function anonymous(<params>\n) {\n<body>\n}` (its
/// source text, as the spec builds it), or the message of its syntax error. It's sloppy code: when
/// its body isn't strict, `this` in it is `this_name`, the global object for `undefined`/`null`
/// and ToObject otherwise.
fn function(
    alloc: &Allocator,
    params: &str,
    body: &str,
    this_name: &str,
) -> Result<String, String> {
    const PRE: &str = "(function anonymous(";
    let text = format!("{PRE}{params}\n) {{\n{body}\n}})");
    let wrapped = alloc.alloc_str(&text);
    let ret = Parser::new(alloc, wrapped, SourceType::script()).parse();
    if let Some(e) = ret
        .diagnostics
        .iter()
        .find(|d| d.severity == oxc_diagnostics::Severity::Error)
    {
        return Err(e.message.to_string());
    }
    if ret.fatal_error {
        return Err("Invalid or unexpected token".to_string());
    }
    let program = alloc.alloc(ret.program);
    // The parameters and the body must each parse alone: no `) {` or `}` breaking out.
    let open = (PRE.len() + params.len() + 3) as u32;
    let f = match program.body.as_slice() {
        [Statement::ExpressionStatement(e)] => match e.expression.without_parentheses() {
            Expression::FunctionExpression(f) => Some(f),
            _ => None,
        },
        _ => None,
    };
    let Some(f) = f.filter(|f| {
        f.params.span == Span::new(PRE.len() as u32 - 1, open - 1)
            && f.body
                .as_ref()
                .is_some_and(|b| b.span == Span::new(open, open + body.len() as u32 + 4))
    }) else {
        return Err("Arg string terminates parameters early".to_string());
    };
    let strict = f
        .body
        .as_ref()
        .is_some_and(|b| b.directives.iter().any(|d| d.directive == "use strict"));
    let source = &wrapped[1..wrapped.len() - 1];
    if strict {
        return Ok(source.to_string());
    }
    // Sloppy `this`: the uses in the function itself (and its arrows).
    let sem = SemanticBuilder::new().with_build_nodes(true).build(program);
    let nodes = sem.semantic.nodes();
    let mut uses: Vec<u32> = nodes
        .iter()
        .filter(|n| matches!(n.kind(), AstKind::ThisExpression(_)))
        .filter(|n| {
            nodes.ancestors(n.id()).find_map(|a| match a.kind() {
                AstKind::Function(g) => Some(g.span == f.span),
                _ => None,
            }) == Some(true)
        })
        .map(|n| n.kind().span().start)
        .filter(|at| *at > open)
        .collect();
    if uses.is_empty() {
        return Ok(source.to_string());
    }
    uses.sort_unstable();
    let mut out = String::new();
    let mut pos = 1;
    let body_start = open as usize + 1;
    out.push_str(&wrapped[pos..body_start]);
    out.push_str(&format!(
        " const {this_name} = this == null ? globalThis : Object(this);"
    ));
    pos = body_start;
    for at in uses {
        out.push_str(&wrapped[pos..at as usize]);
        out.push_str(this_name);
        pos = at as usize + 4;
    }
    out.push_str(&wrapped[pos..wrapped.len() - 1]);
    Ok(out)
}

/// The function body for eval code `s`: `s` itself, or with `completion` (a name) its completion
/// value returned. Or (inner `Err`) the message of its syntax error, or (outer `Err`) why it can't
/// compile. `global`: indirect eval code (sloppy, the global scope).
fn compile(
    alloc: &Allocator,
    s: &str,
    completion: Option<&str>,
    global: bool,
) -> Result<Result<String, String>, String> {
    // Direct eval code is a Script, strict here since the caller is (an ES module). It's parsed in
    // a derived constructor, where `super`, `new.target` and `arguments` parse: the in-place parse
    // checks them in the caller's context.
    let (pre, post) = if global {
        ("", "")
    } else {
        (
            "\"use strict\";\n(class extends Object { constructor() {\n",
            "\n} });",
        )
    };
    let wrapped = alloc.alloc_str(&format!("{pre}{s}{post}"));
    let ret = Parser::new(alloc, wrapped, SourceType::script()).parse();
    if let Some(e) = ret
        .diagnostics
        .iter()
        .find(|d| d.severity == oxc_diagnostics::Severity::Error)
    {
        return Ok(Err(e.message.to_string()));
    }
    if ret.fatal_error {
        return Ok(Err("Invalid or unexpected token".to_string()));
    }
    let (directives, statements) = if global {
        (&ret.program.directives, &ret.program.body)
    } else {
        // The constructor's body must be exactly `s` (no `} m() {` breaking out of it).
        let body = match ret.program.body.as_slice() {
            [Statement::ExpressionStatement(e)] => match e.expression.without_parentheses() {
                Expression::ClassExpression(c) => match c.body.body.as_slice() {
                    [ClassElement::MethodDefinition(m)] => m.value.body.as_deref(),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        let Some(body) = body.filter(|b| {
            b.span == Span::new(pre.len() as u32 - 2, (pre.len() + s.len() + 2) as u32)
        }) else {
            return Ok(Err("Unexpected token '}'".to_string()));
        };
        (&body.directives, &body.statements)
    };
    if statements.iter().any(has_return) {
        return Ok(Err("Illegal return statement".to_string()));
    }
    // Indirect eval code (T44): its `var` and function declarations are global-object properties.
    let mut edits: Vec<(u32, u32, i64, String)> = Vec::new();
    let mut prologue = String::new();
    // Strict eval code has its own variable environment (a `"use strict"` directive).
    let strict = directives.iter().any(|d| d.directive == "use strict");
    if global && !strict {
        let mut d = Decls {
            text: wrapped,
            vars: Vec::new(),
            fns: Vec::new(),
            edits: Vec::new(),
            error: None,
        };
        for stmt in statements {
            d.stmt(stmt, true);
        }
        if let Some(e) = d.error {
            return Err(e);
        }
        prologue = d.prologue();
        edits = d.edits;
    }
    if completion.is_none() && edits.is_empty() {
        return Ok(Ok(s.to_string()));
    }
    let mut out = prologue;
    if let Some(c) = completion {
        let assign = format!("{c} = (");
        let reset = format!("{{ {c} = void 0; ");
        let saved = format!("{c}f");
        let (save, restore) = (
            format!(" const {saved} = {c}; {c} = void 0; "),
            format!(";\n{c} = {saved}; "),
        );
        let mut w = Completion {
            assign: &assign,
            reset: &reset,
            save: &save,
            restore: &restore,
            edits: Vec::new(),
        };
        // Directives are expression statements' values too.
        for d in directives {
            w.open(d.expression.span.start, &assign);
            w.close(d.expression.span.end, ")");
        }
        for stmt in statements {
            w.stmt(stmt, true);
        }
        edits.extend(
            w.edits
                .into_iter()
                .map(|(at, rank, t)| (at, at, rank, t.to_string())),
        );
        out.push_str(&format!("let {c};\n"));
    }
    // At one offset, closing text comes first, innermost first; then opening text, outermost first
    // (replacements, which never share an offset with other edits, rank 0).
    edits.sort_by_key(|(at, _, rank, _)| (*at, *rank));
    let mut pos = pre.len();
    for (at, end, _, t) in edits {
        out.push_str(&wrapped[pos..at as usize]);
        out.push_str(&t);
        pos = end as usize;
    }
    out.push_str(&wrapped[pos..pre.len() + s.len()]);
    if let Some(c) = completion {
        out.push_str(&format!("\n;return {c};"));
    }
    Ok(Ok(out))
}

/// Indirect eval code's declarations (T44): each `var` declaration becomes assignments (its names are
/// then undeclared, so they go through the global object), each top-level function declaration a
/// named function expression in a prologue that declares them all on the global object first
/// (EvalDeclarationInstantiation with the global environment).
struct Decls<'t> {
    text: &'t str,
    vars: Vec<String>,
    /// Name and source text, the last declaration of a name wins.
    fns: Vec<(String, String)>,
    edits: Vec<(u32, u32, i64, String)>,
    error: Option<String>,
}

impl Decls<'_> {
    fn fail(&mut self, what: &str) {
        self.error
            .get_or_insert_with(|| format!("unsupported: {what} in indirect `eval` code"));
    }

    fn stmt(&mut self, s: &Statement, top: bool) {
        match s {
            Statement::VariableDeclaration(d) if d.kind == VariableDeclarationKind::Var => {
                let semi = self.text.as_bytes().get(d.span.end as usize - 1) == Some(&b';');
                if let Some(a) = self.assigns(d) {
                    let t = if a.is_empty() {
                        "void 0".to_string()
                    } else {
                        format!("void ({a})")
                    };
                    self.edits.push((
                        d.span.start,
                        d.span.end,
                        0,
                        format!("{t}{}", if semi { ";" } else { "" }),
                    ));
                }
            }
            Statement::FunctionDeclaration(f) => {
                let Some(id) = &f.id else { return };
                if !top {
                    self.fail("a function declaration in a block");
                    return;
                }
                let name = id.name.to_string();
                self.fns.retain(|(n, _)| *n != name);
                self.fns
                    .push((name, f.span.source_text(self.text).to_string()));
                self.edits
                    .push((f.span.start, f.span.end, 0, String::new()));
            }
            Statement::BlockStatement(b) => b.body.iter().for_each(|s| self.stmt(s, false)),
            Statement::LabeledStatement(l) => self.stmt(&l.body, false),
            Statement::IfStatement(i) => {
                self.stmt(&i.consequent, false);
                if let Some(a) = &i.alternate {
                    self.stmt(a, false);
                }
            }
            Statement::WhileStatement(w) => self.stmt(&w.body, false),
            Statement::DoWhileStatement(w) => self.stmt(&w.body, false),
            Statement::ForStatement(f) => {
                if let Some(ForStatementInit::VariableDeclaration(d)) = &f.init
                    && d.kind == VariableDeclarationKind::Var
                    && let Some(a) = self.assigns(d)
                {
                    self.edits.push((d.span.start, d.span.end, 0, a));
                }
                self.stmt(&f.body, false);
            }
            Statement::ForInStatement(f) => {
                if matches!(&f.left, ForStatementLeft::VariableDeclaration(d) if d.kind == VariableDeclarationKind::Var)
                {
                    self.fail("`for (var … in …)`");
                }
                self.stmt(&f.body, false);
            }
            Statement::ForOfStatement(f) => {
                if matches!(&f.left, ForStatementLeft::VariableDeclaration(d) if d.kind == VariableDeclarationKind::Var)
                {
                    self.fail("`for (var … of …)`");
                }
                self.stmt(&f.body, false);
            }
            Statement::SwitchStatement(sw) => {
                for c in &sw.cases {
                    c.consequent.iter().for_each(|s| self.stmt(s, false));
                }
            }
            Statement::TryStatement(t) => {
                t.block.body.iter().for_each(|s| self.stmt(s, false));
                if let Some(h) = &t.handler {
                    h.body.body.iter().for_each(|s| self.stmt(s, false));
                }
                if let Some(f) = &t.finalizer {
                    f.body.iter().for_each(|s| self.stmt(s, false));
                }
            }
            _ => {}
        }
    }

    /// `var a = 1, b` → `a = (1)` (the names noted), or `None` after a failure.
    fn assigns(&mut self, d: &VariableDeclaration) -> Option<String> {
        let mut parts = Vec::new();
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                self.fail("a destructuring `var`");
                return None;
            };
            let name = id.name.to_string();
            if !self.vars.contains(&name) {
                self.vars.push(name.clone());
            }
            if let Some(init) = &decl.init {
                parts.push(format!("{name} = ({})", init.span().source_text(self.text)));
            }
        }
        Some(parts.join(", "))
    }

    /// The declarations on the global object: node's checks first, then the functions (defined, or
    /// set when non-configurable), then the `var`s (`undefined` where absent); all configurable.
    fn prologue(&self) -> String {
        let vars: Vec<&String> = self
            .vars
            .iter()
            .filter(|v| !self.fns.iter().any(|(n, _)| n == *v))
            .collect();
        if vars.is_empty() && self.fns.is_empty() {
            return String::new();
        }
        let fns: Vec<String> = self
            .fns
            .iter()
            .map(|(n, t)| format!("[{}, {t}]", json_string(n)))
            .collect();
        let vars: Vec<String> = vars.iter().map(|v| json_string(v)).collect();
        format!(
            "((g, fs, vs) => {{ \
             const own = (n) => Object.getOwnPropertyDescriptor(g, n); \
             const add = (n) => new TypeError(\"Cannot add property \" + n + \", object is not extensible\"); \
             for (const [n] of fs) {{ const d = own(n); \
             if (d === undefined ? !Object.isExtensible(g) : !d.configurable && !(d.writable && d.enumerable)) \
             throw d === undefined ? add(n) : new TypeError(\"Identifier '\" + n + \"' has already been declared\"); }} \
             for (const n of vs) {{ if (own(n) === undefined && !Object.isExtensible(g)) throw add(n); }} \
             for (const [n, f] of fs) {{ const d = own(n); \
             if (d === undefined || d.configurable) Object.defineProperty(g, n, {{ value: f, writable: true, enumerable: true, configurable: true }}); \
             else g[n] = f; }} \
             for (const n of vs) {{ if (own(n) === undefined) Object.defineProperty(g, n, {{ value: undefined, writable: true, enumerable: true, configurable: true }}); }} \
             }})(globalThis, [{}], [{}]);\n",
            fns.join(", "),
            vars.join(", ")
        )
    }
}

/// Does a statement (outside nested functions) `return`? Only statements can contain one outside
/// a function.
fn has_return(s: &Statement) -> bool {
    match s {
        Statement::ReturnStatement(_) => true,
        Statement::BlockStatement(b) => b.body.iter().any(has_return),
        Statement::LabeledStatement(l) => has_return(&l.body),
        Statement::IfStatement(i) => {
            has_return(&i.consequent) || i.alternate.as_ref().is_some_and(has_return)
        }
        Statement::WhileStatement(w) => has_return(&w.body),
        Statement::DoWhileStatement(w) => has_return(&w.body),
        Statement::ForStatement(f) => has_return(&f.body),
        Statement::ForInStatement(f) => has_return(&f.body),
        Statement::ForOfStatement(f) => has_return(&f.body),
        Statement::WithStatement(w) => has_return(&w.body),
        Statement::SwitchStatement(sw) => {
            sw.cases.iter().any(|c| c.consequent.iter().any(has_return))
        }
        Statement::TryStatement(t) => {
            t.block.body.iter().any(has_return)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(has_return))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(has_return))
        }
        _ => false,
    }
}

/// The completion-value rewrite of eval code's statements: an expression statement stores its
/// value; a statement whose value is UpdateEmpty(…, undefined) first stores `undefined` (in a new
/// block: `{ c = void 0; while (…) … }`).
struct Completion<'e> {
    assign: &'e str,
    reset: &'e str,
    /// Around a `finally` block: its values count only if it completes abruptly (`break`), so
    /// the value before it is saved and restored at its end.
    save: &'e str,
    restore: &'e str,
    /// (offset, order at that offset, text).
    edits: Vec<(u32, i64, &'e str)>,
}

impl<'e> Completion<'e> {
    fn open(&mut self, at: u32, t: &'e str) {
        let rank = self.edits.len() as i64;
        self.edits.push((at, rank, t));
    }

    fn close(&mut self, at: u32, t: &'e str) {
        let rank = -(self.edits.len() as i64) - 1_000_000_000;
        self.edits.push((at, rank, t));
    }

    /// `reset`: store `undefined` first (not for a labelled statement's body: the label must stay
    /// on the loop).
    fn stmt(&mut self, s: &Statement, reset: bool) {
        let wrap = |w: &mut Self, span: Span| {
            if reset {
                w.open(span.start, w.reset);
                w.close(span.end, " }");
            }
        };
        match s {
            Statement::ExpressionStatement(e) => {
                self.open(e.expression.span().start, self.assign);
                self.close(e.expression.span().end, ")");
            }
            Statement::BlockStatement(b) => {
                for s in &b.body {
                    self.stmt(s, true);
                }
            }
            Statement::LabeledStatement(l) => {
                let breakable = matches!(
                    l.body,
                    Statement::IfStatement(_)
                        | Statement::WhileStatement(_)
                        | Statement::DoWhileStatement(_)
                        | Statement::ForStatement(_)
                        | Statement::ForInStatement(_)
                        | Statement::ForOfStatement(_)
                        | Statement::SwitchStatement(_)
                        | Statement::TryStatement(_)
                        | Statement::LabeledStatement(_)
                );
                if breakable {
                    wrap(self, l.span);
                }
                self.stmt(&l.body, false);
            }
            Statement::IfStatement(i) => {
                wrap(self, i.span);
                self.stmt(&i.consequent, true);
                if let Some(a) = &i.alternate {
                    self.stmt(a, true);
                }
            }
            Statement::WhileStatement(w) => {
                wrap(self, w.span);
                self.stmt(&w.body, true);
            }
            Statement::DoWhileStatement(w) => {
                wrap(self, w.span);
                self.stmt(&w.body, true);
            }
            Statement::ForStatement(f) => {
                wrap(self, f.span);
                self.stmt(&f.body, true);
            }
            Statement::ForInStatement(f) => {
                wrap(self, f.span);
                self.stmt(&f.body, true);
            }
            Statement::ForOfStatement(f) => {
                wrap(self, f.span);
                self.stmt(&f.body, true);
            }
            Statement::SwitchStatement(sw) => {
                wrap(self, sw.span);
                for case in &sw.cases {
                    for s in &case.consequent {
                        self.stmt(s, true);
                    }
                }
            }
            Statement::TryStatement(t) => {
                wrap(self, t.span);
                for s in &t.block.body {
                    self.stmt(s, true);
                }
                if let Some(h) = &t.handler {
                    for s in &h.body.body {
                        self.stmt(s, true);
                    }
                }
                if let Some(f) = &t.finalizer {
                    self.open(f.span.start + 1, self.save);
                    for s in &f.body {
                        self.stmt(s, true);
                    }
                    self.close(f.span.end - 1, self.restore);
                }
            }
            _ => {}
        }
    }
}

/// A JS string literal.
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_map_back() {
        let alloc = Allocator::default();
        let src = "let a = 1;\nconsole.log(eval(\"a + 1\"));\nfunction f() { return eval(\"arguments\"); }\nconsole.log(f());\n";
        let (p, map) = parse_spliced(&alloc, "t.js", src).unwrap();
        let map = map.unwrap();
        let spliced = p.source;
        let at = spliced.find("arguments").unwrap() as u32;
        assert_eq!(line_col(src, map.original(at)), (3, 23), "{spliced}");
        let at = spliced.find("f());").unwrap() as u32;
        assert_eq!(line_col(src, map.original(at)), (4, 13));
    }
}
