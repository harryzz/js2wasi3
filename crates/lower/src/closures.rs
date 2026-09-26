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
//!
//! **Classes (T16)** are functions too, keyed by the class scope: the constructor's body (its
//! `Function` scope maps to the class scope, [`Analysis::ctor_class`]) and the instance fields'
//! initializers run in it (`self` is the class, `this` the instance). Methods are ordinary
//! (non-arrow) functions. Static field initializers belong to the function defining the class
//! (the lowering binds `this` to the class there). `super.x` needs the home object: a method (and
//! arrow functions in it) captures it by value ([`Cap::Home`]); a class computes it.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_semantic::{Scoping, Semantic};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeId;
use oxc_syntax::symbol::SymbolId;

/// One environment entry of a closure.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum Cap {
    /// The enclosing function's `this`, by value.
    This,
    /// The home object (for `super.x`) of the enclosing method, by value (T16).
    Home,
    /// A private name's symbol ([`Analysis::privates`] index), by value.
    Private(u32),
    /// The cell holding an instance private method (filled when the class is evaluated), for its
    /// constructor to install.
    PrivMethod(u32),
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
    /// Non-arrow functions whose body uses `arguments` (T25; not in nested arrow functions yet).
    pub uses_arguments: HashSet<ScopeId>,
    /// What each function captures, in environment order.
    pub captures: HashMap<ScopeId, Vec<Cap>>,
    /// Variables that live in cells.
    pub captured: HashSet<SymbolId>,
    /// Captured `let`/`const` that a closure made before the declaration ends can read: their cell is
    /// created uninitialized at the start of their block and closures check it.
    pub tdz: HashSet<SymbolId>,
    /// `let`/`const`/`class` declared in a `switch` case and referenced in a later case (T24): a
    /// jump to that case skips the declaration, so they're TDZ-checked cells (also in `tdz` and
    /// `captured`) and every read checks them.
    pub switch_tdz: HashSet<SymbolId>,
    /// Named function expressions' names: the function they name.
    pub self_names: HashMap<SymbolId, ScopeId>,
    /// Functions that refer to themselves by their own (function expression) name.
    pub self_ref: HashSet<ScopeId>,
    /// Function declarations referenced other than as the callee of a call.
    pub fn_values: HashSet<SymbolId>,
    /// Classes, by their scope (T16).
    pub classes: HashMap<ScopeId, ClassInfo>,
    /// An explicit constructor's `Function` scope: its class's scope.
    pub ctor_class: HashMap<ScopeId, ScopeId>,
    /// Methods and classes using `super.x` (themselves or in arrow functions).
    pub uses_home: HashSet<ScopeId>,
    /// Classes whose constructor uses `new.target`.
    pub uses_new_target: HashSet<ScopeId>,
    /// Constructs this analysis can't handle: rejected by the lowering (span, what).
    pub unsupported: Vec<(Span, &'static str)>,
    /// Private names (T16), one per `#x` declared in a class.
    pub privates: Vec<PrivateName>,
    /// Each `#x` reference (by its start): the private name it denotes.
    pub private_refs: HashMap<u32, u32>,
}

#[derive(Debug)]
pub struct PrivateName {
    pub class: ScopeId,
    /// With the `#`.
    pub name: String,
    pub is_static: bool,
    pub method: bool,
}

#[derive(Default, Debug)]
pub struct ClassInfo {
    /// The instance fields' initializers: `[start, end)` source ranges.
    pub fields: Vec<(u32, u32)>,
    /// The static fields' initializers.
    pub statics: Vec<(u32, u32)>,
    /// Has an `extends` clause.
    pub derived: bool,
}

impl Analysis {
    /// The function containing source position `pos` in `scope` (itself if it's a function
    /// scope; a class for its constructor and instance field initializers).
    pub fn fn_at(&self, scoping: &Scoping, scope: ScopeId, pos: u32) -> Option<ScopeId> {
        for s in scoping.scope_ancestors(scope) {
            if let Some(c) = self.classes.get(&s)
                && c.fields.iter().any(|(a, b)| *a <= pos && pos < *b)
            {
                return Some(s);
            }
            if scoping.scope_flags(s).is_function() {
                return Some(self.ctor_class.get(&s).copied().unwrap_or(s));
            }
        }
        None
    }

