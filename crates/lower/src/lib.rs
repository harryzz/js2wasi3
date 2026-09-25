//! Lowering: checked JS AST ([`js2wasi3_frontend::Program`]) → [`js2wasi3_ir::Module`].
//!
//! # T05 subset: numbers only
//! Every JS value is an `f64`. Comparisons, `!`, `&&`/`||` over booleans and boolean
//! literals produce IR `Bool`, which may **only** be used as a condition: nothing in this
//! subset can observe a boolean as a value, so no boolean representation is needed yet.
//!
//! Supported: top-level `function` declarations (exported or not), simple parameters,
//! `var`/`let`/`const` with an initializer, top-level variables with a numeric-literal
//! initializer (they become globals), number literals, `+ - * / %`, unary `- + ! ~`,
//! `| & ^ << >>`, comparisons, `&& || ??`, `?:`, assignment and compound assignment,
//! `++`/`--` (postfix only as a statement), direct calls with exact arity, `if`,
//! `while`, `do/while`, `for`, labels, `break`, `continue`, `return`.
//!
//! Anything else produces `unsupported: <construct> (planned: <task>)` at its source
//! position. The lowering never guesses: a construct it can't lower exactly is rejected.
//! All diagnostics of a file are collected, not just the first.

use std::collections::HashMap;

use js2wasi3_frontend::{Diagnostic, Diagnostics, Program, line_col, statement_kind};
use js2wasi3_ir::{
    self as ir, BinOp, Const, Expr, FuncId, GlobalId, LabelId, LocalId, Pos, Stmt, Type, UnOp,
};
use oxc_ast::ast::*;
use oxc_semantic::Scoping;
use oxc_span::{GetSpan, Span};
use oxc_syntax::{
    operator::{
        AssignmentOperator, BinaryOperator, LogicalOperator, UnaryOperator, UpdateOperator,
    },
    symbol::SymbolId,
};

/// Lower a whole module. Returns every unsupported construct as a diagnostic.
pub fn lower(program: &Program) -> Result<ir::Module, Diagnostics> {
    let mut l = Lowerer {
        p: program,
        scoping: program.semantic.scoping(),
        module: ir::Module::default(),
        funcs: HashMap::new(),
        globals: HashMap::new(),
        names: HashMap::new(),
        diags: Vec::new(),
    };
    l.run();
    if l.diags.is_empty() {
        Ok(l.module)
    } else {
        l.diags.sort_by_key(|d| (d.line, d.col));
        Err(Diagnostics(l.diags))
    }
}

/// Marker: a diagnostic has been recorded, abandon the current construct.
struct Unsupported;
type R<T> = Result<T, Unsupported>;

#[derive(Clone, Copy)]
enum Binding {
    Func(FuncId),
    Global,
}

struct Lowerer<'p, 'a> {
    p: &'p Program<'a>,
    scoping: &'p Scoping,
    module: ir::Module,
    funcs: HashMap<SymbolId, FuncId>,
    globals: HashMap<SymbolId, GlobalId>,
    /// Top-level names, for resolving the export list.
    names: HashMap<String, Binding>,
    diags: Vec<Diagnostic>,
}

/// A value being built: an IR expression and its type (`F64` or `Bool`).
struct Val {
    e: Expr,
    ty: Type,
    span: Span,
}

/// A `break`/`continue` target.
struct Target {
    js_label: Option<String>,
    brk: LabelId,
    kind: TargetKind,
}

enum TargetKind {
    /// Labelled non-loop statement: only `break label` may target it.
    Block,
    /// `while`: `continue` jumps straight to the loop head.
    While,
    /// `for` / `do-while`: `continue` must still run the update / test, so it breaks out of
    /// a block around the body. The block label is created on first use.
    Wrapped(Option<LabelId>),
}

struct FnCx {
    func: ir::Function,
    locals: HashMap<SymbolId, LocalId>,
    next_label: u32,
    targets: Vec<Target>,
}

impl FnCx {
    fn label(&mut self) -> LabelId {
        self.next_label += 1;
        LabelId(self.next_label - 1)
    }
}

enum Place {
    Local(LocalId),
    Global(GlobalId),
}

impl<'p, 'a> Lowerer<'p, 'a> {
    // ---- diagnostics -------------------------------------------------------------------

    fn unsupported(&mut self, span: Span, what: &str, planned: &str) -> Unsupported {
        let (line, col) = line_col(self.p.source, span.start);
        self.diags.push(Diagnostic {
            path: self.p.path.clone(),
            line,
            col,
            message: format!("unsupported: {what} (planned: {planned})"),
            help: None,
        });
        Unsupported
    }

    fn pos(&self, span: Span) -> Pos {
        let (line, col) = line_col(self.p.source, span.start);
        Pos { line, col }
    }

    // ---- module ------------------------------------------------------------------------

