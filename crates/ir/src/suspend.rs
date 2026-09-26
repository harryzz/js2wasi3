//! The suspension transform (T18, design D3): generators become resumable frames.
//!
//! [`lower_generators`] rewrites every function with `flags.is_generator`. Its body (from the
//! lowering) holds `Suspend` expressions, each followed by `Intrinsic::ResumeMode` checks, and
//! starts with an implicit `yield` after the prologue (parameter defaults, cells), so that the
//! prologue runs when the generator is created and the body on the first `next()`.
//!
//! # Output
//! - The function keeps its id and parameters and becomes the **start** function: it makes the
//!   generator object (`NewGenerator { resume, slots }`, one frame slot per local of the
//!   original function), stores its parameters in the frame (slot = local index), runs `resume`
//!   once (up to the implicit first `yield`) and returns the generator object.
//! - A new **resume** function `name$resume(gen, mode: i32, sent) -> jsval` holds the body. The
//!   runtime calls it for `next`/`throw`/`return` (`mode`, see [`crate::RESUME_NEXT`]) and gets
//!   back the value to yield (when it called `GenSuspend`) or the return value.
//!
//! # Algorithm
//! 1. **Flatten**: afterwards a `Suspend` only appears as `Set(l, Suspend { kind, arg })` at
//!    statement level, with no `Suspend` inside `arg`. In an expression holding one, the operands
//!    evaluated before the operand holding it are spilled to new locals (so their values are
//!    taken first, as in JS), the operand's own statements are hoisted, and a `Cond` whose branch
//!    holds one becomes an `If` setting a new local; a `Seq`'s body is hoisted. (A `Seq` may hold
//!    the `return` path of a `yield`, whose `Break`s are only allowed here, before the pass.)
//! 2. **Number** the yield points 1..n in source order (pre-order). The points inside any
//!    statement then form a range.
//! 3. **Liveness**: none yet. Every local of the original function is saved to the frame at each
//!    yield and restored on resume. (Correct, and simple; a later pass can save only live ones.)
//! 4. **Re-entry** keeps the structure (Wasm can't jump into a block): the resume function loads
//!    the frame and sets a local `st` to the saved point (0: the start). Then:
//!    - a statement list holding points starts with a dispatch: nested blocks, one ending just
//!      before each statement with points, and `if st != 0 { if st in range_i { br block_i } … }`;
//!    - an `If` holding points evaluates its condition only when `st == 0`, else it takes the
//!      branch whose range holds `st`;
//!    - `Block`/`Loop` bodies dispatch like any list (after the first pass `st` is 0, so a loop's
//!      later iterations run normally);
//!    - a `Try` whose catch body holds points becomes
//!      `block $a { block $c { if st in catch_range { br $c }; try { body } catch { br $c }; br $a }; catch_body }`,
//!      so resuming inside the catch body skips the `try_table` and resuming inside the body
//!      re-enters a fresh `try_table` (the same handler), with no code duplicated;
//!    - the point itself: `if st != 0 { st = 0; l = sent } else { tmp = arg; point = k; br $save }`;
//!      the body is wrapped in `block $save`, followed by one copy of `<save all locals>;
//!      GenSuspend(gen, point); return tmp` (T20: inlining the saves at every point made the code
//!      points × locals, which took wasmtime seconds to compile for a few `for await` loops).
//! 5. `ResumeMode` becomes the `mode` param. The lowering tests it right after each `Suspend`:
//!    throw → `throw sent`, return → a `return sent` at that point, which runs the enclosing
//!    `finally` code through the lowering's completion dispatch (design D2).
//!
//! # Async functions (T19)
//! A function with `flags.is_async` is transformed the same way (an `await` is a point like a
//! `yield`; it's never resumed with `return`). Its body has no implicit first suspension: the
//! start function makes the frame (`NewAsync`, one more slot for the promise), stores the
//! parameters and returns `AsyncStart(frame)`, which runs the body up to its first `await` and
//! gives the promise. The runtime resumes the frame from promise jobs (see the backend's
//! `asyncfn` module).
//!
//! # Async generators (T20)
//! Both flags: transformed like a generator (implicit first `yield`, the start function returns
//! the object), made by `NewAsyncGenerator`; its `await` points are `GenSuspend { awaiting }` so
//! the runtime (the backend's `asyncgen` module) tells them from `yield`s.

