//! `using` and `await using` declarations (T45, `docs/design.md` D6): rewritten in the source, as
//! TypeScript's downlevel emit does, before the linker parses the module.
//!
//! A statement list with a `using` declaration (a block, a function body, a class static block, a
//! `switch`'s cases, the module's top level) gets a disposal environment and a `try`:
//!
//! ```text
//! { const env = { stack: [], error: undefined, has: false }; try { …
//!     const x = v, r1 = add(env, x, false); …
//! } catch (e) { env.error = e; env.has = true; } finally { dispose(env); } }
//! ```
//!
//! so `break`, `continue`, `return` and `yield` leave through the existing `finally` lowering.
//! `add` checks the value (node's TypeErrors) and pushes `[value, method, async]`; `dispose` calls
//! the methods in reverse order, chaining errors in `SuppressedError`s, and throws the result. With
//! an `await using`, the `finally` holds the disposal loop itself, so its `await`s are the spec's
//! (DisposeResources: an `await undefined` for `null`/`undefined` resources). `for (using x of it)`
//! disposes at the end of each iteration: the loop binds `const x` and its body gets the
//! environment. The declaration stays a `const` of the original initializer, so function names are
//! inferred as before. The helpers are generated once per module, as plain JS.

use oxc_ast::AstKind;
use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};

use crate::eval::Region;
use crate::{Allocator, Diagnostics, Program, parse};

/// A statement list (or loop) that needs a disposal environment.
struct Scope {
    /// Where the environment and `try {` go, and where the rest goes.
    open: u32,
    close: u32,
    /// Wrap in a new block (a `switch` or `for` statement, a `for-of` body).
    braces: bool,
    is_async: bool,
    /// `for (using x of it)`: the loop's binding, added to the environment in the body.
    loop_binding: Option<String>,
}

/// Does the module declare anything with `using`?
pub(crate) fn has_using(p: &Program) -> bool {
    p.semantic
        .nodes()
        .iter()
        .any(|n| matches!(n.kind(), AstKind::VariableDeclaration(d) if d.kind.is_using()))
}