    fn run(&mut self) {
        let body = &self.p.ast.body;
        // Pass 1: declare every top-level function and global so bodies can refer to them in any order.
        let mut fns: Vec<(&'a Function<'a>, FuncId)> = Vec::new();
        for stmt in body.iter() {
            let _ = self.declare_top(stmt, &mut fns);
        }
        // Pass 2: lower bodies.
        for (f, id) in fns {
            let _ = self.lower_function(f, id);
        }
        // Pass 3: exports.
        for e in &self.p.exports {
            let _ = self.export(e);
        }
    }

    fn declare_top(
        &mut self,
        stmt: &'a Statement<'a>,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<()> {
        match stmt {
            Statement::FunctionDeclaration(f) => self.declare_function(f, fns),
            Statement::VariableDeclaration(d) => self.declare_globals(d),
            Statement::ExportDeclaration(e) => match &e.declaration {
                Declaration::FunctionDeclaration(f) => self.declare_function(f, fns),
                Declaration::VariableDeclaration(d) => {
                    self.declare_globals(d)?;
                    Err(self.unsupported(
                        e.span,
                        "exporting a variable (only functions can be exported)",
                        "T20",
                    ))
                }
                _ => Err(self.unsupported(
                    e.span,
                    "exported declaration other than a function",
                    "T16/T20",
                )),
            },
            Statement::ExportDefaultDeclaration(e) => match &e.declaration {
                ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.id.is_some() => {
                    self.declare_function(f, fns)
                }
                _ => Err(self.unsupported(
                    e.span,
                    "`export default` of anything but a named function",
                    "T14",
                )),
            },
            // `export { a, b as c }`: resolved from the export list in pass 3.
            Statement::ExportNamedDeclaration(_) => Ok(()),
            Statement::ImportDeclaration(i) => Err(self.unsupported(
                i.span,
                "import of another module (multi-file modules, F7)",
                "not scheduled",
            )),
            Statement::ExportFromDeclaration(e) => Err(self.unsupported(
                e.span,
                "re-export from another module (multi-file modules, F7)",
                "not scheduled",
            )),
            Statement::ExportAllDeclaration(e) => Err(self.unsupported(
                e.span,
                "`export *` from another module (multi-file modules, F7)",
                "not scheduled",
            )),
            Statement::EmptyStatement(_) => Ok(()),
            other => Err(self.unsupported(
                other.span(),
                &format!(
                    "top-level {} (top-level code runs in the command world)",
                    statement_kind(other)
                ),
                "T10",
            )),
        }
    }

    fn declare_function(
        &mut self,
        f: &'a Function<'a>,
        fns: &mut Vec<(&'a Function<'a>, FuncId)>,
    ) -> R<()> {
        let id_node = f.id.as_ref().expect("function declarations have a name");
        if f.r#async {
            return Err(self.unsupported(f.span, "async function", "T19"));
        }
        if f.generator {
            return Err(self.unsupported(f.span, "generator function", "T18"));
        }
        let mut params = Vec::new();
        let mut bad = false;
        for p in &f.params.items {
            match (&p.pattern, &p.initializer) {
                (BindingPattern::BindingIdentifier(b), None) => {
                    params.push((b.name.as_str(), b.symbol_id.get()))
                }
                _ => {
                    self.unsupported(p.span, "default or destructured parameter", "T15");
                    bad = true;
                }
            }
        }
        if let Some(rest) = &f.params.rest {
            self.unsupported(rest.span, "rest parameter", "T15");
            bad = true;
        }
        let body = f.body.as_ref().expect("function declarations have a body");
        let returns_value = body.statements.iter().any(has_value_return);
        let sig: Vec<(&str, Type)> = params.iter().map(|(n, _)| (*n, Type::F64)).collect();
        let mut func = ir::Function::new(
            id_node.name.as_str(),
            &sig,
            returns_value.then_some(Type::F64),
        );
        func.pos = Some(self.pos(f.span));
        let id = self.module.add_function(func);
        if let Some(sym) = id_node.symbol_id.get() {
            self.funcs.insert(sym, id);
        }
        self.names
            .insert(id_node.name.to_string(), Binding::Func(id));
        if bad {
            return Err(Unsupported);
        }
        fns.push((f, id));
        Ok(())
    }