use crate::{
    BinOp, Const, Expr, FnFlags, FuncId, Function, Intrinsic, LabelId, LocalId, Module,
    RESUME_NEXT, Stmt, SuspendKind, Type,
};

/// Parameters of a resume function before the original locals.
const GEN: LocalId = LocalId(0);
const MODE: LocalId = LocalId(1);
const SENT: LocalId = LocalId(2);
const SHIFT: u32 = 3;

/// Rewrite every generator and async function of `m` (see the module docs).
pub fn lower_generators(m: &mut Module) {
    for i in 0..m.functions.len() {
        let flags = m.functions[i].flags;
        if flags.is_generator || flags.is_async {
            transform(m, FuncId(i as u32));
        }
    }
}

fn transform(m: &mut Module, id: FuncId) {
    let mut f = m.functions[id.0 as usize].clone();
    assert_eq!(
        f.result,
        Some(Type::JsVal),
        "generator {} must return a jsval",
        f.name
    );
    let body = std::mem::take(&mut f.body);
    let body = Flatten { f: &mut f, m }.stmts(body);
    let slots = f.locals.len() as u32;

    // The resume function: `gen`, `mode`, `sent`, then the original locals.
    let mut r = Function::new(
        format!("{}$resume", f.name),
        &[
            ("gen", Type::JsVal),
            ("mode", Type::I32),
            ("sent", Type::JsVal),
        ],
        Some(Type::JsVal),
    );
    r.pos = f.pos;
    r.locals.extend(f.locals.iter().cloned());
    let st = r.add_local("state", Type::I32);
    let mut body = body;
    for s in &mut body {
        shift_stmt(s);
    }
    let mut next_label = 0;
    for s in &body {
        max_label(s, &mut next_label);
    }
    let resume_id = FuncId(m.functions.len() as u32);
    // The shared suspension tail (see step 4 in the module docs).
    let save = LabelId(next_label);
    let (ytmp, kpoint, kcode) = (
        r.add_local("yielded", Type::JsVal),
        r.add_local("point", Type::I32),
        r.add_local("yield_kind", Type::I32),
    );
    let mut t = Transform {
        st,
        async_generator: f.flags.is_async && f.flags.is_generator,
        next_point: 1,
        next_label: next_label + 1,
        save,
        ytmp,
        kpoint,
        kcode,
        kinds: [false; 4],
    };
    let body = t.list(body);
    let has_points = t.next_point > 1;
    let kinds = t.kinds;
    let load = |i: u32| {
        let ty = f.locals[i as usize].ty;
        Stmt::Set(
            LocalId(i + SHIFT),
            from_js(ty, gen_op(Intrinsic::GenLoad(i), vec![])),
        )
    };
    let mut out = vec![
        Stmt::Set(st, gen_op(Intrinsic::GenPoint, vec![])),
        Stmt::If {
            cond: ne0(st),
            then: (0..slots).map(load).collect(),
            els: (0..f.params).map(load).collect(),
        },
    ];
    if has_points {
        let mut inner = body;
        inner.push(Stmt::Return(Some(Expr::undefined())));
        out.push(Stmt::Block {
            label: save,
            body: inner,
        });
        // Save every local once, for all the points.
        for i in 0..slots {
            let ty = r.locals[(i + SHIFT) as usize].ty;
            out.push(Stmt::Expr(gen_op(
                Intrinsic::GenStore(i),
                vec![to_js(ty, Expr::Local(LocalId(i + SHIFT)))],
            )));
        }
        let suspend = |raw, awaiting| {
            Stmt::Expr(gen_op(
                Intrinsic::GenSuspend { raw, awaiting },
                vec![Expr::Local(kpoint)],
            ))
        };
        // One `GenSuspend` per kind of point the function has.
        let is = |c: i32| Expr::binary(BinOp::Eq, Type::I32, Expr::Local(kcode), Expr::i32(c));
        let mut tail: Vec<Stmt> = Vec::new();
        for (code, raw, awaiting) in [
            (KIND_YIELD, false, false),
            (KIND_AWAITING, false, true),
            (KIND_RAW, true, false),
        ] {
            if !kinds[code as usize] {
                continue;
            }
            tail = if tail.is_empty() {
                vec![suspend(raw, awaiting)]
            } else {
                vec![Stmt::If {
                    cond: is(code),
                    then: vec![suspend(raw, awaiting)],
                    els: tail,
                }]
            };
        }
        out.extend(tail);
        out.push(Stmt::Return(Some(Expr::Local(ytmp))));
    } else {
        out.extend(body);
        out.push(Stmt::Return(Some(Expr::undefined())));
    }
    r.body = out;
    m.functions.push(r);

    // The start function.
    let mut s = Function {
        name: f.name.clone(),
        params: f.params,
        locals: f.locals[..f.params as usize].to_vec(),
        result: Some(Type::JsVal),
        flags: FnFlags::default(),
        body: Vec::new(),
        pos: f.pos,
    };
    let g = s.add_local("gen", Type::JsVal);
    let async_fn = f.flags.is_async && !f.flags.is_generator;
    let new = if f.flags.is_async && f.flags.is_generator {
        Intrinsic::NewAsyncGenerator {
            resume: resume_id,
            slots,
        }
    } else if async_fn {
        Intrinsic::NewAsync {
            resume: resume_id,
            slots,
        }
    } else {
        Intrinsic::NewGenerator {
            resume: resume_id,
            slots,
        }
    };
    s.body.push(Stmt::Set(g, Expr::intrinsic(new, vec![])));
    for i in 0..f.params {
        let ty = f.locals[i as usize].ty;
        s.body.push(Stmt::Expr(Expr::intrinsic(
            Intrinsic::GenStore(i),
            vec![Expr::Local(g), to_js(ty, Expr::Local(LocalId(i)))],
        )));
    }
    if async_fn {
        s.body.push(Stmt::Return(Some(Expr::intrinsic(
            Intrinsic::AsyncStart,
            vec![Expr::Local(g)],
        ))));
    } else {
        s.body.push(Stmt::Expr(Expr::Call {
            func: resume_id,
            args: vec![Expr::Local(g), Expr::i32(RESUME_NEXT), Expr::undefined()],
        }));
        s.body.push(Stmt::Return(Some(Expr::Local(g))));
    }
    m.functions[id.0 as usize] = s;
}

