//! Built-ins written in JS (T21, D5): the prelude.
//!
//! The prelude (`crates/lower/builtins/*.js`) is appended to the user's source and parsed with it
//! as one module ([`parse`]), so it goes through the same lowering, inference and backend as user
//! code. Its conventions:
//! - Top-level names start with `$$`. `$$Math_floor` implements `Math.floor`: the lowering maps
//!   a static member `A.b` of an undeclared global `A` to the prelude function `$$A_b`, if there is
//!   one. Other `$$` names are helpers.
//! - Only function declarations at the top level, and variables with a literal initializer
//!   (state, e.g. `Math.random`'s): they become globals, in a command too.
//! - A call of an undeclared `__intrinsic_<name>(…)` is an IR operation (see
//!   `Lowerer::intrinsic_call`); only the prelude may use them.
//!
//! Only the prelude functions the user's code can reach are declared and lowered ([`Prelude`]), so a
//! program without built-ins compiles to the same IR as without the prelude.

use std::collections::HashMap;

use js2wasi3_frontend::{self as frontend, Allocator, Diagnostics, Program};
use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;

/// The JS source of the built-ins.
pub const PRELUDE: &str = concat!(include_str!("../builtins/math.js"));

/// Parse a user module followed by the built-ins prelude (what [`crate::lower`] expects).
pub fn parse<'a>(
    alloc: &'a Allocator,
    path: &str,
    source: &'a str,
) -> Result<Program<'a>, Diagnostics> {
    frontend::parse_with_prelude(alloc, path, source, PRELUDE)
}

/// What the lowering needs to know about the prelude of one program.
#[derive(Default)]
pub(crate) struct Prelude {
    /// Top-level prelude functions and variables by name.
    pub names: HashMap<String, SymbolId>,
    /// User references to prelude names (errors: they aren't JS globals).
    pub user_refs: Vec<(Span, String)>,
    /// The spans of the prelude functions not needed, in source order.
    dead: Vec<Span>,
}

impl Prelude {
    pub fn new(p: &Program) -> Prelude {
        let mut out = Prelude::default();
        // Top-level prelude declarations in source order: functions and (state) variables.
        let mut items: Vec<Span> = Vec::new();
        let mut item_of: HashMap<SymbolId, usize> = HashMap::new();
        for stmt in &p.ast.body {
            if !p.in_prelude(stmt.span().start) {
                continue;
            }
            let mut add = |name: &str, sym: Option<SymbolId>| {
                if let Some(sym) = sym {
                    item_of.insert(sym, items.len());
                    out.names.insert(name.to_string(), sym);
                }
            };
            match stmt {
                Statement::FunctionDeclaration(f) => {
                    if let Some(id) = &f.id {
                        add(&id.name, id.symbol_id.get());
                    }
                }
                Statement::VariableDeclaration(d) => {
                    for decl in &d.declarations {
                        if let BindingPattern::BindingIdentifier(b) = &decl.id {
                            add(&b.name, b.symbol_id.get());
                        }
                    }
                }
                _ => {}
            }
            items.push(stmt.span());
        }
        let scoping = p.semantic.scoping();
        let mut roots = Vec::new();
        let mut deps: HashMap<usize, Vec<usize>> = HashMap::new();
        for node in p.semantic.nodes().iter() {
            let (target, span) = match node.kind() {
                AstKind::IdentifierReference(r) => {
                    let Some(sym) = scoping.get_reference(r.reference_id()).symbol_id() else {
                        continue;
                    };
                    let Some(&item) = item_of.get(&sym) else {
                        continue;
                    };
                    if !p.in_prelude(r.span.start) {
                        out.user_refs.push((r.span, r.name.to_string()));
                        continue;
                    }
                    (item, r.span)
                }
                AstKind::StaticMemberExpression(m) => {
                    let Some(name) = static_name(p, &m.object, &m.property.name) else {
                        continue;
                    };
                    let Some(sym) = out.names.get(&name) else {
                        continue;
                    };
                    (item_of[sym], m.span)
                }
                _ => continue,
            };
            if !p.in_prelude(span.start) {
                roots.push(target);
                continue;
            }
            // The prelude item containing it.
            let i = items.partition_point(|s| s.start <= span.start);
            if i > 0 && span.end <= items[i - 1].end {
                deps.entry(i - 1).or_default().push(target);
            }
        }
        let mut live = vec![false; items.len()];
        while let Some(i) = roots.pop() {
            if !live[i] {
                live[i] = true;
                roots.extend(deps.get(&i).into_iter().flatten().copied());
            }
        }
        out.dead = items
            .iter()
            .zip(&live)
            .filter(|(_, l)| !**l)
            .map(|(s, _)| *s)
            .collect();
        out
    }

    /// Is this offset inside a prelude declaration that isn't needed (so not lowered)?
    pub fn is_dead(&self, offset: u32) -> bool {
        let i = self.dead.partition_point(|s| s.start <= offset);
        i > 0 && offset < self.dead[i - 1].end
    }
}

/// The `length` of a built-in whose parameter list can't say it (a rest parameter).
pub(crate) fn length(name: &str) -> Option<u32> {
    match name {
        "$$Math_max" | "$$Math_min" | "$$Math_hypot" => Some(2),
        _ => None,
    }
}

/// `A.b` with `A` an undeclared global: the prelude name `$$A_b` that would implement it.
pub(crate) fn static_name(p: &Program, object: &Expression, prop: &str) -> Option<String> {
    let Expression::Identifier(id) = object.without_parentheses() else {
        return None;
    };
    let global = p
        .semantic
        .scoping()
        .get_reference(id.reference_id())
        .symbol_id()
        .is_none();
    global.then(|| format!("$${}_{}", id.name, prop))
}
