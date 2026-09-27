//! Generators (T18): `function*`, `yield` and `yield*`; async functions and `await` (T19).
//!
//! A generator is lowered like any function, with `flags.is_generator`, a `jsval` result and an
//! implicit `yield` after its prologue; `ir::suspend` then turns it into a start function and a
//! resume function. Here each `yield` becomes a `Suspend` followed by its resume handling:
//! `throw(v)` throws `v` at the `yield`, `return(v)` is a `return v` there, so it runs the
//! enclosing `finally` blocks like any `return` (see [`Lowerer::try_stmt`]).
//!
//! `yield* v` is a loop: `DelegateStep` calls the inner iterator's `next`/`throw`/`return` with
//! the value the generator was resumed with, and each result object that isn't done is passed out
//! as is (`SuspendKind::YieldDelegate`).
//!
//! In an async generator (T29) the loop is [`Lowerer::async_yield_star`]: the prelude
//! (`builtins/asyncgen.js`) opens the inner iterator (a sync one wrapped by
//! CreateAsyncFromSyncIterator) and makes the calls; the generator awaits each result and yields
//! each value with an ordinary async-generator `yield`.

use js2wasi3_ir::{
    self as ir, BinOp, Expr, Intrinsic, RESUME_RETURN, RESUME_THROW, Stmt, SuspendKind, Type,
};

use super::{FnCx, Jump, Kind, Lowerer, Slot};

impl<'p, 'a> Lowerer<'p, 'a> {
    /// Make `func` (being declared) a generator: its result, the generator object, is a `jsval`
    /// of any kind.
    pub(crate) fn generator_sig(&mut self, func: &mut ir::Function, result: Slot) {
        func.flags.is_generator = true;
        func.result = Some(Type::JsVal);
        self.observe(result, Kind::Any);
    }

    /// Make `func` (being declared) an async function (T19): its result, the promise, is a
    /// `jsval` of any kind.
    pub(crate) fn async_sig(&mut self, func: &mut ir::Function, result: Slot) {
        func.flags.is_async = true;
        func.result = Some(Type::JsVal);
        self.observe(result, Kind::Any);
        if func.flags.is_generator {
            // The backend links the async generator runtime (T20).
            self.module.names.insert("$asyncgen".to_string());
        }
    }

    /// `await arg` (also `for await`'s): a top-level one makes `main` an async function.
    pub(crate) fn await_value(&mut self, cx: &mut FnCx, arg: Expr) -> Expr {
        if Some(cx.id) == self.main {
            cx.func.flags.is_async = true;
            cx.func.result = Some(Type::JsVal);
        }
        self.suspend(cx, SuspendKind::Await, arg)
    }

    /// A `yield` (an `await`) of `arg`: the value the generator (async function) is resumed with.
    pub(crate) fn suspend(&mut self, cx: &mut FnCx, kind: SuspendKind, arg: Expr) -> Expr {
        let sent = cx.func.add_local("sent", Type::JsVal);
        let mode = cx.func.add_local("mode", Type::I32);
        let is = |m: i32| Expr::binary(BinOp::Eq, Type::I32, Expr::Local(mode), Expr::i32(m));
        // An async function is never resumed with `return`. An async generator awaits the value
        // it's resumed with by `return(v)` (T20).
        let ret = if kind == SuspendKind::Await {
            Vec::new()
        } else if cx.func.flags.is_async {
            let awaited = self.await_value(cx, Expr::Local(sent));
            let mut ret = vec![Stmt::Set(sent, awaited)];
            ret.extend(self.jump(cx, Jump::Return, Some(Expr::Local(sent))));
            ret
        } else {
            self.jump(cx, Jump::Return, Some(Expr::Local(sent)))
        };
        let mut body = vec![
            Stmt::Set(
                sent,
                Expr::Suspend {
                    kind,
                    arg: Box::new(arg),
                },
            ),
            Stmt::Set(mode, Expr::intrinsic(Intrinsic::ResumeMode, vec![])),
            Stmt::If {
                cond: is(RESUME_THROW),
                then: vec![Stmt::Throw(Expr::Local(sent))],
                els: vec![],
            },
        ];
        if !ret.is_empty() {
            body.push(Stmt::If {
                cond: is(RESUME_RETURN),
                then: ret,
                els: vec![],
            });
        }
        Expr::Seq {
            body,
            value: Box::new(Expr::Local(sent)),
        }
    }

