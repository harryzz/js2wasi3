//! Arrays, iteration and destructuring (T15): array literals, spread, `for-of`, and binding /
//! assignment patterns with defaults and rest elements.
//!
//! One walker ([`Lowerer::pattern`]) handles both kinds of patterns (`BindingPattern` in
//! declarations, parameters, `catch` and `for` heads; `AssignmentTarget` in assignments and
//! `for` heads). Values are `jsval`s of kind `Any`.
//!
//! - Object patterns read each key with `GetProp`; the outermost one first checks for
//!   `undefined`/`null` with node's message ("Cannot destructure property 'a' of 'x' as it is
//!   undefined."). Nested ones fail with GetProp's message, like node. A rest element copies the
//!   own enumerable properties except the keys read before it (`ObjectRest`).
//! - Array patterns iterate (`IterOpen`/`IterStep`/`IterValue`): each element takes the next value or
//!   `undefined`; a rest element takes the rest (`IterRest`). The bindings run inside a `Try` that
//!   closes the iterator (ignoring errors from `return`) if one throws; otherwise an iterator that
//!   isn't done is closed at the end.
//! - `for (x of v)` is lowered like `try { loop } finally { close unless done }`, reusing the
//!   T12 `finally` machinery so `break`, `continue` to outer loops, `return` and throws close the
//!   iterator.

use js2wasi3_ir::{BinOp, Const, ErrorKind, Expr, Intrinsic, LocalId, Stmt, Type, UnOp};
use oxc_ast::ast::*;
use oxc_span::{GetSpan, Span};

use super::{Finally, FnCx, Kind, Lowerer, R, Slot, Target, TargetKind, Val, jsval_of, utf16};

/// A pattern of either kind.
#[derive(Clone, Copy)]
pub(crate) enum Pat<'a> {
    Bind(&'a BindingPattern<'a>),
    Target(&'a AssignmentTarget<'a>),
    MaybeDefault(&'a AssignmentTargetMaybeDefault<'a>),
    Ident(&'a IdentifierReference<'a>),
}

/// How a pattern stores its values.
#[derive(Clone, Copy)]
pub(crate) enum Mode {
    /// A declaration (`fresh`: `let`/`const`/parameters, a new binding each time it runs).
    Decl { fresh: bool },
    /// An assignment to existing variables and properties.
    Assign,
}

/// What node calls the destructured value in its TypeError.
#[derive(Clone)]
pub(crate) enum Src {
    /// Its source text.
    Text(String),
    /// Its value (`undefined` / `null`): parameters.
    Value,
}

enum Key<'a> {
    Static(Vec<u16>, String),
    Computed(&'a Expression<'a>),
}