    fn declare_globals(&mut self, d: &'a VariableDeclaration<'a>) -> R<()> {
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                result = Err(self.unsupported(decl.span, "destructuring declaration", "T15"));
                continue;
            };
            let Some(init) = decl.init.as_ref().and_then(literal_number) else {
                result = Err(self.unsupported(
                    decl.span,
                    "top-level variable without a numeric-literal initializer (needs module init code)",
                    "T10",
                ));
                continue;
            };
            self.module.globals.push(ir::Global {
                name: b.name.to_string(),
                ty: Type::F64,
                mutable: d.kind != VariableDeclarationKind::Const,
                init: Const::F64(init),
            });
            let id = GlobalId(self.module.globals.len() as u32 - 1);
            if let Some(sym) = b.symbol_id.get() {
                self.globals.insert(sym, id);
            }
            self.names.insert(b.name.to_string(), Binding::Global);
        }
        result
    }

    fn export(&mut self, e: &js2wasi3_frontend::Export) -> R<()> {
        if e.from.is_some() {
            return Ok(()); // already reported as a re-export in pass 1
        }
        match e.local.as_deref().and_then(|l| self.names.get(l)).copied() {
            Some(Binding::Func(func)) => {
                let pos = Some(self.pos(e.span));
                self.module.exports.push(ir::Export {
                    name: e.name.clone(),
                    func,
                    string_name: e.string_name,
                    pos,
                });
                Ok(())
            }
            // Exported globals and non-function defaults are reported in pass 1.
            Some(Binding::Global) => Ok(()),
            None => Ok(()),
        }
    }

    // ---- functions ---------------------------------------------------------------------

    fn lower_function(&mut self, f: &'a Function<'a>, id: FuncId) -> R<()> {
        let func = self.module.functions[id.0 as usize].clone();
        let mut cx = FnCx {
            func,
            locals: HashMap::new(),
            next_label: 0,
            targets: Vec::new(),
        };
        for (i, p) in f.params.items.iter().enumerate() {
            if let BindingPattern::BindingIdentifier(b) = &p.pattern
                && let Some(sym) = b.symbol_id.get()
            {
                cx.locals.insert(sym, cx.func.param(i as u32));
            }
        }
        let body = f.body.as_ref().expect("function declarations have a body");
        let before = self.diags.len();
        let stmts = self.stmts(&mut cx, &body.statements);
        if cx.func.result.is_some() && !always_returns(&body.statements) {
            let span = Span::new(body.span.end.saturating_sub(1), body.span.end);
            self.unsupported(
                span,
                "function may reach its end without returning a value (implicit `undefined`)",
                "T08",
            );
        }
        if self.diags.len() > before {
            return Err(Unsupported);
        }
        cx.func.body = stmts;
        self.module.functions[id.0 as usize] = cx.func;
        Ok(())
    }

    // ---- statements --------------------------------------------------------------------

    /// Lower a statement list, continuing after errors so every problem is reported.
    fn stmts(&mut self, cx: &mut FnCx, list: &'a [Statement<'a>]) -> Vec<Stmt> {
        let mut out = Vec::new();
        for s in list {
            let _ = self.stmt(cx, s, &mut out);
        }
        out
    }

    fn stmt_body(&mut self, cx: &mut FnCx, s: &'a Statement<'a>) -> Vec<Stmt> {
        let mut out = Vec::new();
        let _ = self.stmt(cx, s, &mut out);
        out
    }

    fn stmt(&mut self, cx: &mut FnCx, s: &'a Statement<'a>, out: &mut Vec<Stmt>) -> R<()> {
        match s {
            Statement::EmptyStatement(_) => Ok(()),
            Statement::ExpressionStatement(e) => self.expr_stmt(cx, &e.expression, out),
            Statement::BlockStatement(b) => {
                out.extend(self.stmts(cx, &b.body));
                Ok(())
            }
            Statement::VariableDeclaration(d) => self.var_decl(cx, d, out),
            Statement::ReturnStatement(r) => {
                let v = match &r.argument {
                    Some(a) => {
                        let v = self.expr(cx, a)?;
                        Some(self.number_of(v)?)
                    }
                    None if cx.func.result.is_some() => {
                        return Err(self.unsupported(
                            r.span,
                            "`return;` in a function that also returns a value (implicit `undefined`)",
                            "T08",
                        ));
                    }
                    None => None,
                };
                out.push(Stmt::Return(v));
                Ok(())
            }
            Statement::IfStatement(i) => {
                let c = self.expr(cx, &i.test);
                let then = self.stmt_body(cx, &i.consequent);
                let els = match &i.alternate {
                    Some(a) => self.stmt_body(cx, a),
                    None => Vec::new(),
                };
                let cond = self.cond_of(c?);
                out.push(Stmt::If { cond, then, els });
                Ok(())
            }
            Statement::WhileStatement(w) => self.while_loop(cx, None, w, out),
            Statement::DoWhileStatement(w) => self.do_while_loop(cx, None, w, out),
            Statement::ForStatement(f) => self.for_loop(cx, None, f, out),
            Statement::LabeledStatement(l) => {
                let name = Some(l.label.name.to_string());
                match &l.body {
                    Statement::WhileStatement(w) => self.while_loop(cx, name, w, out),
                    Statement::DoWhileStatement(w) => self.do_while_loop(cx, name, w, out),
                    Statement::ForStatement(f) => self.for_loop(cx, name, f, out),
                    body => {
                        let brk = cx.label();
                        cx.targets.push(Target {
                            js_label: name,
                            brk,
                            kind: TargetKind::Block,
                        });
                        let inner = self.stmt_body(cx, body);
                        cx.targets.pop();
                        out.push(Stmt::Block {
                            label: brk,
                            body: inner,
                        });
                        Ok(())
                    }
                }
            }
            Statement::BreakStatement(b) => {
                let name = b.label.as_ref().map(|l| l.name.as_str());
                let Some(t) = find_target(&cx.targets, name, false) else {
                    return Err(self.unsupported(
                        b.span,
                        "`break` outside a loop (e.g. in `switch`)",
                        "later",
                    ));
                };
                out.push(Stmt::Break(cx.targets[t].brk));
                Ok(())
            }
            Statement::ContinueStatement(c) => {
                let name = c.label.as_ref().map(|l| l.name.as_str());
                let t = find_target(&cx.targets, name, true)
                    .expect("semantic analysis checks continue targets");
                let jump = match cx.targets[t].kind {
                    TargetKind::While => Stmt::Continue(cx.targets[t].brk),
                    TargetKind::Wrapped(Some(l)) => Stmt::Break(l),
                    TargetKind::Wrapped(None) => {
                        let l = cx.label();
                        cx.targets[t].kind = TargetKind::Wrapped(Some(l));
                        Stmt::Break(l)
                    }
                    TargetKind::Block => unreachable!("find_target skips blocks for continue"),
                };
                out.push(jump);
                Ok(())
            }
            Statement::FunctionDeclaration(f) => {
                Err(self.unsupported(f.span, "nested function (closures)", "T14"))
            }
            other => Err(self.unsupported(
                other.span(),
                &format!("statement `{}`", statement_kind(other)),
                "later",
            )),
        }
    }

    fn var_decl(
        &mut self,
        cx: &mut FnCx,
        d: &'a VariableDeclaration<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if matches!(
            d.kind,
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing
        ) {
            return Err(self.unsupported(d.span, "`using` declaration", "not scheduled"));
        }
        let mut result = Ok(());
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(b) = &decl.id else {
                result = Err(self.unsupported(decl.span, "destructuring declaration", "T15"));
                continue;
            };
            let Some(init) = &decl.init else {
                result = Err(self.unsupported(
                    decl.span,
                    "declaration without an initializer (its value is `undefined`)",
                    "T08",
                ));
                continue;
            };
            let v = match self.expr(cx, init).and_then(|v| self.number_of(v)) {
                Ok(v) => v,
                Err(e) => {
                    result = Err(e);
                    continue;
                }
            };
            let sym = b
                .symbol_id
                .get()
                .expect("semantic analysis assigns symbols");
            let local = match cx.locals.get(&sym) {
                Some(l) => *l,
                None => {
                    let l = cx.func.add_local(b.name.as_str(), Type::F64);
                    cx.locals.insert(sym, l);
                    l
                }
            };
            out.push(Stmt::Set(local, v));
        }
        result
    }

    fn while_loop(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        w: &'a WhileStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let brk = cx.label();
        let test = self.expr(cx, &w.test);
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::While,
        });
        let body = self.stmt_body(cx, &w.body);
        cx.targets.pop();
        let test = self.cond_of(test?);
        let mut inner = exit_unless(test, brk);
        inner.extend(body);
        inner.push(Stmt::Continue(brk));
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        Ok(())
    }

    fn do_while_loop(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        w: &'a DoWhileStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let brk = cx.label();
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::Wrapped(None),
        });
        let body = self.stmt_body(cx, &w.body);
        let t = cx.targets.pop().expect("pushed above");
        let test = self.expr(cx, &w.test);
        let mut inner = wrap_continue(t.kind, body);
        let test = self.cond_of(test?);
        inner.push(Stmt::If {
            cond: test,
            then: vec![Stmt::Continue(brk)],
            els: vec![],
        });
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        Ok(())
    }

    fn for_loop(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        f: &'a ForStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let mut result = Ok(());
        match &f.init {
            None => {}
            Some(ForStatementInit::VariableDeclaration(d)) => result = self.var_decl(cx, d, out),
            Some(init) => {
                let e = init
                    .as_expression()
                    .expect("non-declaration for-init is an expression");
                result = self.expr_stmt(cx, e, out);
            }
        }
        let brk = cx.label();
        let test = f.test.as_ref().map(|t| self.expr(cx, t));
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::Wrapped(None),
        });
        let body = self.stmt_body(cx, &f.body);
        let t = cx.targets.pop().expect("pushed above");
        let mut update = Vec::new();
        if let Some(u) = &f.update {
            result = result.and(self.expr_stmt(cx, u, &mut update));
        }
        let mut inner = match test {
            Some(t) => exit_unless(self.cond_of(t?), brk),
            None => Vec::new(),
        };
        inner.extend(wrap_continue(t.kind, body));
        inner.extend(update);
        inner.push(Stmt::Continue(brk));
        out.push(Stmt::Loop {
            label: brk,
            body: inner,
        });
        result
    }

    /// An expression whose value is discarded.
    fn expr_stmt(&mut self, cx: &mut FnCx, e: &'a Expression<'a>, out: &mut Vec<Stmt>) -> R<()> {
        match e {
            Expression::ParenthesizedExpression(p) => self.expr_stmt(cx, &p.expression, out),
            Expression::SequenceExpression(s) => {
                let mut result = Ok(());
                for x in &s.expressions {
                    result = result.and(self.expr_stmt(cx, x, out));
                }
                result
            }
            Expression::AssignmentExpression(a) => {
                let (place, value) = self.assignment(cx, a)?;
                out.push(set(place, value));
                Ok(())
            }
            Expression::UpdateExpression(u) => {
                let (place, value) = self.update(cx, u)?;
                out.push(set(place, value));
                Ok(())
            }
            Expression::CallExpression(c) => {
                let call = self.call(cx, c)?;
                out.push(Stmt::Expr(call));
                Ok(())
            }
            _ => {
                let v = self.expr(cx, e)?;
                out.push(Stmt::Expr(v.e));
                Ok(())
            }
        }
    }

    // ---- expressions -------------------------------------------------------------------

    fn expr(&mut self, cx: &mut FnCx, e: &'a Expression<'a>) -> R<Val> {
        let span = e.span();
        let num = |e: Expr| Val {
            e,
            ty: Type::F64,
            span,
        };
        let boolean = |e: Expr| Val {
            e,
            ty: Type::Bool,
            span,
        };
        match e {
            Expression::NumericLiteral(n) => Ok(num(Expr::f64(n.value))),
            Expression::BooleanLiteral(b) => Ok(boolean(Expr::bool(b.value))),
            Expression::ParenthesizedExpression(p) => {
                let v = self.expr(cx, &p.expression)?;
                Ok(Val { span, ..v })
            }
            Expression::Identifier(id) => match self.resolve(cx, id)? {
                Place::Local(l) => Ok(num(Expr::Local(l))),
                Place::Global(g) => Ok(num(Expr::Global(g))),
            },
            Expression::BinaryExpression(b) => self.binary(cx, b),
            Expression::LogicalExpression(l) => self.logical(cx, l),
            Expression::UnaryExpression(u) => {
                let v = self.expr(cx, &u.argument)?;
                match u.operator {
                    UnaryOperator::UnaryNegation => {
                        Ok(num(Expr::unary(UnOp::Neg, Type::F64, self.number_of(v)?)))
                    }
                    UnaryOperator::UnaryPlus => Ok(num(self.number_of(v)?)),
                    UnaryOperator::LogicalNot => {
                        Ok(boolean(Expr::unary(UnOp::Not, Type::Bool, self.cond_of(v))))
                    }
                    UnaryOperator::BitwiseNot => {
                        let x = to_i32(self.number_of(v)?);
                        Ok(num(from_i32(Expr::unary(UnOp::BitNot, Type::I32, x))))
                    }
                    op => {
                        Err(self.unsupported(span, &format!("operator `{}`", op.as_str()), "T08"))
                    }
                }
            }
            Expression::ConditionalExpression(c) => {
                let test = self.expr(cx, &c.test);
                let then = self.expr(cx, &c.consequent);
                let els = self.expr(cx, &c.alternate);
                let (test, then, els) = (test?, then?, els?);
                let cond = Box::new(self.cond_of(test));
                if then.ty == Type::Bool || els.ty == Type::Bool {
                    // Only usable as a condition, so truthiness is all that matters.
                    let (then, els) = (self.cond_of(then), self.cond_of(els));
                    Ok(boolean(Expr::Cond {
                        cond,
                        then: Box::new(then),
                        els: Box::new(els),
                        ty: Type::Bool,
                    }))
                } else {
                    Ok(num(Expr::Cond {
                        cond,
                        then: Box::new(then.e),
                        els: Box::new(els.e),
                        ty: Type::F64,
                    }))
                }
            }
            Expression::AssignmentExpression(a) => {
                let (place, value) = self.assignment(cx, a)?;
                match place {
                    Place::Local(l) => Ok(num(Expr::Tee(l, Box::new(value)))),
                    Place::Global(_) => Err(self.unsupported(
                        span,
                        "assignment to a module variable used as a value",
                        "T08",
                    )),
                }
            }
            Expression::UpdateExpression(u) => {
                if !u.prefix {
                    return Err(self.unsupported(span, "postfix `++`/`--` used as a value", "T08"));
                }
                let (place, value) = self.update(cx, u)?;
                match place {
                    Place::Local(l) => Ok(num(Expr::Tee(l, Box::new(value)))),
                    Place::Global(_) => Err(self.unsupported(
                        span,
                        "update of a module variable used as a value",
                        "T08",
                    )),
                }
            }
            Expression::CallExpression(c) => {
                let call = self.call(cx, c)?;
                let Expr::Call { func, .. } = &call else {
                    unreachable!()
                };
                if self.module.function(*func).result.is_none() {
                    return Err(self.unsupported(
                        span,
                        "using the result of a function that returns nothing (`undefined`)",
                        "T08",
                    ));
                }
                Ok(num(call))
            }
            Expression::StringLiteral(_) | Expression::TemplateLiteral(_) => {
                Err(self.unsupported(span, "string", "T09"))
            }
            Expression::NullLiteral(_) => Err(self.unsupported(span, "`null`", "T08")),
            Expression::BigIntLiteral(_) => Err(self.unsupported(span, "BigInt", "not scheduled")),
            Expression::SequenceExpression(_) => {
                Err(self.unsupported(span, "comma operator used as a value", "later"))
            }
            Expression::ObjectExpression(_) => Err(self.unsupported(span, "object literal", "T13")),
            Expression::ArrayExpression(_) => Err(self.unsupported(span, "array literal", "T15")),
            Expression::FunctionExpression(_) | Expression::ArrowFunctionExpression(_) => {
                Err(self.unsupported(span, "function expression (closures)", "T14"))
            }
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
                Err(self.unsupported(span, "property access", "T13"))
            }
            other => Err(self.unsupported(
                span,
                &format!("expression `{}`", expression_kind(other)),
                "later",
            )),
        }
    }

    fn binary(&mut self, cx: &mut FnCx, b: &'a BinaryExpression<'a>) -> R<Val> {
        let span = b.span;
        let l = self.expr(cx, &b.left);
        let r = self.expr(cx, &b.right);
        let (l, r) = (l?, r?);
        use BinaryOperator as B;
        let arith = |op| Some((op, false));
        let cmp = |op| Some((op, true));
        let bitwise = match b.operator {
            B::BitwiseOR => Some(BinOp::Or),
            B::BitwiseAnd => Some(BinOp::And),
            B::BitwiseXOR => Some(BinOp::Xor),
            B::ShiftLeft => Some(BinOp::Shl),
            B::ShiftRight => Some(BinOp::Shr),
            _ => None,
        };
        if let Some(op) = bitwise {
            let (l, r) = (self.number_of(l)?, self.number_of(r)?);
            let e = from_i32(Expr::binary(op, Type::I32, to_i32(l), to_i32(r)));
            return Ok(Val {
                e,
                ty: Type::F64,
                span,
            });
        }
        let op = match b.operator {
            B::Addition => arith(BinOp::Add),
            B::Subtraction => arith(BinOp::Sub),
            B::Multiplication => arith(BinOp::Mul),
            B::Division => arith(BinOp::Div),
            B::Remainder => arith(BinOp::Rem),
            // With numbers only, loose and strict equality coincide.
            B::StrictEquality | B::Equality => cmp(BinOp::Eq),
            B::StrictInequality | B::Inequality => cmp(BinOp::Ne),
            B::LessThan => cmp(BinOp::Lt),
            B::LessEqualThan => cmp(BinOp::Le),
            B::GreaterThan => cmp(BinOp::Gt),
            B::GreaterEqualThan => cmp(BinOp::Ge),
            _ => None,
        };
        let Some((op, is_cmp)) = op else {
            let planned = match b.operator {
                B::ShiftRightZeroFill => "T08 (needs unsigned i32 → f64)",
                B::Exponential => "T21 (Math.pow)",
                _ => "T13",
            };
            return Err(self.unsupported(
                span,
                &format!("operator `{}`", b.operator.as_str()),
                planned,
            ));
        };
        let (l, r) = (self.number_of(l)?, self.number_of(r)?);
        let ty = if is_cmp { Type::Bool } else { Type::F64 };
        Ok(Val {
            e: Expr::binary(op, Type::F64, l, r),
            ty,
            span,
        })
    }

    fn logical(&mut self, cx: &mut FnCx, l: &'a LogicalExpression<'a>) -> R<Val> {
        let span = l.span;
        let a = self.expr(cx, &l.left);
        let b = self.expr(cx, &l.right);
        let (a, b) = (a?, b?);
        if l.operator == LogicalOperator::Coalesce {
            // A number is never null/undefined: `a ?? b` is `a` and `b` is not evaluated.
            return Ok(Val { span, ..a });
        }
        let and = l.operator == LogicalOperator::And;
        if a.ty == Type::Bool || b.ty == Type::Bool {
            // The result is only usable as a condition: combine truthiness.
            let (a, b) = (self.cond_of(a), self.cond_of(b));
            let (then, els) = if and {
                (b, Expr::bool(false))
            } else {
                (Expr::bool(true), b)
            };
            let e = Expr::Cond {
                cond: Box::new(a),
                then: Box::new(then),
                els: Box::new(els),
                ty: Type::Bool,
            };
            return Ok(Val {
                e,
                ty: Type::Bool,
                span,
            });
        }
        // Numbers: the result is one of the operands. Evaluate `a` once into a temp.
        let tmp = cx.func.add_local("tmp", Type::F64);
        let cond = truthy(Expr::Tee(tmp, Box::new(a.e)));
        let (then, els) = if and {
            (b.e, Expr::Local(tmp))
        } else {
            (Expr::Local(tmp), b.e)
        };
        let e = Expr::Cond {
            cond: Box::new(cond),
            then: Box::new(then),
            els: Box::new(els),
            ty: Type::F64,
        };
        Ok(Val {
            e,
            ty: Type::F64,
            span,
        })
    }

    fn call(&mut self, cx: &mut FnCx, c: &'a CallExpression<'a>) -> R<Expr> {
        let span = c.span;
        if c.optional {
            return Err(self.unsupported(span, "optional call `?.()`", "T14"));
        }
        let Expression::Identifier(id) = &c.callee else {
            return Err(self.unsupported(
                c.callee.span(),
                "call of something other than a named function",
                "T14",
            ));
        };
        let sym = self.scoping.get_reference(id.reference_id()).symbol_id();
        let Some(func) = sym.and_then(|s| self.funcs.get(&s)).copied() else {
            let what = match sym {
                None => format!("call of global `{}`", id.name),
                Some(_) => format!("call of `{}`, which is not a top-level function", id.name),
            };
            return Err(self.unsupported(span, &what, "T14/T21"));
        };
        let params = self.module.function(func).params as usize;
        if c.arguments.len() != params {
            return Err(self.unsupported(
                span,
                &format!(
                    "call with {} argument(s) to a function with {params} parameter(s)",
                    c.arguments.len()
                ),
                "T08",
            ));
        }
        let mut args = Vec::new();
        let mut result = Ok(());
        for a in &c.arguments {
            let Some(e) = a.as_expression() else {
                result = Err(self.unsupported(a.span(), "spread argument", "T15"));
                continue;
            };
            match self.expr(cx, e).and_then(|v| self.number_of(v)) {
                Ok(v) => args.push(v),
                Err(e) => result = Err(e),
            }
        }
        result?;
        Ok(Expr::Call { func, args })
    }

    fn assignment(&mut self, cx: &mut FnCx, a: &'a AssignmentExpression<'a>) -> R<(Place, Expr)> {
        let AssignmentTarget::AssignmentTargetIdentifier(id) = &a.left else {
            return Err(self.unsupported(
                a.left.span(),
                "assignment to something other than a variable",
                "T13/T15",
            ));
        };
        let place = self.resolve(cx, id)?;
        let rhs = self.expr(cx, &a.right).and_then(|v| self.number_of(v))?;
        use AssignmentOperator as A;
        let op = match a.operator {
            A::Assign => return Ok((place, rhs)),
            A::Addition => BinOp::Add,
            A::Subtraction => BinOp::Sub,
            A::Multiplication => BinOp::Mul,
            A::Division => BinOp::Div,
            A::Remainder => BinOp::Rem,
            A::BitwiseOR | A::BitwiseAnd | A::BitwiseXOR | A::ShiftLeft | A::ShiftRight => {
                let op = match a.operator {
                    A::BitwiseOR => BinOp::Or,
                    A::BitwiseAnd => BinOp::And,
                    A::BitwiseXOR => BinOp::Xor,
                    A::ShiftLeft => BinOp::Shl,
                    _ => BinOp::Shr,
                };
                let e = from_i32(Expr::binary(
                    op,
                    Type::I32,
                    to_i32(read(&place)),
                    to_i32(rhs),
                ));
                return Ok((place, e));
            }
            op => {
                return Err(self.unsupported(
                    a.span,
                    &format!("operator `{}`", op.as_str()),
                    "T08",
                ));
            }
        };
        let e = Expr::binary(op, Type::F64, read(&place), rhs);
        Ok((place, e))
    }

    fn update(&mut self, cx: &mut FnCx, u: &'a UpdateExpression<'a>) -> R<(Place, Expr)> {
        let SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &u.argument else {
            return Err(self.unsupported(
                u.argument.span(),
                "`++`/`--` on something other than a variable",
                "T13",
            ));
        };
        let place = self.resolve(cx, id)?;
        let op = if u.operator == UpdateOperator::Increment {
            BinOp::Add
        } else {
            BinOp::Sub
        };
        let e = Expr::binary(op, Type::F64, read(&place), Expr::f64(1.0));
        Ok((place, e))
    }

    fn resolve(&mut self, cx: &FnCx, id: &IdentifierReference<'a>) -> R<Place> {
        let span = id.span;
        let Some(sym) = self.scoping.get_reference(id.reference_id()).symbol_id() else {
            return Err(self.unsupported(span, &format!("global `{}`", id.name), "T21"));
        };
        if let Some(l) = cx.locals.get(&sym) {
            return Ok(Place::Local(*l));
        }
        if let Some(g) = self.globals.get(&sym) {
            return Ok(Place::Global(*g));
        }
        if self.funcs.contains_key(&sym) {
            return Err(self.unsupported(
                span,
                &format!("function `{}` used as a value", id.name),
                "T14",
            ));
        }
        Err(self.unsupported(
            span,
            &format!("`{}` used before its declaration", id.name),
            "T08",
        ))
    }

    // ---- conversions -------------------------------------------------------------------

    fn number_of(&mut self, v: Val) -> R<Expr> {
        match v.ty {
            Type::F64 => Ok(v.e),
            _ => Err(self.unsupported(v.span, "boolean used as a value", "T08")),
        }
    }

    fn cond_of(&mut self, v: Val) -> Expr {
        match v.ty {
            Type::Bool => v.e,
            _ => truthy(v.e),
        }
    }
}

