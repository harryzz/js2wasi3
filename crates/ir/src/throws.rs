//! Which functions may throw a JS value out of their body (T12): the library boundary gives
//! exactly those exports a `result<T, js-error>` WIT result.
//!
//! A function may throw if it contains, outside the body of a `Try`, a `Throw`, a call of a
//! function that may throw, a call through a value, or an intrinsic that may throw ([`Intrinsic::may_throw`]). A catch
//! body is outside its `Try`'s protection. Recursion is handled by iterating to a fixpoint.

use crate::{Expr, Intrinsic, Module, Stmt, visit_exprs};

/// Can the module queue a promise job at all (T20, F65)? Then a library drains the queue after
/// each export call. Module-wide: getters, `valueOf` and other implicit calls can run any function.
/// Before or after the suspension transform: an async function (generator) queues jobs.
pub fn can_queue(m: &Module) -> bool {
    let mut queue = m.functions.iter().any(|f| f.flags.is_async);
    for f in &m.functions {
        visit_exprs(&f.body, &mut |x| {
            queue |= matches!(
                x,
                Expr::Intrinsic {
                    op: Intrinsic::PromiseCtor
                        | Intrinsic::QueueMicrotask
                        | Intrinsic::Timer(_)
                        | Intrinsic::AsyncStart
                        | Intrinsic::NewAsyncGenerator { .. },
                    ..
                }
            );
        });
    }
    queue
}

/// For each function (by index): may a JS value be thrown out of it?
pub fn may_throw(m: &Module) -> Vec<bool> {
    let mut throws = vec![false; m.functions.len()];
    loop {
        let mut changed = false;
        for (i, f) in m.functions.iter().enumerate() {
            if !throws[i] && stmts(&f.body, &throws) {
                throws[i] = true;
                changed = true;
            }
        }
        if !changed {
            return throws;
        }
    }
}

fn stmts(list: &[Stmt], throws: &[bool]) -> bool {
    list.iter().any(|s| match s {
        Stmt::Throw(_) => true,
        Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) | Stmt::Return(Some(e)) => {
            expr(e, throws)
        }
        Stmt::Return(None) | Stmt::Break(_) | Stmt::Continue(_) => false,
        Stmt::Block { body, .. } | Stmt::Loop { body, .. } => stmts(body, throws),
        Stmt::If { cond, then, els } => {
            expr(cond, throws) || stmts(then, throws) || stmts(els, throws)
        }
        // Whatever the body throws is caught.
        Stmt::Try { catch_body, .. } => stmts(catch_body, throws),
    })
}

fn expr(e: &Expr, throws: &[bool]) -> bool {
    match e {
        Expr::Const(_) | Expr::Local(_) | Expr::Global(_) | Expr::Str(_) => false,
        Expr::Tee(_, a)
        | Expr::Unary { arg: a, .. }
        | Expr::Convert { arg: a, .. }
        | Expr::TypeofIs { arg: a, .. }
        | Expr::Suspend { arg: a, .. } => expr(a, throws),
        Expr::Binary { lhs, rhs, .. } => expr(lhs, throws) || expr(rhs, throws),
        Expr::Cond {
            cond, then, els, ..
        } => expr(cond, throws) || expr(then, throws) || expr(els, throws),
        Expr::Call { func, args } => {
            throws[func.0 as usize] || args.iter().any(|a| expr(a, throws))
        }
        Expr::Intrinsic { op, args } => op.may_throw() || args.iter().any(|a| expr(a, throws)),
        Expr::Seq { body, value } => stmts(body, throws) || expr(value, throws),
        Expr::Closure(c) => c.env.iter().any(|a| expr(a, throws)),
        Expr::Array(elems) => elems.iter().flatten().any(|a| expr(a, throws)),
        // Unknown callee (T14).
        Expr::CallRef { .. } | Expr::New { .. } => true,
    }
}