    /// `yield* iterable`: the inner iterator's return value.
    pub(crate) fn yield_star(&mut self, cx: &mut FnCx, iterable: Expr) -> Expr {
        let it = cx.func.add_local("delegate", Type::JsVal);
        let mode = cx.func.add_local("mode", Type::I32);
        let received = cx.func.add_local("received", Type::JsVal);
        let r = cx.func.add_local("inner", Type::JsVal);
        let result = cx.func.add_local("result", Type::JsVal);
        let lp = cx.label();
        let get =
            |key: &str| Expr::intrinsic(Intrinsic::GetProp, vec![Expr::Local(r), Expr::str(key)]);
        // The inner iterator has no `return`: return the value we got.
        let ret_received = self.jump(cx, Jump::Return, Some(Expr::Local(received)));
        let ret_value = self.jump(cx, Jump::Return, Some(Expr::Local(result)));
        let body = vec![
            Stmt::Set(it, Expr::intrinsic(Intrinsic::DelegateOpen, vec![iterable])),
            Stmt::Set(mode, Expr::i32(ir::RESUME_NEXT)),
            Stmt::Set(received, Expr::undefined()),
            Stmt::Loop {
                label: lp,
                body: vec![
                    Stmt::Set(
                        r,
                        Expr::intrinsic(
                            Intrinsic::DelegateStep,
                            vec![Expr::Local(it), Expr::Local(mode), Expr::Local(received)],
                        ),
                    ),
                    Stmt::If {
                        cond: Expr::intrinsic(Intrinsic::IsHole, vec![Expr::Local(r)]),
                        then: ret_received,
                        els: vec![],
                    },
                    Stmt::If {
                        cond: Expr::convert(Type::JsVal, Type::Bool, get("done")),
                        then: vec![
                            Stmt::Set(result, get("value")),
                            Stmt::If {
                                cond: Expr::binary(
                                    BinOp::Eq,
                                    Type::I32,
                                    Expr::Local(mode),
                                    Expr::i32(RESUME_RETURN),
                                ),
                                then: ret_value,
                                els: vec![],
                            },
                            Stmt::Break(lp),
                        ],
                        els: vec![],
                    },
                    Stmt::Set(
                        received,
                        Expr::Suspend {
                            kind: SuspendKind::YieldDelegate,
                            arg: Box::new(Expr::Local(r)),
                        },
                    ),
                    Stmt::Set(mode, Expr::intrinsic(Intrinsic::ResumeMode, vec![])),
                    Stmt::Continue(lp),
                ],
            },
        ];
        Expr::Seq {
            body,
            value: Box::new(Expr::Local(result)),
        }
    }

    /// A direct call of a prelude function with JS-value arguments, as a JS value (T29).
    fn prelude_call(&mut self, name: &str, args: Vec<Expr>) -> Expr {
        let sym = self.prelude.names[name];
        let func = self.funcs[&sym];
        for p in self.func_params[&func].clone().into_iter().flatten() {
            self.observe(Slot::Var(p), Kind::Any);
        }
        let call = Expr::Call { func, args };
        match self.module.function(func).result {
            Some(Type::JsVal) => call,
            Some(t) => Expr::convert(t, Type::JsVal, call),
            None => Expr::Seq {
                body: vec![Stmt::Expr(call)],
                value: Box::new(Expr::undefined()),
            },
        }
    }

