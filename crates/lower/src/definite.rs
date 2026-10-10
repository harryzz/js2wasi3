//! Definite assignment of `var`s (T103, F389): the uncaptured function-local `var`s declared
//! without a value whose every read comes after a write on every path. For those, `var x;` needn't
//! store `undefined`, so the type inference (see the crate doc) can keep them `f64`.
//!
//! One forward pass per function over its statements with the set of variables assigned so far
//! (`None`: unreachable, after `return`/`break`/`continue`/`throw`). Only plain `x = e` and
//! `var x = e` count as writes; a read counts as safe when the set has its variable. Branches meet
//! by intersection; a loop, a `switch`, a non-loop label and a `try` give what held before their
//! bodies (plus their test or discriminant), since a jump can leave them from any point. A
//! construct the pass doesn't model checks every read in it against the set at its start and adds
//! nothing. A read the pass never reached (code it doesn't walk) isn't safe, so missing a case
//! only loses precision.

use std::collections::HashSet;

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_semantic::{Scoping, Semantic};
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;

use crate::closures::Analysis;

type Set = Option<HashSet<SymbolId>>;

/// The `var`s of [`Analysis::definite`].
pub fn analyze(semantic: &Semantic, an: &Analysis) -> HashSet<SymbolId> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let root = scoping.root_scope_id();
    let is_decl = |n| matches!(nodes.kind(n), AstKind::VariableDeclarator(_));
    let mut candidates = HashSet::new();
    let mut reads = Vec::new();
    for sym in scoping.symbol_ids() {
        if scoping.symbol_scope_id(sym) == root
            || !scoping.symbol_flags(sym).is_function_scoped_declaration()
            || an.captured.contains(&sym)
            || !is_decl(scoping.symbol_declaration(sym))
            || scoping
                .symbol_redeclarations(sym)
                .iter()
                .any(|r| !is_decl(r.declaration))
        {
            continue;
        }
        candidates.insert(sym);
        for r in scoping.get_resolved_references(sym) {
            if r.is_read() {
                reads.push((nodes.get_node(r.node_id()).span().start, sym));
            }
        }
    }
    reads.sort_unstable();
    let mut w = Walk {
        scoping,
        reads: &reads,
        safe: HashSet::new(),
    };
    for node in nodes.iter() {
        let body = match node.kind() {
            AstKind::Function(f) => f.body.as_deref(),
            AstKind::ArrowFunctionExpression(a) => a.body.as_function_body(),
            _ => None,
        };
        if let Some(b) = body {
            w.stmts(&b.statements, Some(HashSet::new()));
        }
    }
    let unsafe_syms: HashSet<SymbolId> = reads
        .iter()
        .filter(|(at, _)| !w.safe.contains(at))
        .map(|(_, s)| *s)
        .collect();
    candidates.retain(|s| !unsafe_syms.contains(s));
    candidates
}

struct Walk<'s> {
    scoping: &'s Scoping,
    /// The candidates' reads (start, variable), sorted.
    reads: &'s [(u32, SymbolId)],
    /// Starts of the reads seen with their variable assigned.
    safe: HashSet<u32>,
}

fn meet(a: Set, b: Set) -> Set {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => Some(a.intersection(&b).copied().collect()),
    }
}