/// A `Gen*` intrinsic on the resume function's generator (`gen` is prepended to `args`).
fn gen_op(op: Intrinsic, args: Vec<Expr>) -> Expr {
    let mut all = vec![Expr::Local(GEN)];
    all.extend(args);
    Expr::intrinsic(op, all)
}

fn ne0(l: LocalId) -> Expr {
    Expr::binary(BinOp::Ne, Type::I32, Expr::Local(l), Expr::i32(0))
}

/// A value of type `ty` as a `jsval` (frame slots hold `jsval`s).
fn to_js(ty: Type, e: Expr) -> Expr {
    match ty {
        Type::JsVal => e,
        Type::I32 => Expr::convert(
            Type::F64,
            Type::JsVal,
            Expr::convert(Type::I32, Type::F64, e),
        ),
        ty => Expr::convert(ty, Type::JsVal, e),
    }
}

/// The inverse of [`to_js`].
fn from_js(ty: Type, e: Expr) -> Expr {
    match ty {
        Type::JsVal => e,
        Type::I32 => Expr::convert(
            Type::F64,
            Type::I32,
            Expr::convert(Type::JsVal, Type::F64, e),
        ),
        ty => Expr::convert(Type::JsVal, ty, e),
    }
}

// ---- step 1: flatten -----------------------------------------------------------------------

struct Flatten<'a> {
    f: &'a mut Function,
    m: &'a Module,
}