/// Rewrite the module's `using` declarations: the new program and the replaced spans. Or the
/// declarations this can't rewrite (`Ok`: at offsets in `p`), or a parse error of the rewrite.
#[allow(clippy::type_complexity)]
pub(crate) fn rewrite<'a>(
    alloc: &'a Allocator,
    path: &str,
    p: &Program<'a>,
) -> Result<(Program<'a>, Vec<Region>), Result<Vec<(Span, String)>, Diagnostics>> {
    let text = p.source;
    let nodes = p.semantic.nodes();
    let prefix = (0..)
        .map(|k| {
            if k == 0 {
                "using$".to_string()
            } else {
                format!("using{k}$")
            }
        })
        .find(|n| !text.contains(n.as_str()))
        .expect("a fresh prefix");
    let (add, dispose) = (format!("{prefix}add"), format!("{prefix}dispose"));
    // Each scope by the node that owns it, in the order found.
    let mut scopes: Vec<(oxc_semantic::NodeId, Scope)> = Vec::new();
    let mut edits: Vec<(u32, u32, String)> = Vec::new();
    let mut errors = Vec::new();
    let mut temps = 0;
    for node in nodes.iter() {
        let AstKind::VariableDeclaration(d) = node.kind() else {
            continue;
        };
        if !d.kind.is_using() {
            continue;
        }
        let is_async = d.kind == VariableDeclarationKind::AwaitUsing;
        let parent = nodes.parent_node(node.id());
        // The scope: the enclosing statement list, or the loop.
        let (owner, scope) = match parent.kind() {
            AstKind::ForOfStatement(f) => {
                let Some(BindingPattern::BindingIdentifier(id)) =
                    d.declarations.first().map(|x| &x.id)
                else {
                    continue;
                };
                // The loop's own `const` (its TDZ covers the head's expression).
                let first = d.declarations.first().map_or(d.span.end, |x| x.span.start);
                edits.push((d.span.start, first, "const ".to_string()));
                let body = f.body.span();
                (
                    parent.id(),
                    Scope {
                        open: body.start,
                        close: body.end,
                        braces: true,
                        is_async,
                        loop_binding: Some(id.name.to_string()),
                    },
                )
            }
            AstKind::ForStatement(f) => (
                parent.id(),
                Scope {
                    open: f.span.start,
                    close: f.span.end,
                    braces: true,
                    is_async,
                    loop_binding: None,
                },
            ),
            AstKind::BlockStatement(b) => (
                parent.id(),
                Scope {
                    open: b.span.start + 1,
                    close: b.span.end - 1,
                    braces: false,
                    is_async,
                    loop_binding: None,
                },
            ),
            AstKind::FunctionBody(b) => (
                parent.id(),
                Scope {
                    open: b.directives.last().map_or(b.span.start + 1, |d| d.span.end),
                    close: b.span.end - 1,
                    braces: false,
                    is_async,
                    loop_binding: None,
                },
            ),
            AstKind::StaticBlock(b) => {
                let brace = text[b.span.start as usize..].find('{').unwrap_or(0) as u32;
                (
                    parent.id(),
                    Scope {
                        open: b.span.start + brace + 1,
                        close: b.span.end - 1,
                        braces: false,
                        is_async,
                        loop_binding: None,
                    },
                )
            }
            AstKind::SwitchCase(_) => {
                let switch = nodes.parent_node(parent.id());
                let span = switch.kind().span();
                (
                    switch.id(),
                    Scope {
                        open: span.start,
                        close: span.end,
                        braces: true,
                        is_async,
                        loop_binding: None,
                    },
                )
            }
            AstKind::Program(prog) => {
                if prog.body.iter().any(|s| s.is_module_declaration()) {
                    errors.push((
                        d.span,
                        "unsupported: `using` at the top level of a module with imports or exports"
                            .to_string(),
                    ));
                    continue;
                }
                let first = prog.body.first().map_or(0, |s| s.span().start);
                let last = prog.body.last().map_or(0, |s| s.span().end);
                (
                    parent.id(),
                    Scope {
                        open: first,
                        close: last,
                        braces: true,
                        is_async,
                        loop_binding: None,
                    },
                )
            }
            _ => {
                errors.push((d.span, "unsupported: this `using` declaration".to_string()));
                continue;
            }
        };
        let k = match scopes.iter().position(|(o, _)| *o == owner) {
            Some(k) => {
                scopes[k].1.is_async |= is_async;
                k
            }
            None => {
                scopes.push((owner, scope));
                scopes.len() - 1
            }
        };
        if matches!(parent.kind(), AstKind::ForOfStatement(_)) {
            continue;
        }
        // `using a = A, b = B` → `const a = A, r1 = add(env, a, …), b = B, r2 = …`.
        let env = format!("{prefix}env{k}");
        let first = d.declarations.first().map_or(d.span.end, |x| x.span.start);
        edits.push((d.span.start, first, "const ".to_string()));
        for decl in &d.declarations {
            let BindingPattern::BindingIdentifier(id) = &decl.id else {
                continue;
            };
            temps += 1;
            edits.push((
                decl.span.end,
                decl.span.end,
                format!(", {prefix}r{temps} = {add}({env}, {}, {is_async})", id.name),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(Ok(errors));
    }
    for (k, (_, s)) in scopes.iter().enumerate() {
        let env = format!("{prefix}env{k}");
        let mut open = format!(
            "{}const {env} = {{ stack: [], error: undefined, has: false }}; try {{ ",
            if s.braces { "{ " } else { "" }
        );
        if let Some(name) = &s.loop_binding {
            open.push_str(&format!(
                "const {prefix}r0 = {add}({env}, {name}, {}); ",
                s.is_async
            ));
        }
        let finally = if s.is_async {
            disposal_loop(&env)
        } else {
            format!("{dispose}({env});")
        };
        let close = format!(
            "\n}} catch ({prefix}e) {{ {env}.error = {prefix}e; {env}.has = true; }} finally {{ {finally} }}{}",
            if s.braces { " }" } else { "" }
        );
        edits.push((s.open, s.open, open));
        edits.push((s.close, s.close, close));
    }
    edits.push((
        text.len() as u32,
        text.len() as u32,
        helpers(&add, &dispose),
    ));
    // At one offset: closing text, then opening text (in source order), then a replacement.
    edits.sort_by_key(|(start, end, t)| {
        let class = if end > start {
            2
        } else if t.starts_with('\n') {
            0
        } else {
            1
        };
        (*start, class)
    });
    let mut out = String::with_capacity(text.len() + 1024);
    let mut regions = Vec::new();
    let mut pos = 0;
    for (start, end, t) in edits {
        out.push_str(&text[pos..start as usize]);
        regions.push(Region::new(
            out.len() as u32,
            t.len() as u32,
            start,
            end - start,
        ));
        out.push_str(&t);
        pos = end as usize;
    }
    out.push_str(&text[pos..]);
    let out: &'a str = alloc.alloc_str(&out);
    let next = parse(alloc, path, out).map_err(|mut d| {
        // A rewrite the parser rejects is a compiler bug.
        for d in &mut d.0 {
            d.message = format!("internal: `using` rewrite: {}", d.message);
        }
        Err(d)
    })?;
    Ok((next, regions))
}

/// The `finally` of a scope with an `await using`: DisposeResources with its `await`s.
fn disposal_loop(env: &str) -> String {
    format!(
        "const s = {env}.stack; let n = false, w = false, h = {env}.has, r = {env}.error; \
         for (let i = s.length - 1; i >= 0; i--) {{ const [v, m, a] = s[i]; \
         if (!a && n && !w) {{ await undefined; n = false; }} \
         if (m !== undefined) {{ try {{ const x = m.call(v); if (a) {{ await x; w = true; }} }} \
         catch (e) {{ r = h ? new SuppressedError(e, r, \"An error was suppressed during disposal\") : e; h = true; }} }} \
         else n = true; }} \
         if (n && !w) await undefined; \
         if (h) throw r;"
    )
}

/// AddDisposableResource and the synchronous DisposeResources.
fn helpers(add: &str, dispose: &str) -> String {
    format!(
        "\nfunction {add}(env, v, a) {{ \
         if (v === null || v === undefined) {{ if (a) env.stack.push([v, undefined, true]); return; }} \
         if (typeof v !== \"object\" && typeof v !== \"function\") \
         throw new TypeError(\"An object is expected with `using` declarations\"); \
         let m; \
         if (a) {{ m = v[Symbol.asyncDispose]; \
         if (m === undefined) {{ const d = v[Symbol.dispose]; \
         if (typeof d !== \"function\") throw new TypeError(\"Symbol(Symbol.dispose) is not a function\"); \
         m = async function () {{ d.call(this); }}; }} }} \
         else m = v[Symbol.dispose]; \
         if (typeof m !== \"function\") throw new TypeError(a ? \"Symbol(Symbol.asyncDispose) is not a function\" : \"Symbol(Symbol.dispose) is not a function\"); \
         env.stack.push([v, m, a]); }}\n\
         function {dispose}(env) {{ const s = env.stack; let h = env.has, r = env.error; \
         for (let i = s.length - 1; i >= 0; i--) {{ const [v, m] = s[i]; \
         try {{ m.call(v); }} catch (e) {{ r = h ? new SuppressedError(e, r, \"An error was suppressed during disposal\") : e; h = true; }} }} \
         if (h) throw r; }}\n"
    )
}
