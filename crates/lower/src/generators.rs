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
}