impl Flatten<'_> {
    fn temp(&mut self, name: &str, ty: Type) -> LocalId {
        self.f.add_local(name, ty)
    }

    fn stmts(&mut self, list: Vec<Stmt>) -> Vec<Stmt> {
        let mut out = Vec::new();
        for s in list {
            self.stmt(s, &mut out);
        }
        out
    }

    fn stmt(&mut self, s: Stmt, out: &mut Vec<Stmt>) {
        match s {
            Stmt::Set(l, Expr::Suspend { kind, arg }) => {
                let a = self.hoist(*arg, out);
                out.push(Stmt::Set(
                    l,
                    Expr::Suspend {
                        kind,
                        arg: Box::new(a),
                    },
                ));
            }
            Stmt::Expr(e) if has(&e) => {
                let e = self.hoist(e, out);
                if !matches!(e, Expr::Local(_) | Expr::Const(_)) {
                    out.push(Stmt::Expr(e));
                }
            }
            Stmt::Set(l, e) => {
                let e = self.hoist(e, out);
                out.push(Stmt::Set(l, e));
            }
            Stmt::SetGlobal(g, e) => {
                let e = self.hoist(e, out);
                out.push(Stmt::SetGlobal(g, e));
            }
            Stmt::Return(Some(e)) => {
                let e = self.hoist(e, out);
                out.push(Stmt::Return(Some(e)));
            }
            Stmt::Throw(e) => {
                let e = self.hoist(e, out);
                out.push(Stmt::Throw(e));
            }
            Stmt::If { cond, then, els } => {
                let cond = self.hoist(cond, out);
                let then = self.stmts(then);
                let els = self.stmts(els);
                out.push(Stmt::If { cond, then, els });
            }
            Stmt::Block { label, body } => {
                let body = self.stmts(body);
                out.push(Stmt::Block { label, body });
            }
            Stmt::Loop { label, body } => {
                let body = self.stmts(body);
                out.push(Stmt::Loop { label, body });
            }
            Stmt::Try {
                body,
                catch_local,
                catch_body,
            } => {
                let body = self.stmts(body);
                let catch_body = self.stmts(catch_body);
                out.push(Stmt::Try {
                    body,
                    catch_local,
                    catch_body,
                });
            }
            s => out.push(s),
        }
    }

    /// `e` without `Suspend`s: its statements go to `out` first.
    fn hoist(&mut self, e: Expr, out: &mut Vec<Stmt>) -> Expr {
        if !has(&e) {
            return e;
        }
        match e {
            Expr::Suspend { kind, arg } => {
                let a = self.hoist(*arg, out);
                let t = self.temp("sent", Type::JsVal);
                out.push(Stmt::Set(
                    t,
                    Expr::Suspend {
                        kind,
                        arg: Box::new(a),
                    },
                ));
                Expr::Local(t)
            }
            Expr::Tee(l, x) => Expr::Tee(l, Box::new(self.hoist(*x, out))),
            Expr::Unary { op, ty, arg } => Expr::Unary {
                op,
                ty,
                arg: Box::new(self.hoist(*arg, out)),
            },
            Expr::Convert { from, to, arg } => Expr::Convert {
                from,
                to,
                arg: Box::new(self.hoist(*arg, out)),
            },
            Expr::TypeofIs { arg, tag } => Expr::TypeofIs {
                arg: Box::new(self.hoist(*arg, out)),
                tag,
            },
            Expr::Binary { op, ty, lhs, rhs } => {
                let mut v = self.operands(vec![*lhs, *rhs], out);
                let rhs = v.pop().unwrap();
                let lhs = v.pop().unwrap();
                Expr::binary(op, ty, lhs, rhs)
            }
            Expr::Call { func, args } => Expr::Call {
                func,
                args: self.operands(args, out),
            },
            Expr::Intrinsic { op, args } => Expr::Intrinsic {
                op,
                args: self.operands(args, out),
            },
            Expr::Array(elems) => {
                let holes: Vec<bool> = elems.iter().map(|e| e.is_none()).collect();
                let mut vals = self
                    .operands(elems.into_iter().flatten().collect(), out)
                    .into_iter();
                Expr::Array(
                    holes
                        .into_iter()
                        .map(|h| if h { None } else { vals.next() })
                        .collect(),
                )
            }
            Expr::CallRef {
                callee,
                this,
                args,
                what,
            } => {
                let mut ops = vec![*callee, *this];
                ops.extend(args);
                let mut v = self.operands(ops, out).into_iter();
                let callee = Box::new(v.next().unwrap());
                let this = Box::new(v.next().unwrap());
                Expr::CallRef {
                    callee,
                    this,
                    args: v.collect(),
                    what,
                }
            }
            Expr::New {
                callee,
                new_target,
                args,
                what,
            } => {
                // The backend evaluates the callee, the arguments, then `new_target`.
                let n = args.len();
                let mut ops = vec![*callee];
                ops.extend(args);
                let has_target = new_target.is_some();
                ops.extend(new_target.map(|t| *t));
                let mut v = self.operands(ops, out).into_iter();
                let callee = Box::new(v.next().unwrap());
                let args: Vec<Expr> = v.by_ref().take(n).collect();
                let new_target = has_target.then(|| Box::new(v.next().unwrap()));
                Expr::New {
                    callee,
                    new_target,
                    args,
                    what,
                }
            }
            Expr::Closure(mut c) => {
                c.env = self.operands(std::mem::take(&mut c.env), out);
                Expr::Closure(c)
            }
            Expr::Cond {
                cond,
                then,
                els,
                ty,
            } => {
                if !has(&then) && !has(&els) {
                    let cond = self.hoist(*cond, out);
                    return Expr::Cond {
                        cond: Box::new(cond),
                        then,
                        els,
                        ty,
                    };
                }
                let cond = self.hoist(*cond, out);
                let t = self.temp("cond", ty);
                let mut a = Vec::new();
                let v = self.hoist(*then, &mut a);
                a.push(Stmt::Set(t, v));
                let mut b = Vec::new();
                let v = self.hoist(*els, &mut b);
                b.push(Stmt::Set(t, v));
                out.push(Stmt::If {
                    cond,
                    then: a,
                    els: b,
                });
                Expr::Local(t)
            }
            Expr::Seq { body, value } => {
                for s in body {
                    self.stmt(s, out);
                }
                self.hoist(*value, out)
            }
            Expr::Const(_) | Expr::Local(_) | Expr::Global(_) | Expr::Str(_) => {
                unreachable!("no Suspend inside")
            }
        }
    }

    /// Operands evaluated left to right, the last one holding a `Suspend` hoisted; those before
    /// it are spilled to temps (unless constant) so they're taken first.
    fn operands(&mut self, ops: Vec<Expr>, out: &mut Vec<Stmt>) -> Vec<Expr> {
        let Some(last) = ops.iter().rposition(has) else {
            return ops;
        };
        let mut res = Vec::with_capacity(ops.len());
        for (i, e) in ops.into_iter().enumerate() {
            if i > last {
                res.push(e);
                continue;
            }
            let e = self.hoist(e, out);
            if i == last || matches!(e, Expr::Const(_) | Expr::Str(_)) {
                res.push(e);
                continue;
            }
            let ty = e.ty(self.f, self.m).expect("operands have values");
            let t = self.temp("spill", ty);
            out.push(Stmt::Set(t, e));
            res.push(Expr::Local(t));
        }
        res
    }
}