/// JS ToBoolean of a number: false for ±0 and NaN.
fn truthy(e: Expr) -> Expr {
    Expr::binary(
        BinOp::Gt,
        Type::F64,
        Expr::unary(UnOp::Abs, Type::F64, e),
        Expr::f64(0.0),
    )
}

fn to_i32(e: Expr) -> Expr {
    Expr::Convert {
        from: Type::F64,
        to: Type::I32,
        arg: Box::new(e),
    }
}

fn from_i32(e: Expr) -> Expr {
    Expr::Convert {
        from: Type::I32,
        to: Type::F64,
        arg: Box::new(e),
    }
}

fn read(p: &Place) -> Expr {
    match p {
        Place::Local(l) => Expr::Local(*l),
        Place::Global(g) => Expr::Global(*g),
    }
}

fn set(p: Place, e: Expr) -> Stmt {
    match p {
        Place::Local(l) => Stmt::Set(l, e),
        Place::Global(g) => Stmt::SetGlobal(g, e),
    }
}

/// `if (!cond) break label`, with `!true` folded away for `while (true)`.
fn exit_unless(cond: Expr, label: LabelId) -> Vec<Stmt> {
    if cond == Expr::bool(true) {
        return Vec::new();
    }
    vec![Stmt::If {
        cond: Expr::unary(UnOp::Not, Type::Bool, cond),
        then: vec![Stmt::Break(label)],
        els: vec![],
    }]
}