    /// `yield* iterable` in an async generator (T29, ES2025 14.4.14 with generatorKind async):
    /// the inner iterator's result objects are awaited and must be objects; each value that isn't
    /// the last is yielded (not awaited again); `throw`/`return` are forwarded, a missing `throw`
    /// closes the inner iterator and throws a TypeError, a missing `return` returns the
    /// (twice-awaited) value. A `return` resumption is awaited first
    /// (AsyncGeneratorUnwrapYieldResumption); if that rejects, the delegation continues with a
    /// `throw`.
    pub(crate) fn async_yield_star(&mut self, cx: &mut FnCx, iterable: Expr) -> Expr {
        let rec = cx.func.add_local("delegate", Type::JsVal);
        let mode = cx.func.add_local("mode", Type::I32);
        let received = cx.func.add_local("received", Type::JsVal);
        let inner = cx.func.add_local("inner", Type::JsVal);
        let m = cx.func.add_local("method", Type::JsVal);
        let c = cx.func.add_local("close", Type::JsVal);
        let result = cx.func.add_local("result", Type::JsVal);
        let err = cx.func.add_local("error", Type::JsVal);
        let lp = cx.label();
        let l = Expr::Local;
        let is_mode =
            |k: i32| Expr::binary(BinOp::Eq, Type::I32, Expr::Local(mode), Expr::i32(k));
        let is_undefined =
            |local| Expr::binary(BinOp::Eq, Type::JsVal, Expr::Local(local), Expr::undefined());
        let get = |key: &str| {
            Expr::intrinsic(Intrinsic::GetProp, vec![Expr::Local(inner), Expr::str(key)])
        };
        let open = self.prelude_call("$$asyncDelegateOpen", vec![iterable]);
        // mode `next`.
        let next_call = self.prelude_call("$$asyncDelegateNext", vec![l(rec), l(received)]);
        let next = vec![Stmt::Set(inner, self.await_value(cx, next_call))];
        // mode `throw`.
        let throw_method = self.prelude_call("$$asyncDelegateMethod", vec![l(rec), Expr::str("throw")]);
        let return_method =
            self.prelude_call("$$asyncDelegateMethod", vec![l(rec), Expr::str("return")]);
        let close_call = self.prelude_call("$$asyncDelegateClose", vec![l(rec), l(c)]);
        let closed = self.await_value(cx, close_call);
        let check_closed = self.prelude_call("$$asyncDelegateResult", vec![closed]);
        let no_throw = self.prelude_call("$$asyncDelegateNoThrow", vec![]);
        let throw_call = self.prelude_call("$$asyncDelegateCall", vec![l(rec), l(m), l(received)]);
        let throw = vec![
            Stmt::Set(m, throw_method),
            Stmt::If {
                cond: is_undefined(m),
                then: vec![
                    Stmt::Set(c, return_method.clone()),
                    Stmt::If {
                        cond: Expr::unary(ir::UnOp::Not, Type::Bool, is_undefined(c)),
                        then: vec![Stmt::Expr(check_closed)],
                        els: vec![],
                    },
                    Stmt::Expr(no_throw),
                ],
                els: vec![],
            },
            Stmt::Set(inner, self.await_value(cx, throw_call)),
        ];
        // mode `return`.
        let twice = self.await_value(cx, l(received));
        let ret_received = self.jump(cx, Jump::Return, Some(l(received)));
        let return_call =
            self.prelude_call("$$asyncDelegateCall", vec![l(rec), l(m), l(received)]);
        let mut no_return = vec![Stmt::Set(received, twice)];
        no_return.extend(ret_received);
        let ret = vec![
            Stmt::Set(m, return_method),
            Stmt::If {
                cond: is_undefined(m),
                then: no_return,
                els: vec![],
            },
            Stmt::Set(inner, self.await_value(cx, return_call)),
        ];
        // Done: the `yield*` value, or (mode `return`) the generator returns it, awaited.
        let awaited_result = self.await_value(cx, l(result));
        let mut ret_done = vec![Stmt::Set(result, awaited_result)];
        ret_done.extend(self.jump(cx, Jump::Return, Some(l(result))));
        let check = self.prelude_call("$$asyncDelegateResult", vec![l(inner)]);
        // A `return` resumption is awaited; a rejection continues as a `throw`.
        let unwrapped = self.await_value(cx, l(received));
        let body = vec![
            Stmt::Set(rec, open),
            Stmt::Set(mode, Expr::i32(ir::RESUME_NEXT)),
            Stmt::Set(received, Expr::undefined()),
            Stmt::Loop {
                label: lp,
                body: vec![
                    Stmt::If {
                        cond: is_mode(RESUME_THROW),
                        then: throw,
                        els: vec![Stmt::If {
                            cond: is_mode(RESUME_RETURN),
                            then: ret,
                            els: next,
                        }],
                    },
                    Stmt::Expr(check),
                    Stmt::If {
                        cond: Expr::convert(Type::JsVal, Type::Bool, get("done")),
                        then: vec![
                            Stmt::Set(result, get("value")),
                            Stmt::If {
                                cond: is_mode(RESUME_RETURN),
                                then: ret_done,
                                els: vec![],
                            },
                            Stmt::Break(lp),
                        ],
                        els: vec![],
                    },
                    Stmt::Set(
                        received,
                        Expr::Suspend {
                            kind: SuspendKind::Yield,
                            arg: Box::new(get("value")),
                        },
                    ),
                    Stmt::Set(mode, Expr::intrinsic(Intrinsic::ResumeMode, vec![])),
                    Stmt::If {
                        cond: is_mode(RESUME_RETURN),
                        then: vec![Stmt::Try {
                            body: vec![Stmt::Set(received, unwrapped)],
                            catch_local: Some(err),
                            catch_body: vec![
                                Stmt::Set(mode, Expr::i32(RESUME_THROW)),
                                Stmt::Set(received, l(err)),
                            ],
                        }],
                        els: vec![],
                    },
                    Stmt::Continue(lp),
                ],
            },
        ];
        Expr::Seq {
            body,
            value: Box::new(l(result)),
        }
    }
}