/// Does `e` contain a `Suspend`?
fn has(e: &Expr) -> bool {
    match e {
        Expr::Suspend { .. } => true,
        Expr::Const(_) | Expr::Local(_) | Expr::Global(_) | Expr::Str(_) => false,
        Expr::Tee(_, x)
        | Expr::Unary { arg: x, .. }
        | Expr::Convert { arg: x, .. }
        | Expr::TypeofIs { arg: x, .. } => has(x),
        Expr::Binary { lhs, rhs, .. } => has(lhs) || has(rhs),
        Expr::Cond {
            cond, then, els, ..
        } => has(cond) || has(then) || has(els),
        Expr::Call { args, .. } | Expr::Intrinsic { args, .. } => args.iter().any(has),
        Expr::Seq { body, value } => body.iter().any(stmt_has) || has(value),
        Expr::Closure(c) => c.env.iter().any(has),
        Expr::Array(elems) => elems.iter().flatten().any(has),
        Expr::CallRef {
            callee, this, args, ..
        } => has(callee) || has(this) || args.iter().any(has),
        Expr::New {
            callee,
            new_target,
            args,
            ..
        } => has(callee) || new_target.as_deref().is_some_and(has) || args.iter().any(has),
    }
}

fn stmt_has(s: &Stmt) -> bool {
    match s {
        Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) | Stmt::Throw(e) => has(e),
        Stmt::Return(e) => e.as_ref().is_some_and(has),
        Stmt::Block { body, .. } | Stmt::Loop { body, .. } => body.iter().any(stmt_has),
        Stmt::If { cond, then, els } => {
            has(cond) || then.iter().any(stmt_has) || els.iter().any(stmt_has)
        }
        Stmt::Try {
            body, catch_body, ..
        } => body.iter().any(stmt_has) || catch_body.iter().any(stmt_has),
        Stmt::Break(_) | Stmt::Continue(_) => false,
    }
}