/// Wrap a loop body in the `continue` block if any `continue` used it.
fn wrap_continue(kind: TargetKind, body: Vec<Stmt>) -> Vec<Stmt> {
    match kind {
        TargetKind::Wrapped(Some(label)) => vec![Stmt::Block { label, body }],
        _ => body,
    }
}

fn find_target(targets: &[Target], name: Option<&str>, is_continue: bool) -> Option<usize> {
    targets
        .iter()
        .rposition(|t| match name {
            Some(n) => t.js_label.as_deref() == Some(n),
            None => !matches!(t.kind, TargetKind::Block),
        })
        .filter(|&i| !is_continue || !matches!(targets[i].kind, TargetKind::Block))
}

/// A number literal, possibly negated: the only allowed global initializer for now.
fn literal_number(e: &Expression) -> Option<f64> {
    match e {
        Expression::NumericLiteral(n) => Some(n.value),
        Expression::ParenthesizedExpression(p) => literal_number(&p.expression),
        Expression::UnaryExpression(u) if u.operator == UnaryOperator::UnaryNegation => {
            literal_number(&u.argument).map(|v| -v)
        }
        _ => None,
    }
}

/// Does this statement contain `return <expr>` (not looking into nested functions)?
fn has_value_return(s: &Statement) -> bool {
    match s {
        Statement::ReturnStatement(r) => r.argument.is_some(),
        Statement::BlockStatement(b) => b.body.iter().any(has_value_return),
        Statement::IfStatement(i) => {
            has_value_return(&i.consequent) || i.alternate.as_ref().is_some_and(has_value_return)
        }
        Statement::WhileStatement(w) => has_value_return(&w.body),
        Statement::DoWhileStatement(w) => has_value_return(&w.body),
        Statement::ForStatement(f) => has_value_return(&f.body),
        Statement::ForInStatement(f) => has_value_return(&f.body),
        Statement::ForOfStatement(f) => has_value_return(&f.body),
        Statement::LabeledStatement(l) => has_value_return(&l.body),
        Statement::TryStatement(t) => {
            t.block.body.iter().any(has_value_return)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| h.body.body.iter().any(has_value_return))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|f| f.body.iter().any(has_value_return))
        }
        Statement::SwitchStatement(s) => s
            .cases
            .iter()
            .any(|c| c.consequent.iter().any(has_value_return)),
        _ => false,
    }
}

/// Conservative: true only if control provably can't fall off the end of `list`.
fn always_returns(list: &[Statement]) -> bool {
    list.iter().any(|s| match s {
        Statement::ReturnStatement(_) => true,
        Statement::BlockStatement(b) => always_returns(&b.body),
        Statement::IfStatement(i) => i.alternate.as_ref().is_some_and(|a| {
            always_returns(std::slice::from_ref(&i.consequent))
                && always_returns(std::slice::from_ref(a))
        }),
        _ => false,
    })
}

fn expression_kind(e: &Expression) -> String {
    let dbg = format!("{e:?}");
    dbg.split(['(', ' ', '{'])
        .next()
        .unwrap_or("Expression")
        .to_string()
}
