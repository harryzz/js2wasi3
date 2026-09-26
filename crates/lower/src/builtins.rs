//! Built-ins written in JS (T21, D5): the prelude.
//!
//! The prelude (`crates/lower/builtins/*.js`) is appended to the user's source and parsed with it
//! as one module ([`parse`]), so it goes through the same lowering, inference and backend as user
//! code. Its conventions:
//! - Top-level names start with `$$`. `$$Math_floor` implements `Math.floor`: the lowering maps
//!   a static member `A.b` of an undeclared global `A` to the prelude function `$$A_b`, if there is
//!   one. `$$parseInt` implements the global function `parseInt`. `$$String_prototype_slice` is
//!   `String.prototype.slice`: installed on the prototype (calls go through it) when the user's
//!   code names a property `slice` (`x.slice`, `x["slice"]`). Other `$$` names are helpers.
//! - Only function declarations at the top level, and variables with a literal initializer
//!   (state, e.g. `Math.random`'s): they become globals, in a command too.
//! - A call of an undeclared `__intrinsic_<name>(…)` is an IR operation (see
//!   `Lowerer::intrinsic_call`); only the prelude may use them.
//!
//! Only the prelude functions the user's code can reach are declared and lowered ([`Prelude`]), so a
//! program without built-ins compiles to the same IR as without the prelude.

use std::collections::{HashMap, HashSet};

use js2wasi3_frontend::{self as frontend, Allocator, Diagnostics, Program};
use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression, Statement};
use oxc_span::{GetSpan, Span};
use oxc_syntax::symbol::SymbolId;

/// The JS source of the built-ins.
pub const PRELUDE: &str = concat!(
    include_str!("../builtins/math.js"),
    include_str!("../builtins/string.js"),
);

/// The prelude's StringToNumber: the runtime's ToNumber of a string calls it.
pub(crate) const STRING_TO_NUMBER: &str = "$$StringToNumber";

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
    /// The names of the needed top-level declarations.
    live: HashSet<String>,
}

impl Prelude {
    /// `extra`: prelude names needed by what the lowering emits (not visible in the AST).
    pub fn new(p: &Program, extra: &[&str]) -> Prelude {
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
        let mut roots: Vec<usize> = extra
            .iter()
            .filter_map(|n| out.names.get(*n))
            .map(|s| item_of[s])
            .collect();
        // Prototype methods by property name: `slice` → `$$String_prototype_slice`, ….
        let mut methods: HashMap<String, Vec<usize>> = HashMap::new();
        for (name, sym) in &out.names {
            if let Some((_, m)) = name.split_once("_prototype_") {
                methods.entry(m.to_string()).or_default().push(item_of[sym]);
            }
        }
        let mut deps: HashMap<usize, Vec<usize>> = HashMap::new();
        let mut add = |target: usize, span: Span, roots: &mut Vec<usize>| {
            if !p.in_prelude(span.start) {
                roots.push(target);
                return;
            }
            // The prelude item containing it.
            let i = items.partition_point(|s| s.start <= span.start);
            if i > 0 && span.end <= items[i - 1].end {
                deps.entry(i - 1).or_default().push(target);
            }
        };
        for node in p.semantic.nodes().iter() {
            // A property name: the prototype methods it may read.
            let prop = match node.kind() {
                AstKind::StaticMemberExpression(m) => Some((m.property.name.as_str(), m.span)),
                AstKind::ComputedMemberExpression(m) => match &m.expression {
                    Expression::StringLiteral(k) => Some((k.value.as_str(), m.span)),
                    _ => None,
                },
                _ => None,
            };
            if let Some((name, span)) = prop {
                for &t in methods.get(name).into_iter().flatten() {
                    add(t, span, &mut roots);
                }
            }
            let (target, span) = match node.kind() {
                AstKind::IdentifierReference(r) => {
                    let Some(sym) = scoping.get_reference(r.reference_id()).symbol_id() else {
                        // A global function (`parseInt` → `$$parseInt`).
                        if let Some(sym) = out.names.get(&format!("$${}", r.name)) {
                            add(item_of[sym], r.span, &mut roots);
                        }
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
            add(target, span, &mut roots);
        }
        let mut live = vec![false; items.len()];
        while let Some(i) = roots.pop() {
            if !live[i] {
                live[i] = true;
                roots.extend(deps.get(&i).into_iter().flatten().copied());
            }
        }
        out.live = out
            .names
            .iter()
            .filter(|(_, s)| live[item_of[s]])
            .map(|(n, _)| n.clone())
            .collect();
        out.dead = items
            .iter()
            .zip(&live)
            .filter(|(_, l)| !**l)
            .map(|(s, _)| *s)
            .collect();
        out
    }

    /// Is the prelude declaration `name` needed (so lowered)?
    pub fn is_live(&self, name: &str) -> bool {
        self.live.contains(name)
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
        "$$String"
        | "$$Number"
        | "$$String_fromCharCode"
        | "$$String_fromCodePoint"
        | "$$String_prototype_concat"
        | "$$String_prototype_padStart"
        | "$$String_prototype_padEnd"
        | "$$String_prototype_indexOf"
        | "$$String_prototype_lastIndexOf"
        | "$$String_prototype_includes"
        | "$$String_prototype_startsWith"
        | "$$String_prototype_endsWith" => Some(1),
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
    global.then(|| match (id.name.as_str(), prop) {
        // The same function objects as the globals.
        ("Number", "parseFloat" | "parseInt") => format!("$${prop}"),
        _ => format!("$${}_{}", id.name, prop),
    })
}
