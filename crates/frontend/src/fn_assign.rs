//! Function declarations whose binding is assigned (T60: graceful-fs's `ReadStream = val`): the
//! lowering calls declared functions directly, so it can't assign to their bindings. Rewritten in
//! the source, before the linker parses the module, into the binding they are: a variable holding
//! the function.
//!
//! ```text
//! function f() { … f … }   …   f = g;
//! ```
//! becomes
//! ```text
//! var f = f$fn; Object.defineProperty(f$fn, "name", { value: "f", configurable: true });
//! function f$fn() { … f … }   …   f = g;
//! ```
//! The `var` goes after the directives of the function body or script that has the declaration,
//! so it holds the (hoisted) function when that scope's code starts, as the declaration's binding
//! does; the body's references to `f` are the variable's. Only declarations directly in a function
//! body or at the top level, not exported ones (those stay unsupported).

use oxc_ast::AstKind;

use crate::eval::Region;
use crate::{Allocator, Diagnostics, Program, parse};

/// The declarations to rewrite: (insertion point, declaration name's span, name).
fn find(p: &Program) -> Vec<(u32, oxc_span::Span, String)> {
    let scoping = p.semantic.scoping();
    let nodes = p.semantic.nodes();
    let mut out = Vec::new();
    for node in nodes.iter() {
        let AstKind::Function(f) = node.kind() else {
            continue;
        };
        if !f.is_declaration() {
            continue;
        }
        let Some(id) = f.id.as_ref() else {
            continue;
        };
        let Some(sym) = id.symbol_id.get() else {
            continue;
        };
        if !scoping.get_resolved_references(sym).any(|r| r.is_write()) {
            continue;
        }
        let at = match nodes.parent_kind(node.id()) {
            AstKind::FunctionBody(b) => b.directives.last().map_or(b.span.start + 1, |d| d.span.end),
            AstKind::Program(prog) => prog.directives.last().map_or(prog.span.start, |d| d.span.end),
            _ => continue,
        };
        out.push((at, id.span, id.name.to_string()));
    }
    out
}

/// Does the module assign to a function declaration's binding?
pub(crate) fn has_assigned(p: &Program) -> bool {
    !find(p).is_empty()
}

/// Rewrite the module's assigned function declarations: the new program and the replaced spans.
pub(crate) fn rewrite<'a>(
    alloc: &'a Allocator,
    path: &str,
    p: &Program<'a>,
) -> Result<(Program<'a>, Vec<Region>), Diagnostics> {
    let text = p.source;
    let suffix = (0..)
        .map(|k| if k == 0 { "$fn".to_string() } else { format!("$fn{k}") })
        .find(|s| !text.contains(s.as_str()))
        .expect("a fresh suffix");
    // (start, end, text): insertions (start == end) and renamed declarations, in source order.
    let mut edits: Vec<(u32, u32, String)> = Vec::new();
    for (at, span, name) in find(p) {
        let renamed = format!("{name}{suffix}");
        let line = format!(
            "\nvar {name} = {renamed}; Object.defineProperty({renamed}, \"name\", {{ value: \"{name}\", configurable: true }});\n"
        );
        match edits.iter_mut().find(|e| e.0 == at && e.1 == at) {
            Some(e) => e.2.push_str(&line),
            None => edits.push((at, at, line)),
        }
        edits.push((span.start, span.end, renamed));
    }
    edits.sort_by_key(|e| (e.0, e.1));
    let mut out = String::with_capacity(text.len() + 256);
    let mut regions = Vec::new();
    let mut pos = 0;
    for (start, end, t) in edits {
        out.push_str(&text[pos..start as usize]);
        regions.push(Region::new(out.len() as u32, t.len() as u32, start, end - start));
        out.push_str(&t);
        pos = end as usize;
    }
    out.push_str(&text[pos..]);
    let out: &'a str = alloc.alloc_str(&out);
    let next = parse(alloc, path, out).map_err(|mut d| {
        // A rewrite the parser rejects is a compiler bug.
        for d in &mut d.0 {
            d.message = format!("internal: function declaration rewrite: {}", d.message);
        }
        d
    })?;
    Ok((next, regions))
}