impl Walk<'_> {
    /// Every read within `span` is checked against `d`; nothing is added.
    fn opaque(&mut self, span: Span, d: &Set) {
        let Some(d) = d else { return };
        let from = self.reads.partition_point(|(at, _)| *at < span.start);
        for (at, sym) in &self.reads[from..] {
            if *at >= span.end {
                break;
            }
            if d.contains(sym) {
                self.safe.insert(*at);
            }
        }
    }

    fn assigned(&self, id: &IdentifierReference) -> Option<SymbolId> {
        self.scoping.get_reference(id.reference_id()).symbol_id()
    }

    fn stmts(&mut self, ss: &[Statement], mut d: Set) -> Set {
        for s in ss {
            d = self.stmt(s, d);
        }
        d
    }

    fn stmt(&mut self, s: &Statement, d: Set) -> Set {
        d.as_ref()?;
        match s {
            Statement::BlockStatement(b) => self.stmts(&b.body, d),
            Statement::ExpressionStatement(e) => self.expr(&e.expression, d),
            Statement::VariableDeclaration(v) => self.var_decl(v, d),
            Statement::EmptyStatement(_) | Statement::FunctionDeclaration(_) => d,
            Statement::IfStatement(i) => {
                let d = self.expr(&i.test, d);
                let a = self.stmt(&i.consequent, d.clone());
                let b = match &i.alternate {
                    Some(s) => self.stmt(s, d),
                    None => d,
                };
                meet(a, b)
            }
            Statement::WhileStatement(w) => {
                let d = self.expr(&w.test, d);
                self.stmt(&w.body, d.clone());
                d
            }
            Statement::DoWhileStatement(w) => {
                self.stmt(&w.body, d.clone());
                self.expr(&w.test, d)
            }
            Statement::ForStatement(f) => {
                let d = match &f.init {
                    Some(ForStatementInit::VariableDeclaration(v)) => self.var_decl(v, d),
                    Some(init) => match init.as_expression() {
                        Some(e) => self.expr(e, d),
                        None => {
                            self.opaque(init.span(), &d);
                            d
                        }
                    },
                    None => d,
                };
                let d = match &f.test {
                    Some(t) => self.expr(t, d),
                    None => d,
                };
                self.stmt(&f.body, d.clone());
                if let Some(u) = &f.update {
                    self.expr(u, d.clone());
                }
                d
            }
            Statement::ForInStatement(f) => self.for_each(f.left.span(), &f.right, &f.body, d),
            Statement::ForOfStatement(f) => self.for_each(f.left.span(), &f.right, &f.body, d),
            Statement::LabeledStatement(l) => {
                let after = self.stmt(&l.body, d.clone());
                if matches!(
                    l.body,
                    Statement::WhileStatement(_)
                        | Statement::DoWhileStatement(_)
                        | Statement::ForStatement(_)
                        | Statement::ForInStatement(_)
                        | Statement::ForOfStatement(_)
                ) {
                    after
                } else {
                    d
                }
            }
            Statement::SwitchStatement(sw) => {
                let d = self.expr(&sw.discriminant, d);
                for c in &sw.cases {
                    if let Some(t) = &c.test {
                        self.expr(t, d.clone());
                    }
                    self.stmts(&c.consequent, d.clone());
                }
                d
            }
            Statement::TryStatement(t) => {
                let tried = self.stmts(&t.block.body, d.clone());
                let merged = match &t.handler {
                    Some(h) => {
                        if let Some(p) = &h.param {
                            self.opaque(p.span, &d);
                        }
                        let caught = self.stmts(&h.body.body, d.clone());
                        meet(tried, caught)
                    }
                    None => tried,
                };
                match &t.finalizer {
                    Some(f) => {
                        // Reached from anywhere in the block; the state after it, when it ends
                        // normally, is the block's or the handler's.
                        self.stmts(&f.body, d);
                        self.stmts(&f.body, merged)
                    }
                    None => merged,
                }
            }
            Statement::ReturnStatement(r) => {
                if let Some(e) = &r.argument {
                    self.expr(e, d);
                }
                None
            }
            Statement::ThrowStatement(t) => {
                self.expr(&t.argument, d);
                None
            }
            Statement::BreakStatement(_) | Statement::ContinueStatement(_) => None,
            _ => {
                self.opaque(s.span(), &d);
                d
            }
        }
    }

    fn for_each(&mut self, left: Span, right: &Expression, body: &Statement, d: Set) -> Set {
        let d = self.expr(right, d);
        self.opaque(left, &d);
        self.stmt(body, d.clone());
        d
    }

    fn var_decl(&mut self, v: &VariableDeclaration, mut d: Set) -> Set {
        for decl in &v.declarations {
            match (&decl.id, &decl.init) {
                (BindingPattern::BindingIdentifier(b), Some(init)) => {
                    d = self.expr(init, d);
                    if let (Some(set), Some(sym)) = (&mut d, b.symbol_id.get()) {
                        set.insert(sym);
                    }
                }
                (BindingPattern::BindingIdentifier(_), None) => {}
                _ => self.opaque(decl.span, &d),
            }
        }
        d
    }

    fn expr(&mut self, e: &Expression, d: Set) -> Set {
        d.as_ref()?;
        match e {
            Expression::Identifier(_)
            | Expression::NumericLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_) => {
                self.opaque(e.span(), &d);
                d
            }
            Expression::ParenthesizedExpression(p) => self.expr(&p.expression, d),
            Expression::SequenceExpression(s) => {
                let mut d = d;
                for e in &s.expressions {
                    d = self.expr(e, d);
                }
                d
            }
            Expression::AssignmentExpression(a)
                if a.operator == AssignmentOperator::Assign
                    && let AssignmentTarget::AssignmentTargetIdentifier(id) = &a.left =>
            {
                let mut d = self.expr(&a.right, d);
                if let (Some(set), Some(sym)) = (&mut d, self.assigned(id)) {
                    set.insert(sym);
                }
                d
            }
            Expression::BinaryExpression(b) => {
                let d = self.expr(&b.left, d);
                self.expr(&b.right, d)
            }
            Expression::UnaryExpression(u) => self.expr(&u.argument, d),
            Expression::LogicalExpression(l) => {
                let d = self.expr(&l.left, d);
                self.expr(&l.right, d.clone());
                d
            }
            Expression::ConditionalExpression(c) => {
                let d = self.expr(&c.test, d);
                let a = self.expr(&c.consequent, d.clone());
                let b = self.expr(&c.alternate, d);
                meet(a, b)
            }
            Expression::CallExpression(c) if !c.optional => {
                let mut d = self.expr(&c.callee, d);
                for a in &c.arguments {
                    d = match a.as_expression() {
                        Some(e) => self.expr(e, d),
                        None => {
                            self.opaque(a.span(), &d);
                            d
                        }
                    };
                }
                d
            }
            _ => {
                self.opaque(e.span(), &d);
                d
            }
        }
    }
}