/// The number of yield points in a flattened statement.
fn points(s: &Stmt) -> u32 {
    match s {
        Stmt::Set(_, Expr::Suspend { .. }) => 1,
        Stmt::Block { body, .. } | Stmt::Loop { body, .. } => body.iter().map(points).sum(),
        Stmt::If { then, els, .. } => then.iter().chain(els).map(points).sum(),
        Stmt::Try {
            body, catch_body, ..
        } => body.iter().chain(catch_body).map(points).sum(),
        _ => 0,
    }
}

// ---- local renumbering ---------------------------------------------------------------------

fn shift(l: &mut LocalId) {
    l.0 += SHIFT;
}

fn shift_stmt(s: &mut Stmt) {
    match s {
        Stmt::Expr(e) | Stmt::SetGlobal(_, e) | Stmt::Throw(e) => shift_expr(e),
        Stmt::Set(l, e) => {
            shift(l);
            shift_expr(e);
        }
        Stmt::Return(e) => e.iter_mut().for_each(shift_expr),
        Stmt::Block { body, .. } | Stmt::Loop { body, .. } => body.iter_mut().for_each(shift_stmt),
        Stmt::If { cond, then, els } => {
            shift_expr(cond);
            then.iter_mut().chain(els).for_each(shift_stmt);
        }
        Stmt::Try {
            body,
            catch_local,
            catch_body,
        } => {
            catch_local.iter_mut().for_each(shift);
            body.iter_mut().chain(catch_body).for_each(shift_stmt);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

/// Renumber locals for the resume function; `ResumeMode` reads its `mode` param.
fn shift_expr(e: &mut Expr) {
    match e {
        Expr::Intrinsic {
            op: Intrinsic::ResumeMode,
            ..
        } => *e = Expr::Local(MODE),
        Expr::Local(l) => shift(l),
        Expr::Tee(l, x) => {
            shift(l);
            shift_expr(x);
        }
        Expr::Const(_) | Expr::Global(_) | Expr::Str(_) => {}
        Expr::Unary { arg, .. }
        | Expr::Convert { arg, .. }
        | Expr::TypeofIs { arg, .. }
        | Expr::Suspend { arg, .. } => shift_expr(arg),
        Expr::Binary { lhs, rhs, .. } => {
            shift_expr(lhs);
            shift_expr(rhs);
        }
        Expr::Cond {
            cond, then, els, ..
        } => {
            shift_expr(cond);
            shift_expr(then);
            shift_expr(els);
        }
        Expr::Call { args, .. } | Expr::Intrinsic { args, .. } => {
            args.iter_mut().for_each(shift_expr)
        }
        Expr::Seq { body, value } => {
            body.iter_mut().for_each(shift_stmt);
            shift_expr(value);
        }
        Expr::Closure(c) => c.env.iter_mut().for_each(shift_expr),
        Expr::Array(elems) => elems.iter_mut().flatten().for_each(shift_expr),
        Expr::CallRef {
            callee, this, args, ..
        } => {
            shift_expr(callee);
            shift_expr(this);
            args.iter_mut().for_each(shift_expr);
        }
        Expr::New {
            callee,
            new_target,
            args,
            ..
        } => {
            shift_expr(callee);
            new_target.iter_mut().for_each(|t| shift_expr(t));
            args.iter_mut().for_each(shift_expr);
        }
    }
}

/// Raise `next` past every label in `s` (including those in `Seq` bodies).
fn max_label(s: &Stmt, next: &mut u32) {
    fn expr(e: &Expr, next: &mut u32) {
        match e {
            Expr::Seq { body, value } => {
                body.iter().for_each(|s| max_label(s, next));
                expr(value, next);
            }
            Expr::Tee(_, x)
            | Expr::Unary { arg: x, .. }
            | Expr::Convert { arg: x, .. }
            | Expr::TypeofIs { arg: x, .. }
            | Expr::Suspend { arg: x, .. } => expr(x, next),
            Expr::Binary { lhs, rhs, .. } => {
                expr(lhs, next);
                expr(rhs, next);
            }
            Expr::Cond {
                cond, then, els, ..
            } => {
                expr(cond, next);
                expr(then, next);
                expr(els, next);
            }
            Expr::Call { args, .. } | Expr::Intrinsic { args, .. } => {
                args.iter().for_each(|a| expr(a, next))
            }
            Expr::Closure(c) => c.env.iter().for_each(|a| expr(a, next)),
            Expr::Array(elems) => elems.iter().flatten().for_each(|a| expr(a, next)),
            Expr::CallRef {
                callee, this, args, ..
            } => {
                expr(callee, next);
                expr(this, next);
                args.iter().for_each(|a| expr(a, next));
            }
            Expr::New {
                callee,
                new_target,
                args,
                ..
            } => {
                expr(callee, next);
                new_target.iter().for_each(|a| expr(a, next));
                args.iter().for_each(|a| expr(a, next));
            }
            Expr::Const(_) | Expr::Local(_) | Expr::Global(_) | Expr::Str(_) => {}
        }
    }
    match s {
        Stmt::Block { label, body } | Stmt::Loop { label, body } => {
            *next = (*next).max(label.0 + 1);
            body.iter().for_each(|s| max_label(s, next));
        }
        Stmt::Expr(e) | Stmt::Set(_, e) | Stmt::SetGlobal(_, e) | Stmt::Throw(e) => expr(e, next),
        Stmt::Return(e) => e.iter().for_each(|e| expr(e, next)),
        Stmt::If { cond, then, els } => {
            expr(cond, next);
            then.iter().chain(els).for_each(|s| max_label(s, next));
        }
        Stmt::Try {
            body, catch_body, ..
        } => body
            .iter()
            .chain(catch_body)
            .for_each(|s| max_label(s, next)),
        Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

// ---- steps 2 and 4: numbering and re-entry ---------------------------------------------------

struct Transform {
    st: LocalId,
    /// An async generator (T20): its `await`s are marked for the runtime.
    async_generator: bool,
    next_point: u32,
    next_label: u32,
    /// The block every point breaks out of to save the locals and return (one copy of the
    /// saving code: inlining it at every point made code size points × locals); the value to
    /// return, the point and its kind ([`KIND_RAW`], [`KIND_AWAITING`], else a plain yield).
    save: LabelId,
    ytmp: LocalId,
    kpoint: LocalId,
    kcode: LocalId,
    /// The kinds of points seen (indexed by kind).
    kinds: [bool; 4],
}

const KIND_YIELD: i32 = 1;
const KIND_RAW: i32 = 2;
const KIND_AWAITING: i32 = 3;

impl Transform {
    fn label(&mut self) -> LabelId {
        self.next_label += 1;
        LabelId(self.next_label - 1)
    }

    /// `st` is in `[lo, lo + n)` (n ≥ 1).
    fn in_range(&self, lo: u32, n: u32) -> Expr {
        let st = || Expr::Local(self.st);
        if n == 1 {
            return Expr::binary(BinOp::Eq, Type::I32, st(), Expr::i32(lo as i32));
        }
        Expr::Cond {
            cond: Box::new(Expr::binary(
                BinOp::Ge,
                Type::I32,
                st(),
                Expr::i32(lo as i32),
            )),
            then: Box::new(Expr::binary(
                BinOp::Lt,
                Type::I32,
                st(),
                Expr::i32((lo + n) as i32),
            )),
            els: Box::new(Expr::Const(Const::Bool(false))),
            ty: Type::Bool,
        }
    }

    fn list(&mut self, list: Vec<Stmt>) -> Vec<Stmt> {
        let counts: Vec<u32> = list.iter().map(points).collect();
        if counts.iter().all(|c| *c == 0) {
            return list;
        }
        // Each statement's range, before transforming it (which consumes the points).
        let mut ranges = Vec::new();
        let mut p = self.next_point;
        for (i, c) in counts.iter().enumerate() {
            if *c > 0 {
                ranges.push((i, p, *c));
                p += c;
            }
        }
        let mut tr: Vec<Stmt> = list.into_iter().map(|s| self.stmt(s)).collect();
        if ranges.len() == 1 && ranges[0].0 == 0 {
            // Only the first statement has points: it dispatches itself.
            return tr;
        }
        let labels: Vec<LabelId> = ranges.iter().map(|_| self.label()).collect();
        let mut dispatch = Vec::new();
        for (j, (_, lo, n)) in ranges.iter().enumerate() {
            if j + 1 == ranges.len() {
                dispatch.push(Stmt::Break(labels[j]));
            } else {
                dispatch.push(Stmt::If {
                    cond: self.in_range(*lo, *n),
                    then: vec![Stmt::Break(labels[j])],
                    els: vec![],
                });
            }
        }
        // Split `tr` at the target statements: parts[0] before the first, parts[j] from target
        // j-1 up to target j, the last part from the last target on.
        let mut parts: Vec<Vec<Stmt>> = Vec::new();
        for (i, _, _) in ranges.iter().rev() {
            parts.push(tr.split_off(*i));
        }
        parts.push(tr);
        parts.reverse();
        let mut parts = parts.into_iter();
        let mut inner = vec![Stmt::If {
            cond: ne0(self.st),
            then: dispatch,
            els: vec![],
        }];
        inner.extend(parts.next().unwrap());
        let mut cur = Stmt::Block {
            label: labels[0],
            body: inner,
        };
        for (j, part) in parts.enumerate() {
            if j + 1 == labels.len() {
                let mut out = vec![cur];
                out.extend(part);
                return out;
            }
            let mut body = vec![cur];
            body.extend(part);
            cur = Stmt::Block {
                label: labels[j + 1],
                body,
            };
        }
        unreachable!("one part per target plus the first")
    }

    fn stmt(&mut self, s: Stmt) -> Stmt {
        if points(&s) == 0 {
            return s;
        }
        match s {
            Stmt::Set(l, Expr::Suspend { kind, arg }) => {
                let k = self.next_point;
                self.next_point += 1;
                let code = match kind {
                    SuspendKind::YieldDelegate => KIND_RAW,
                    SuspendKind::Await if self.async_generator => KIND_AWAITING,
                    SuspendKind::Yield | SuspendKind::Await => KIND_YIELD,
                };
                self.kinds[code as usize] = true;
                let save = vec![
                    Stmt::Set(self.ytmp, *arg),
                    Stmt::Set(self.kpoint, Expr::i32(k as i32)),
                    Stmt::Set(self.kcode, Expr::i32(code)),
                    Stmt::Break(self.save),
                ];
                Stmt::If {
                    cond: ne0(self.st),
                    then: vec![
                        Stmt::Set(self.st, Expr::i32(0)),
                        Stmt::Set(l, Expr::Local(SENT)),
                    ],
                    els: save,
                }
            }
            Stmt::If { cond, then, els } => {
                let (lo, nt) = (self.next_point, then.iter().map(points).sum::<u32>());
                let pick = if nt == 0 {
                    Expr::Const(Const::Bool(false))
                } else if els.iter().map(points).sum::<u32>() == 0 {
                    Expr::Const(Const::Bool(true))
                } else {
                    self.in_range(lo, nt)
                };
                let cond = Expr::Cond {
                    cond: Box::new(ne0(self.st)),
                    then: Box::new(pick),
                    els: Box::new(cond),
                    ty: Type::Bool,
                };
                let then = self.list(then);
                let els = self.list(els);
                Stmt::If { cond, then, els }
            }
            Stmt::Block { label, body } => Stmt::Block {
                label,
                body: self.list(body),
            },
            Stmt::Loop { label, body } => Stmt::Loop {
                label,
                body: self.list(body),
            },
            Stmt::Try {
                body,
                catch_local,
                catch_body,
            } => {
                let nc: u32 = catch_body.iter().map(points).sum();
                let body = self.list(body);
                if nc == 0 {
                    return Stmt::Try {
                        body,
                        catch_local,
                        catch_body,
                    };
                }
                let enter_catch = self.in_range(self.next_point, nc);
                let catch_body = self.list(catch_body);
                let (after, catch) = (self.label(), self.label());
                let mut outer = vec![Stmt::Block {
                    label: catch,
                    body: vec![
                        Stmt::If {
                            cond: enter_catch,
                            then: vec![Stmt::Break(catch)],
                            els: vec![],
                        },
                        Stmt::Try {
                            body,
                            catch_local,
                            catch_body: vec![Stmt::Break(catch)],
                        },
                        Stmt::Break(after),
                    ],
                }];
                outer.extend(catch_body);
                Stmt::Block {
                    label: after,
                    body: outer,
                }
            }
            s => unreachable!("statement with a yield point after flattening: {s:?}"),
        }
    }
}