impl<'p, 'a> Lowerer<'p, 'a> {
    /// Destructure `v` (a `jsval`) into `p`. `src`: for the outermost pattern, how node names the value.
    pub(crate) fn pattern(
        &mut self,
        cx: &mut FnCx,
        p: Pat<'a>,
        v: Expr,
        mode: Mode,
        src: Option<Src>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        match p {
            Pat::Bind(BindingPattern::BindingIdentifier(b)) => {
                let Mode::Decl { fresh } = mode else {
                    unreachable!("binding patterns only declare")
                };
                let sym = b
                    .symbol_id
                    .get()
                    .expect("semantic analysis assigns symbols");
                let slot = Slot::Var(sym);
                let ty = self.slot_ty(slot);
                let e = self.store(slot, ty, Val::js(v, Kind::Any));
                out.extend(self.declare_store(cx, sym, ty, e, fresh));
                Ok(())
            }
            Pat::Bind(BindingPattern::AssignmentPattern(ap)) => {
                self.with_default(cx, Pat::Bind(&ap.left), &ap.right, v, mode, out)
            }
            Pat::Bind(BindingPattern::ObjectPattern(op)) => {
                let mut props = Vec::new();
                for bp in &op.properties {
                    props.push((self.key(&bp.key, bp.computed)?, Pat::Bind(&bp.value), None));
                }
                let rest = op.rest.as_ref().map(|r| Pat::Bind(&r.argument));
                self.object_pattern(cx, props, rest, v, mode, src, out)
            }
            Pat::Bind(BindingPattern::ArrayPattern(ap)) => {
                let elems = ap
                    .elements
                    .iter()
                    .map(|e| e.as_ref().map(Pat::Bind))
                    .collect();
                let rest = ap.rest.as_ref().map(|r| Pat::Bind(&r.argument));
                self.array_pattern(cx, elems, rest, v, mode, src, out)
            }
            Pat::Target(t) => match t.as_assignment_target_pattern() {
                Some(AssignmentTargetPattern::ArrayAssignmentTarget(a)) => {
                    let elems = a
                        .elements
                        .iter()
                        .map(|e| e.as_ref().map(Pat::MaybeDefault))
                        .collect();
                    let rest = a.rest.as_ref().map(|r| Pat::Target(&r.target));
                    self.array_pattern(cx, elems, rest, v, mode, src, out)
                }
                Some(AssignmentTargetPattern::ObjectAssignmentTarget(o)) => {
                    let mut props = Vec::new();
                    for prop in &o.properties {
                        match prop {
                            AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(pi) => {
                                let name = pi.binding.name.as_str();
                                props.push((
                                    Key::Static(name.encode_utf16().collect(), name.to_string()),
                                    Pat::Ident(&pi.binding),
                                    pi.init.as_ref(),
                                ));
                            }
                            AssignmentTargetProperty::AssignmentTargetPropertyProperty(pp) => {
                                props.push((
                                    self.key(&pp.name, pp.computed)?,
                                    Pat::MaybeDefault(&pp.binding),
                                    None,
                                ));
                            }
                        }
                    }
                    let rest = o.rest.as_ref().map(|r| Pat::Target(&r.target));
                    self.object_pattern(cx, props, rest, v, mode, src, out)
                }
                None => {
                    let simple = t
                        .as_simple_assignment_target()
                        .expect("a simple target or a pattern");
                    self.assign_simple(cx, simple, v, out)
                }
            },
            Pat::MaybeDefault(m) => match m {
                AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => {
                    self.with_default(cx, Pat::Target(&d.binding), &d.init, v, mode, out)
                }
                _ => {
                    let t = m.as_assignment_target().expect("the other variants");
                    self.pattern(cx, Pat::Target(t), v, mode, src, out)
                }
            },
            Pat::Ident(id) => {
                let var = self.resolve(cx, id)?;
                self.check_assignable(&var, id.span)?;
                let e = self.store(Slot::Var(var.sym), var.ty, Val::js(v, Kind::Any));
                out.extend(self.write_var(cx, &var, e));
                Ok(())
            }
        }
    }

