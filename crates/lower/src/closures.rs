//! Closure analysis (T14): which functions use `this`, what each nested function captures, which
//! variables live in cells, and which of those need TDZ checks.
//!
//! A *function* here is a function scope: a declaration, a function expression, an arrow function or
//! an object-literal method. `None` stands for the module (a command's `main`). Every variable belongs to
//! the function whose scope declares it. Variables of the module's root scope are globals (or top-level
//! functions) and are never captured.
//!
//! A reference from another function captures the variable in that function and in every function
//! between it and the declaring one (flat closures). Captured variables live in cells, except two kinds
//! that never change and are captured by value: `this`, and the name of a named function expression
//! (inside it, the function itself).

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_semantic::{Scoping, Semantic};
use oxc_span::GetSpan;
use oxc_syntax::scope::ScopeId;
use oxc_syntax::symbol::SymbolId;

/// One environment entry of a closure.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Cap {
    /// The enclosing function's `this`, by value.
    This,
    /// A variable's cell.
    Cell(SymbolId),
    /// A value that never changes (a named function expression's own name).
    Value(SymbolId),
}

#[derive(Default)]
pub struct Analysis {
    /// Each function scope's enclosing function (`None`: the module).
    pub parent: HashMap<ScopeId, Option<ScopeId>>,
    pub arrows: HashSet<ScopeId>,
    /// Non-arrow functions that use `this` (themselves or in nested arrow functions).
    pub uses_this: HashSet<ScopeId>,
    /// What each function captures, in environment order.
    pub captures: HashMap<ScopeId, Vec<Cap>>,
    /// Variables that live in cells.
    pub captured: HashSet<SymbolId>,
    /// Captured `let`/`const` that a closure made before the declaration ends can read: their cell is
    /// created uninitialized at the start of their block and closures check it.
    pub tdz: HashSet<SymbolId>,
    /// Named function expressions' names: the function they name.
    pub self_names: HashMap<SymbolId, ScopeId>,
    /// Functions that refer to themselves by their own (function expression) name.
    pub self_ref: HashSet<ScopeId>,
    /// Function declarations referenced other than as the callee of a call.
    pub fn_values: HashSet<SymbolId>,
}

impl Analysis {
    /// The function containing `scope` (itself if it's a function scope).
    pub fn fn_of(&self, scoping: &Scoping, scope: ScopeId) -> Option<ScopeId> {
        scoping
            .scope_ancestors(scope)
            .find(|s| scoping.scope_flags(*s).is_function())
    }

    pub fn is_arrow(&self, f: ScopeId) -> bool {
        self.arrows.contains(&f)
    }
}

pub fn analyze(semantic: &Semantic) -> Analysis {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let root = scoping.root_scope_id();
    let mut an = Analysis::default();
    // Where a closure is made (for the TDZ rule): an expression where it is evaluated, a declaration
    // at the start of its block (hoisting).
    let mut made_at: HashMap<ScopeId, u32> = HashMap::new();
    for node in nodes.iter() {
        let (scope, arrow) = match node.kind() {
            AstKind::Function(f) => {
                let Some(s) = f.scope_id.get() else { continue };
                if f.r#type == FunctionType::FunctionExpression
                    && let Some(id) = &f.id
                    && let Some(sym) = id.symbol_id.get()
                {
                    an.self_names.insert(sym, s);
                }
                let at = if f.is_declaration() {
                    let block = nodes.parent_id(node.id());
                    nodes.get_node(block).span().start
                } else {
                    f.span.start
                };
                made_at.insert(s, at);
                (s, false)
            }
            AstKind::ArrowFunctionExpression(a) => {
                let Some(s) = a.scope_id.get() else { continue };
                made_at.insert(s, a.span.start);
                (s, true)
            }
            _ => continue,
        };
        if arrow {
            an.arrows.insert(scope);
        }
        let parent = scoping
            .scope_parent_id(scope)
            .and_then(|p| an.fn_of(scoping, p));
        an.parent.insert(scope, parent);
    }

    let mut captures: HashMap<ScopeId, BTreeSet<Cap>> = HashMap::new();

    // `this`: arrows capture it from the nearest non-arrow function, which gets a `this` param.
    for node in nodes.iter() {
        if !matches!(node.kind(), AstKind::ThisExpression(_)) {
            continue;
        }
        let mut chain = Vec::new();
        let mut f = an.fn_of(scoping, node.scope_id());
        while let Some(s) = f
            && an.is_arrow(s)
        {
            chain.push(s);
            f = an.parent[&s];
        }
        // At the module level `this` is `undefined`: nothing to capture.
        if let Some(owner) = f {
            an.uses_this.insert(owner);
            for s in chain {
                captures.entry(s).or_default().insert(Cap::This);
            }
        }
    }

    for sym in scoping.symbol_ids() {
        let decl_scope = scoping.symbol_scope_id(sym);
        let self_of = an.self_names.get(&sym).copied();
        if decl_scope == root && self_of.is_none() {
            continue; // a global or a top-level function
        }
        let decl_fn = match self_of {
            Some(s) => Some(s),
            None => an.fn_of(scoping, decl_scope),
        };
        let is_fn_decl = matches!(
            nodes.kind(scoping.symbol_declaration(sym)),
            AstKind::Function(f) if f.is_declaration()
        );
        for r in scoping.get_resolved_references(sym) {
            let node = r.node_id();
            if is_fn_decl && !is_callee(semantic, node) {
                an.fn_values.insert(sym);
            }
            let from = an.fn_of(scoping, nodes.get_node(node).scope_id());
            if let Some(owner) = self_of {
                // Its value is the function's `self` param (passed on to nested closures).
                an.self_ref.insert(owner);
            }
            if from == decl_fn {
                continue;
            }
            let cap = if self_of.is_some() {
                Cap::Value(sym)
            } else {
                an.captured.insert(sym);
                Cap::Cell(sym)
            };
            // Every function from the reference up to (not including) the declaring one.
            let mut f = from;
            let mut outermost = None;
            while let Some(s) = f
                && Some(s) != decl_fn
            {
                captures.entry(s).or_default().insert(cap);
                outermost = Some(s);
                f = an.parent.get(&s).copied().flatten();
            }
            let flags = scoping.symbol_flags(sym);
            if flags.is_block_scoped()
                && let Some(o) = outermost
                && made_at.get(&o).copied().unwrap_or(0) < declaration_end(semantic, sym)
            {
                an.tdz.insert(sym);
            }
        }
    }
    an.captures = captures
        .into_iter()
        .map(|(s, c)| (s, c.into_iter().collect()))
        .collect();
    an
}

/// Is the identifier reference at `node` the callee of a (non-optional, non-`new`) call?
fn is_callee(semantic: &Semantic, node: oxc_semantic::NodeId) -> bool {
    let nodes = semantic.nodes();
    let AstKind::IdentifierReference(id) = nodes.kind(node) else {
        return false;
    };
    match nodes.parent_kind(node) {
        AstKind::CallExpression(c) => {
            !c.optional && matches!(&c.callee, Expression::Identifier(x) if x.span == id.span)
        }
        _ => false,
    }
}

/// Where the declaration of `sym` ends (its initializer included).
fn declaration_end(semantic: &Semantic, sym: SymbolId) -> u32 {
    let nodes = semantic.nodes();
    let decl = semantic.scoping().symbol_declaration(sym);
    nodes.get_node(decl).span().end
}