    /// Is `pos` inside a static field initializer of class `class`?
    pub fn in_static(&self, class: ScopeId, pos: u32) -> bool {
        self.classes
            .get(&class)
            .is_some_and(|c| c.statics.iter().any(|(a, b)| *a <= pos && pos < *b))
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
    // Classes first: `fn_at` needs their field initializers.
    for node in nodes.iter() {
        let AstKind::Class(c) = node.kind() else {
            continue;
        };
        let Some(s) = c.scope_id.get() else { continue };
        let mut info = ClassInfo {
            derived: c.heritage.is_some(),
            ..ClassInfo::default()
        };
        for el in &c.body.body {
            let private = match el {
                ClassElement::PropertyDefinition(p) => match &p.key {
                    PropertyKey::PrivateIdentifier(id) => Some((id, p.r#static, false)),
                    _ => None,
                },
                ClassElement::MethodDefinition(m) => match &m.key {
                    PropertyKey::PrivateIdentifier(id) => Some((id, m.r#static, true)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((id, is_static, method)) = private {
                let name = format!("#{}", id.name);
                // A getter and a setter share their name.
                if !an.privates.iter().any(|p| p.class == s && p.name == name) {
                    an.privates.push(PrivateName {
                        class: s,
                        name,
                        is_static,
                        method,
                    });
                }
            }
            match el {
                ClassElement::PropertyDefinition(p) => {
                    if let Some(v) = &p.value {
                        let r = (v.span().start, v.span().end);
                        if p.r#static {
                            info.statics.push(r);
                        } else {
                            info.fields.push(r);
                        }
                    }
                }
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    if let Some(fs) = m.value.scope_id.get() {
                        an.ctor_class.insert(fs, s);
                    }
                }
                _ => {}
            }
        }
        an.classes.insert(s, info);
    }
    // Where a closure is made (for the TDZ rule): an expression where it is evaluated, a declaration
    // at the start of its block (hoisting).
    let mut made_at: HashMap<ScopeId, u32> = HashMap::new();
    for node in nodes.iter() {
        let (scope, arrow, start) = match node.kind() {
            AstKind::Function(f) => {
                let Some(s) = f.scope_id.get() else { continue };
                if an.ctor_class.contains_key(&s) {
                    continue; // part of its class
                }
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
                (s, false, f.span.start)
            }
            AstKind::ArrowFunctionExpression(a) => {
                let Some(s) = a.scope_id.get() else { continue };
                made_at.insert(s, a.span.start);
                (s, true, a.span.start)
            }
            AstKind::Class(c) => {
                let Some(s) = c.scope_id.get() else { continue };
                if c.is_expression()
                    && let Some(id) = &c.id
                    && let Some(sym) = id.symbol_id.get()
                {
                    an.self_names.insert(sym, s);
                }
                made_at.insert(s, c.span.start);
                (s, false, c.span.start)
            }
            _ => continue,
        };
        if arrow {
            an.arrows.insert(scope);
        }
        let parent = scoping
            .scope_parent_id(scope)
            .and_then(|p| an.fn_at(scoping, p, start));
        an.parent.insert(scope, parent);
    }

    let mut captures: HashMap<ScopeId, BTreeSet<Cap>> = HashMap::new();

    // `arguments` (T25): the nearest function's, if it isn't an arrow function or a class.
    for node in nodes.iter() {
        let AstKind::IdentifierReference(r) = node.kind() else {
            continue;
        };
        if r.name != "arguments"
            || scoping
                .get_reference(r.reference_id())
                .symbol_id()
                .is_some()
        {
            continue;
        }
        let Some(f) = an.fn_at(scoping, node.scope_id(), r.span.start) else {
            continue; // the module level: an unknown global
        };
        if an.is_arrow(f) {
            an.unsupported
                .push((r.span, "`arguments` in an arrow function"));
        } else if an.classes.contains_key(&f) {
            an.unsupported
                .push((r.span, "`arguments` in a class constructor or field"));
        } else {
            an.uses_arguments.insert(f);
        }
    }

    // `this`, `super` and `new.target`: arrow functions capture them from the nearest non-arrow
    // function (a `this` param, a home object, a `new.target` param).
    for node in nodes.iter() {
        let which = match node.kind() {
            AstKind::ThisExpression(_) => 0,
            AstKind::Super(_) => 1,
            AstKind::NewTarget(_) => 2,
            _ => continue,
        };
        let span = node.span();
        let mut chain = Vec::new();
        let mut f = an.fn_at(scoping, node.scope_id(), span.start);
        while let Some(s) = f
            && an.is_arrow(s)
        {
            chain.push(s);
            f = an.parent[&s];
        }
        let super_call = which == 1
            && matches!(nodes.parent_kind(node.id()), AstKind::CallExpression(c)
                if matches!(c.callee, Expression::Super(_)));
        // A static field initializer: `this` and the home object are the class (the lowering
        // binds them there).
        let (probe_scope, probe_pos) = match chain.last() {
            Some(a) => (scoping.scope_parent_id(*a).unwrap_or(root), made_at[a]),
            None => (node.scope_id(), span.start),
        };
        if static_class(&an, scoping, probe_scope, probe_pos).is_some() && which != 2 {
            if !chain.is_empty() {
                an.unsupported.push((
                    span,
                    "`this` or `super` in an arrow function in a static field initializer",
                ));
            }
            continue;
        }
        if which == 2 || super_call {
            if !chain.is_empty() {
                an.unsupported.push((
                    span,
                    if which == 2 {
                        "`new.target` in an arrow function"
                    } else {
                        "`super()` in an arrow function"
                    },
                ));
                continue;
            }
            if which == 2 {
                match f {
                    Some(s) if an.classes.contains_key(&s) => {
                        an.uses_new_target.insert(s);
                    }
                    _ => an
                        .unsupported
                        .push((span, "`new.target` outside a class constructor")),
                }
            }
            continue;
        }
        // At the module level `this` is `undefined`: nothing to capture.
        let Some(owner) = f else { continue };
        // `super.x` reads with `this` as the receiver: it uses both.
        an.uses_this.insert(owner);
        let mut caps = vec![Cap::This];
        if which == 1 {
            an.uses_home.insert(owner);
            if !an.classes.contains_key(&owner) {
                // A method: its home object is captured where the method is made.
                captures.entry(owner).or_default().insert(Cap::Home);
            }
            caps.push(Cap::Home);
        }
        for s in chain {
            captures.entry(s).or_default().extend(caps.iter().copied());
        }
    }

    // Private names: each reference captures its class's symbol from where the class is
    // evaluated; a class's constructor captures the symbols of its instance private members and
    // the cells of its instance private methods.
    for (i, p) in an.privates.iter().enumerate() {
        if !p.is_static {
            let e = captures.entry(p.class).or_default();
            e.insert(Cap::Private(i as u32));
            if p.method {
                e.insert(Cap::PrivMethod(i as u32));
            }
        }
    }
    for node in nodes.iter() {
        let (id, span) = match node.kind() {
            AstKind::PrivateFieldExpression(f) => (&f.field, f.span),
            AstKind::PrivateInExpression(p) => (&p.left, p.span),
            _ => continue,
        };
        let name = format!("#{}", id.name);
        // The innermost enclosing class declaring it.
        let class = nodes.ancestors(node.id()).find_map(|a| match a.kind() {
            AstKind::Class(c) => c
                .scope_id
                .get()
                .filter(|s| an.privates.iter().any(|p| p.class == *s && p.name == name)),
            _ => None,
        });
        let Some(class) = class else { continue };
        let i = an
            .privates
            .iter()
            .position(|p| p.class == class && p.name == name)
            .expect("found above") as u32;
        an.private_refs.insert(id.span.start, i);
        let decl_fn = an.parent.get(&class).copied().flatten();
        let mut f = an.fn_at(scoping, node.scope_id(), span.start);
        while let Some(s) = f
            && Some(s) != decl_fn
        {
            captures.entry(s).or_default().insert(Cap::Private(i));
            f = an.parent.get(&s).copied().flatten();
        }
    }

    for sym in scoping.symbol_ids() {
        let decl_scope = scoping.symbol_scope_id(sym);
        let self_of = an.self_names.get(&sym).copied();
        if decl_scope == root && self_of.is_none() {
            continue; // a global or a top-level function
        }
        let decl_node = scoping.symbol_declaration(sym);
        let decl_fn = match self_of {
            Some(s) => Some(s),
            None => an.fn_at(scoping, decl_scope, nodes.get_node(decl_node).span().start),
        };
        let is_fn_decl = matches!(
            nodes.kind(decl_node),
            AstKind::Function(f) if f.is_declaration()
        );
        for r in scoping.get_resolved_references(sym) {
            let node = r.node_id();
            if is_fn_decl && !is_callee(semantic, node) {
                an.fn_values.insert(sym);
            }
            let n = nodes.get_node(node);
            let from = an.fn_at(scoping, n.scope_id(), n.span().start);
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
            if (flags.is_block_scoped() || flags.is_class())
                && let Some(o) = outermost
                && made_at.get(&o).copied().unwrap_or(0) < declaration_end(semantic, sym)
            {
                an.tdz.insert(sym);
            }
        }
    }
    for node in nodes.iter() {
        let AstKind::SwitchStatement(sw) = node.kind() else {
            continue;
        };
        for (i, case) in sw.cases.iter().enumerate() {
            let later = &sw.cases[i + 1..];
            for st in &case.consequent {
                let ids = match st {
                    Statement::VariableDeclaration(d) if d.kind.is_lexical() => d
                        .declarations
                        .iter()
                        .flat_map(|d| d.id.get_binding_identifiers())
                        .collect(),
                    Statement::ClassDeclaration(c) => c.id.iter().collect(),
                    _ => Vec::new(),
                };
                for sym in ids.into_iter().filter_map(|b| b.symbol_id.get()) {
                    let in_later = scoping.get_resolved_references(sym).any(|r| {
                        let at = nodes.get_node(r.node_id()).span().start;
                        later.iter().any(|c| c.span.start <= at && at < c.span.end)
                    });
                    if in_later {
                        an.switch_tdz.insert(sym);
                        an.tdz.insert(sym);
                        an.captured.insert(sym);
                    }
                }
            }
        }
    }
    an.captures = captures
        .into_iter()
        .map(|(s, c)| (s, c.into_iter().collect()))
        .collect();
    an
}

/// The class whose static field initializer contains `pos` in `scope`, if no function boundary
/// comes first.
fn static_class(an: &Analysis, scoping: &Scoping, scope: ScopeId, pos: u32) -> Option<ScopeId> {
    for s in scoping.scope_ancestors(scope) {
        if an.in_static(s, pos) {
            return Some(s);
        }
        if scoping.scope_flags(s).is_function()
            || an
                .classes
                .get(&s)
                .is_some_and(|c| c.fields.iter().any(|(a, b)| *a <= pos && pos < *b))
        {
            return None;
        }
    }
    None
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