    fn key(&mut self, k: &'a PropertyKey<'a>, computed: bool) -> R<Key<'a>> {
        Ok(match k {
            PropertyKey::StaticIdentifier(id) => {
                Key::Static(id.name.encode_utf16().collect(), id.name.to_string())
            }
            PropertyKey::PrivateIdentifier(id) => {
                return Err(self.unsupported(id.span, "private name", "T16"));
            }
            k => {
                let e = k.as_expression().expect("the other property keys");
                match e {
                    Expression::StringLiteral(sl) if !computed => Key::Static(
                        utf16(sl.value.as_str(), sl.lone_surrogates),
                        sl.value.to_string(),
                    ),
                    _ => Key::Computed(e),
                }
            }
        })
    }

    /// `pat = def`: `def` (named after the target) when the value is `undefined`.
    fn with_default(
        &mut self,
        cx: &mut FnCx,
        pat: Pat<'a>,
        def: &'a Expression<'a>,
        v: Expr,
        mode: Mode,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let t = cx.func.add_local("value", Type::JsVal);
        out.push(Stmt::Set(t, v));
        let name = match pat {
            Pat::Bind(BindingPattern::BindingIdentifier(b)) => Some(b.name.as_str()),
            Pat::Ident(id) => Some(id.name.as_str()),
            Pat::Target(AssignmentTarget::AssignmentTargetIdentifier(id)) => Some(id.name.as_str()),
            _ => None,
        };
        let d = self.named(cx, def, name)?;
        let e = Expr::Cond {
            cond: Box::new(Expr::binary(
                BinOp::Eq,
                Type::JsVal,
                Expr::Local(t),
                Expr::undefined(),
            )),
            then: Box::new(jsval_of(d)),
            els: Box::new(Expr::Local(t)),
            ty: Type::JsVal,
        };
        self.pattern(cx, pat, e, mode, None, out)
    }

    fn assign_simple(
        &mut self,
        cx: &mut FnCx,
        t: &'a SimpleAssignmentTarget<'a>,
        v: Expr,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        match t {
            SimpleAssignmentTarget::AssignmentTargetIdentifier(id) => {
                self.pattern(cx, Pat::Ident(id), v, Mode::Assign, None, out)
            }
            _ => match t.as_member_expression() {
                Some(m) => {
                    let (obj, key) = self.member(cx, m)?;
                    out.push(Stmt::Expr(Expr::intrinsic(
                        Intrinsic::SetProp,
                        vec![obj, key, v],
                    )));
                    Ok(())
                }
                None => Err(self.unsupported(t.span(), "assignment target", "later")),
            },
        }
    }

    /// `if (t === undefined/null) throw TypeError(…)` with node's destructuring messages.
    fn check_destructure(&self, t: LocalId, first: Option<&str>, src: &Src, out: &mut Vec<Stmt>) {
        for (c, what) in [(Const::Undefined, "undefined"), (Const::Null, "null")] {
            let name = match src {
                Src::Text(s) => s.as_str(),
                Src::Value => what,
            };
            let msg = match first {
                Some(k) => {
                    format!("Cannot destructure property '{k}' of '{name}' as it is {what}.")
                }
                None => format!("Cannot destructure '{name}' as it is {what}."),
            };
            out.push(Stmt::If {
                cond: Expr::binary(BinOp::Eq, Type::JsVal, Expr::Local(t), Expr::Const(c)),
                then: vec![Stmt::Throw(Expr::intrinsic(
                    Intrinsic::NewError(ErrorKind::TypeError),
                    vec![Expr::str(&msg), Expr::undefined()],
                ))],
                els: vec![],
            });
        }
    }

    #[allow(clippy::too_many_arguments, clippy::type_complexity)]
    fn object_pattern(
        &mut self,
        cx: &mut FnCx,
        props: Vec<(Key<'a>, Pat<'a>, Option<&'a Expression<'a>>)>,
        rest: Option<Pat<'a>>,
        v: Expr,
        mode: Mode,
        src: Option<Src>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let t = cx.func.add_local("object", Type::JsVal);
        out.push(Stmt::Set(t, v));
        // The outermost pattern (and an empty one, which reads nothing) checks with node's message.
        let src = src.or_else(|| (props.is_empty() && rest.is_none()).then_some(Src::Value));
        if let Some(src) = &src {
            let first = match props.first() {
                Some((Key::Static(_, name), _, _)) => Some(name.as_str()),
                _ => None,
            };
            self.check_destructure(t, first, src, out);
        }
        let mut excluded = Vec::new();
        for (key, pat, def) in props {
            let k = match key {
                Key::Static(units, _) => Expr::Str(units),
                Key::Computed(e) => {
                    let kv = self.expr(cx, e)?;
                    let tk = cx.func.add_local("key", Type::JsVal);
                    out.push(Stmt::Set(tk, jsval_of(kv)));
                    Expr::Local(tk)
                }
            };
            if rest.is_some() {
                excluded.push(Some(k.clone()));
            }
            let value = Expr::intrinsic(Intrinsic::GetProp, vec![Expr::Local(t), k]);
            match def {
                Some(d) => self.with_default(cx, pat, d, value, mode, out)?,
                None => self.pattern(cx, pat, value, mode, None, out)?,
            }
        }
        if let Some(r) = rest {
            let value = Expr::intrinsic(
                Intrinsic::ObjectRest,
                vec![Expr::Local(t), Expr::Array(excluded)],
            );
            self.pattern(cx, r, value, mode, None, out)?;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn array_pattern(
        &mut self,
        cx: &mut FnCx,
        elems: Vec<Option<Pat<'a>>>,
        rest: Option<Pat<'a>>,
        v: Expr,
        mode: Mode,
        src: Option<Src>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        let what = match src {
            Some(Src::Text(s)) => s,
            _ => "object".to_string(),
        };
        let it = cx.func.add_local("iter", Type::JsVal);
        let done = cx.func.add_local("done", Type::Bool);
        out.push(Stmt::Set(
            it,
            Expr::intrinsic(
                Intrinsic::IterOpen,
                vec![v, Expr::str(&format!("{what} is not iterable"))],
            ),
        ));
        out.push(Stmt::Set(done, Expr::bool(false)));
        let mut body = Vec::new();
        for e in elems {
            let value = cx.func.add_local("element", Type::JsVal);
            body.push(Stmt::Set(value, Expr::undefined()));
            body.push(Stmt::If {
                cond: Expr::unary(UnOp::Not, Type::Bool, Expr::Local(done)),
                then: vec![
                    Stmt::Set(done, Expr::bool(true)),
                    Stmt::If {
                        cond: Expr::intrinsic(Intrinsic::IterStep, vec![Expr::Local(it)]),
                        then: vec![
                            Stmt::Set(done, Expr::bool(false)),
                            Stmt::Set(
                                value,
                                Expr::intrinsic(Intrinsic::IterValue, vec![Expr::Local(it)]),
                            ),
                        ],
                        els: vec![],
                    },
                ],
                els: vec![],
            });
            if let Some(p) = e {
                self.pattern(cx, p, Expr::Local(value), mode, None, &mut body)?;
            }
        }
        if let Some(r) = rest {
            let value = cx.func.add_local("rest", Type::JsVal);
            body.push(Stmt::Set(
                value,
                Expr::intrinsic(Intrinsic::IterRest, vec![Expr::Local(it)]),
            ));
            body.push(Stmt::Set(done, Expr::bool(true)));
            self.pattern(cx, r, Expr::Local(value), mode, None, &mut body)?;
        }
        let close = |throwing: bool| Stmt::If {
            cond: Expr::unary(UnOp::Not, Type::Bool, Expr::Local(done)),
            then: vec![Stmt::Expr(Expr::intrinsic(
                Intrinsic::IterClose,
                vec![Expr::Local(it), Expr::bool(throwing)],
            ))],
            els: vec![],
        };
        let exc = cx.func.add_local("exception", Type::JsVal);
        out.push(Stmt::Try {
            body,
            catch_local: Some(exc),
            catch_body: vec![close(true), Stmt::Throw(Expr::Local(exc))],
        });
        out.push(close(false));
        Ok(())
    }

    /// `[a, , ...b]`. `msg`: the TypeError text for a spread value that isn't iterable (`None`:
    /// "<source> is not iterable").
    pub(crate) fn array_literal(
        &mut self,
        cx: &mut FnCx,
        elements: &'a [ArrayExpressionElement<'a>],
        msg: Option<&str>,
        span: Span,
    ) -> R<Expr> {
        let mut prefix = Vec::new();
        let mut i = 0;
        let mut result = Ok(());
        while i < elements.len() {
            match &elements[i] {
                ArrayExpressionElement::SpreadElement(_) => break,
                ArrayExpressionElement::Elision(_) => prefix.push(None),
                el => {
                    let e = el.as_expression().expect("the other elements");
                    match self.expr(cx, e) {
                        Ok(v) => prefix.push(Some(jsval_of(v))),
                        Err(e) => result = Err(e),
                    }
                }
            }
            i += 1;
        }
        result?;
        if i == elements.len() {
            return Ok(Expr::Array(prefix));
        }
        let t = cx.func.add_local("array", Type::JsVal);
        let mut body = vec![Stmt::Set(t, Expr::Array(prefix))];
        for el in &elements[i..] {
            match el {
                ArrayExpressionElement::SpreadElement(sp) => {
                    let v = self.expr(cx, &sp.argument)?;
                    let text = match msg {
                        Some(m) => m.to_string(),
                        None => format!("{} is not iterable", self.callee_text(&sp.argument)),
                    };
                    body.push(Stmt::Expr(Expr::intrinsic(
                        Intrinsic::ArraySpread,
                        vec![Expr::Local(t), jsval_of(v), Expr::str(&text)],
                    )));
                }
                ArrayExpressionElement::Elision(_) => {
                    return Err(self.unsupported(
                        span,
                        "array hole after a spread element",
                        "later",
                    ));
                }
                el => {
                    let e = el.as_expression().expect("the other elements");
                    let v = self.expr(cx, e)?;
                    body.push(Stmt::Expr(Expr::intrinsic(
                        Intrinsic::ArrayPush,
                        vec![Expr::Local(t), jsval_of(v)],
                    )));
                }
            }
        }
        Ok(Expr::Seq {
            body,
            value: Box::new(Expr::Local(t)),
        })
    }

    /// The arguments of a call with spread elements, as a new array.
    pub(crate) fn spread_args(
        &mut self,
        cx: &mut FnCx,
        arguments: &'a [Argument<'a>],
        span: Span,
    ) -> R<Expr> {
        // Arguments and array elements share their expression variants.
        let mut prefix = Vec::new();
        let t = cx.func.add_local("args", Type::JsVal);
        let mut body = Vec::new();
        let mut spread = false;
        for a in arguments {
            match a {
                Argument::SpreadElement(sp) => {
                    if !spread {
                        body.push(Stmt::Set(t, Expr::Array(std::mem::take(&mut prefix))));
                        spread = true;
                    }
                    let v = self.expr(cx, &sp.argument)?;
                    body.push(Stmt::Expr(Expr::intrinsic(
                        Intrinsic::ArraySpread,
                        vec![
                            Expr::Local(t),
                            jsval_of(v),
                            Expr::str(
                                "Spread syntax requires ...iterable[Symbol.iterator] to be a function",
                            ),
                        ],
                    )));
                }
                a => {
                    let e = a.as_expression().expect("the other arguments");
                    let v = jsval_of(self.expr(cx, e)?);
                    if spread {
                        body.push(Stmt::Expr(Expr::intrinsic(
                            Intrinsic::ArrayPush,
                            vec![Expr::Local(t), v],
                        )));
                    } else {
                        prefix.push(Some(v));
                    }
                }
            }
        }
        let _ = span;
        Ok(Expr::Seq {
            body,
            value: Box::new(Expr::Local(t)),
        })
    }

    /// A parameter with a default: `p = init`.
    pub(crate) fn param_default(
        &mut self,
        cx: &mut FnCx,
        p: &'a BindingPattern<'a>,
        init: &'a Expression<'a>,
        v: Expr,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        self.with_default(cx, Pat::Bind(p), init, v, Mode::Decl { fresh: false }, out)
    }

    /// `[a, b] = v` / `({ a } = v)`: the value is `v`.
    pub(crate) fn destructuring_assign(
        &mut self,
        cx: &mut FnCx,
        a: &'a AssignmentExpression<'a>,
    ) -> R<Val> {
        let rhs = self.expr(cx, &a.right)?;
        let t = cx.func.add_local("value", Type::JsVal);
        let mut body = vec![Stmt::Set(t, jsval_of(rhs))];
        let src = Src::Text(self.callee_text(&a.right));
        self.pattern(
            cx,
            Pat::Target(&a.left),
            Expr::Local(t),
            Mode::Assign,
            Some(src),
            &mut body,
        )?;
        Ok(Val::js(
            Expr::Seq {
                body,
                value: Box::new(Expr::Local(t)),
            },
            Kind::Any,
        ))
    }

    /// `for (x of v) body`, see the module docs.
    pub(crate) fn for_of(
        &mut self,
        cx: &mut FnCx,
        name: Option<String>,
        f: &'a ForOfStatement<'a>,
        out: &mut Vec<Stmt>,
    ) -> R<()> {
        if f.r#await {
            return Err(self.unsupported(f.span, "`for await`", "later"));
        }
        let right = self.expr(cx, &f.right)?;
        let it = cx.func.add_local("iter", Type::JsVal);
        let done = cx.func.add_local("done", Type::Bool);
        out.push(Stmt::Set(
            it,
            Expr::intrinsic(
                Intrinsic::IterOpen,
                vec![
                    jsval_of(right),
                    Expr::str(&format!("{} is not iterable", self.callee_text(&f.right))),
                ],
            ),
        ));
        out.push(Stmt::Set(done, Expr::bool(true)));
        // A `finally` around the loop: jumps out of it close the iterator first.
        let label = cx.label();
        let kind = cx.func.add_local("completion", Type::I32);
        let exc = cx.func.add_local("exception", Type::JsVal);
        cx.finally.push(Finally {
            depth: cx.targets.len(),
            label,
            kind,
            jumps: Vec::new(),
        });
        let brk = cx.label();
        let mut inner = vec![
            Stmt::Set(done, Expr::bool(true)),
            Stmt::If {
                cond: Expr::unary(
                    UnOp::Not,
                    Type::Bool,
                    Expr::intrinsic(Intrinsic::IterStep, vec![Expr::Local(it)]),
                ),
                then: vec![Stmt::Break(brk)],
                els: vec![],
            },
            Stmt::Set(done, Expr::bool(false)),
        ];
        let value = Expr::intrinsic(Intrinsic::IterValue, vec![Expr::Local(it)]);
        let head = match &f.left {
            ForStatementLeft::VariableDeclaration(d) => {
                let decl = &d.declarations[0];
                let fresh = d.kind != VariableDeclarationKind::Var;
                self.pattern(
                    cx,
                    Pat::Bind(&decl.id),
                    value,
                    Mode::Decl { fresh },
                    None,
                    &mut inner,
                )
            }
            left => match left.as_assignment_target() {
                Some(t) => self.pattern(cx, Pat::Target(t), value, Mode::Assign, None, &mut inner),
                None => Err(self.unsupported(left.span(), "`for-of` target", "later")),
            },
        };
        cx.targets.push(Target {
            js_label: name,
            brk,
            kind: TargetKind::While,
        });
        let body = self.stmt_body(cx, &f.body);
        cx.targets.pop();
        let fin = cx.finally.pop().expect("pushed above");
        head?;
        inner.extend(body);
        inner.push(Stmt::Continue(brk));
        out.push(Stmt::Set(kind, Expr::i32(0)));
        out.push(Stmt::Block {
            label,
            body: vec![Stmt::Try {
                body: vec![Stmt::Loop {
                    label: brk,
                    body: inner,
                }],
                catch_local: Some(exc),
                catch_body: vec![Stmt::Set(kind, Expr::i32(1))],
            }],
        });
        // The "finally": close unless the iterator finished (or failed) by itself.
        let is = |code: i32| Expr::binary(BinOp::Eq, Type::I32, Expr::Local(kind), Expr::i32(code));
        out.push(Stmt::If {
            cond: Expr::unary(UnOp::Not, Type::Bool, Expr::Local(done)),
            then: vec![Stmt::Expr(Expr::intrinsic(
                Intrinsic::IterClose,
                vec![Expr::Local(it), is(1)],
            ))],
            els: vec![],
        });
        for (i, j) in fin.jumps.into_iter().enumerate() {
            let value = match (j, cx.func.result) {
                (super::Jump::Return, Some(_)) => cx.ret.map(Expr::Local),
                _ => None,
            };
            let then = self.jump(cx, j, value);
            out.push(Stmt::If {
                cond: is(i as i32 + 2),
                then,
                els: vec![],
            });
        }
        out.push(Stmt::If {
            cond: is(1),
            then: vec![Stmt::Throw(Expr::Local(exc))],
            els: vec![],
        });
        Ok(())
    }
}
