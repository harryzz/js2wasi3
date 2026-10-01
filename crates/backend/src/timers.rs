//! Timers (T19, design D3): `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval`.
//! Commands only (the lowering rejects them in libraries): they read `monotonic-clock.now`.
//!
//! A timer is a `$timer` (the Timeout object the set functions return, a plain object for JS):
//! the deadline (ns of `monotonic-clock`), the interval (ns; -1: once), the callback and its
//! extra arguments. The active timers are a list in creation order in globals (`G_TIMERS`,
//! `G_TIMERS_N`).
//!
//! Like node (whose clock has ms steps), a timer's deadline is the current time truncated to a
//! whole ms plus the delay, which is clamped to [1, 2^31 - 1] ms (anything else, NaN included,
//! is 1 ms); an interval's next deadline counts from just before its callback runs.
//! [`Helper::RunDue`] runs one pass: every timer due at the loop time (`G_LOOP_NOW`, set by the
//! host when the loop wakes up), earliest deadline first (ties: creation order), draining the
//! microtasks after each callback. Timers made during the pass are due after the loop time, so
//! they wait for the next pass. The host (`crate::host`) waits for [`Helper::Next`] with one
//! `wait-until` subtask.
//!
//! T25: Timeout / Immediate objects have prototypes (`ref`, `unref`, `hasRef`, `refresh`,
//! `[Symbol.toPrimitive]`: the id, which `clearTimeout` also takes). An unref'd timer doesn't
//! keep the loop alive. Immediates (`setImmediate`) are `$timer`s in a FIFO (`G_IMMEDIATES`) run
//! after the due timers of a pass, only those queued before it (node's check phase).
//! `process.nextTick` callbacks queue in `G_TICKS`; after a callback node runs the ticks, then
//! the microtasks, until both are empty ([`Helper::RunTicks`]); after the main code of an ES module
//! the microtasks come first (of a CommonJS entry, the ticks: T60).

use js2wasi3_ir::{
    Builtin, TIMER_CLEAR_IMMEDIATE, TIMER_CLEAR_INTERVAL, TIMER_CLEAR_TIMEOUT, TIMER_NAMES,
    TIMER_NEXT_TICK, TIMER_SET_IMMEDIATE, TIMER_SET_INTERVAL,
};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::host::Import;
use crate::runtime::{
    self, Cx, G_FALSE, G_IMMEDIATES, G_IMMEDIATES_N, G_LOOP_NOW, G_TICKS, G_TICKS_N, G_TIMER_FNS,
    G_TIMER_ID, G_TIMERS, G_TIMERS_N, G_TRUE, G_UNDEFINED, T_BYTES, T_I32S, T_KEYS, T_TIMER,
    T_VALS, jsval, rt,
};
use crate::{func, object, promise};

/// `$timer` field indices after the object's.
pub const F_DEADLINE: u32 = 7;
pub const F_INTERVAL: u32 = 8;
pub const F_CALLBACK: u32 = 9;
pub const F_ARGS: u32 = 10;
/// T25: the delay (ns), the id, the flags.
pub const F_DELAY: u32 = 11;
pub const F_ID: u32 = 12;
pub const F_FLAGS: u32 = 13;

/// `$timer` flags (T25).
const FLAG_UNREF: i32 = 1;
const FLAG_IMMEDIATE: i32 = 2;
const FLAG_CLEARED: i32 = 4;

/// The lists (T25): active timers, queued immediates, `nextTick` callbacks.
const LIST_TIMERS: u8 = 0;
const LIST_IMMEDIATES: u8 = 1;
const LIST_TICKS: u8 = 2;

fn list_globals(list: u8) -> (u32, u32) {
    match list {
        LIST_TIMERS => (G_TIMERS, G_TIMERS_N),
        LIST_IMMEDIATES => (G_IMMEDIATES, G_IMMEDIATES_N),
        _ => (G_TICKS, G_TICKS_N),
    }
}

/// The longest delay, in ms (node's TIMEOUT_MAX).
const TIMEOUT_MAX: f64 = 2147483647.0;
const NS_PER_MS: f64 = 1e6;

/// Push the current time truncated to a whole ms (ns, an `i64`).
fn now_ms(s: &mut InstructionSink, clock: u32) {
    s.call(clock);
    s.i64_const(NS_PER_MS as i64).i64_div_u();
    s.i64_const(NS_PER_MS as i64).i64_mul();
}

pub fn timer() -> HeapType {
    HeapType::Concrete(T_TIMER)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `() -> jsval`: timer function `k` (`js2wasi3_ir::TIMER_NAMES`), made on first use.
    Fn(u8),
    /// Its code (a `$code`).
    Code(u8),
    /// `(t: (ref $timer))`: remove `t` from list `l` (active timers or immediates) if it's there.
    Remove(u8),
    /// `(v: eqref)`: append `v` to list `l` (T25).
    Append(u8),
    /// `() -> i64`: the earliest deadline (0 with immediates queued), or -1 when nothing keeps
    /// the loop alive (no timer or immediate that isn't unref'd).
    Next,
    /// `() -> eqref`: run the timers due at the loop time (see the module docs), then the queued
    /// immediates. Exceptions from a callback or a job propagate. Stops at the first unhandled
    /// rejection after a callback's jobs: its reason (Wasm null: none).
    RunDue,
    /// `() -> eqref`: the immediates part (T25).
    RunImmediates,
    /// `()`: node's processTicksAndRejections: the `nextTick` queue, then the microtasks, until
    /// both are empty (T25).
    RunTicks,
    /// The codes of `ref` (`true`) / `unref`, `hasRef`, `refresh`, `[Symbol.toPrimitive]` (T25).
    Ref(bool),
    HasRef,
    Refresh,
    ToPrimitive,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Fn(_) => "timer_function",
            Helper::Code(k) => TIMER_NAMES[k as usize],
            Helper::Remove(_) => "remove_timer",
            Helper::Append(_) => "append_timer",
            Helper::Next => "next_timer",
            Helper::RunDue => "run_due_timers",
            Helper::RunImmediates => "run_immediates",
            Helper::RunTicks => "run_ticks",
            Helper::Ref(true) => "ref",
            Helper::Ref(false) => "unref",
            Helper::HasRef => "hasRef",
            Helper::Refresh => "refresh",
            Helper::ToPrimitive => "[Symbol.toPrimitive]",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        match self {
            Helper::Fn(_) | Helper::RunDue | Helper::RunImmediates => (vec![], vec![v]),
            Helper::Code(_)
            | Helper::Ref(_)
            | Helper::HasRef
            | Helper::Refresh
            | Helper::ToPrimitive => (vec![v, v, v], vec![v]),
            Helper::Remove(_) => (vec![rt(false, T_TIMER)], vec![]),
            Helper::Append(_) => (vec![v], vec![]),
            Helper::Next => (vec![], vec![ValType::I64]),
            Helper::RunTicks => (vec![], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Fn(k) => {
                let code = cx.helper(runtime::Helper::Timer(Helper::Code(k)));
                cx.declare_func(code);
                let new = cx.helper(runtime::Helper::Func(func::Helper::New));
                let name = cx.str(TIMER_NAMES[k as usize]);
                let g = G_TIMER_FNS + k as u32;
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.global_get(g).ref_is_null().i32_eqz();
                s.if_(BlockType::Empty);
                s.global_get(g).return_();
                s.end();
                // node: setTimeout.length is 1 (callback), the clear functions' 1 (timer).
                promise::native(&mut s, new, code, None, name, 1);
                s.ref_cast_non_null(runtime::object()).global_set(g);
                s.global_get(g);
                s.end();
                f
            }
            Helper::Code(k) if k == TIMER_CLEAR_TIMEOUT || k == TIMER_CLEAR_INTERVAL => {
                clear(cx, false)
            }
            Helper::Code(k) if k == TIMER_CLEAR_IMMEDIATE => clear(cx, true),
            Helper::Code(k) if k == TIMER_NEXT_TICK => next_tick(cx),
            Helper::Code(k) => set(cx, k),
            Helper::Remove(l) => remove(l),
            Helper::Append(l) => append(l),
            Helper::Next => next(cx),
            Helper::RunDue => run_due(cx),
            Helper::RunImmediates => run_immediates(cx),
            Helper::RunTicks => run_ticks(cx),
            Helper::Ref(on) => method(cx, |_, s| {
                s.struct_get(T_TIMER, F_FLAGS);
                if on {
                    s.i32_const(!FLAG_UNREF).i32_and();
                } else {
                    s.i32_const(FLAG_UNREF).i32_or();
                }
                s.local_set(4);
                s.local_get(3).ref_as_non_null().local_get(4);
                s.struct_set(T_TIMER, F_FLAGS);
                s.local_get(THIS);
            }),
            Helper::HasRef => method(cx, |_, s| {
                s.struct_get(T_TIMER, F_FLAGS)
                    .i32_const(FLAG_UNREF)
                    .i32_and();
                s.if_(BlockType::Result(jsval()));
                s.global_get(G_FALSE);
                s.else_();
                s.global_get(G_TRUE);
                s.end();
            }),
            Helper::Refresh => refresh(cx),
            Helper::ToPrimitive => method(cx, |_, s| {
                s.struct_get(T_TIMER, F_ID).ref_i31();
            }),
        }
    }
}

/// `Timeout.prototype` / `Immediate.prototype` methods: (code, name, length).
pub fn proto_methods(b: Builtin) -> Vec<(Helper, &'static str, i32)> {
    let mut out = vec![
        (Helper::Ref(true), "ref", 0),
        (Helper::Ref(false), "unref", 0),
        (Helper::HasRef, "hasRef", 0),
    ];
    if b == Builtin::Timeout {
        out.push((Helper::Refresh, "refresh", 0));
    }
    out
}

const THIS: u32 = 1;

/// A prototype method's code: `this` must be a `$timer` (else `undefined`); `body` gets it on the
/// stack (a copy in local 3, an `i32` scratch in 4) and leaves the result.
fn method(cx: &mut dyn Cx, body: impl Fn(&mut dyn Cx, &mut InstructionSink)) -> Function {
    let mut f = Function::new([(1, rt(true, T_TIMER)), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(timer()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(THIS).ref_cast_non_null(timer()).local_tee(3);
    body(cx, &mut s);
    s.end();
    f
}

fn th(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Timer(h))
}

/// What runs after a callback: the `nextTick` queue and the microtasks (T25), or just the
/// microtasks when the code never calls `process.nextTick`.
pub fn after_callback(cx: &mut dyn Cx) -> u32 {
    if cx.names("$ticks") {
        th(cx, Helper::RunTicks)
    } else {
        cx.helper(runtime::Helper::Prom(promise::Helper::Drain))
    }
}

fn get(s: &mut InstructionSink, local: u32, field: u32) {
    s.local_get(local)
        .ref_cast_non_null(timer())
        .struct_get(T_TIMER, field);
}

/// Push element `i` of list `l` (a `(ref $timer)`).
fn nth(s: &mut InstructionSink, l: u8, i: u32) {
    s.global_get(list_globals(l).0)
        .ref_as_non_null()
        .local_get(i);
    s.array_get(T_VALS).ref_cast_non_null(timer());
}

/// Push the extra arguments of a `$code` from index `first` (a `(ref $vals)`); uses locals `n`
/// (i32) and `rest` (`(ref null $vals)`).
fn rest_args(s: &mut InstructionSink, first: i32, n: u32, rest: u32) {
    const ARGS: u32 = 2;
    s.local_get(ARGS)
        .ref_cast_non_null(func::vals())
        .array_len()
        .local_tee(n);
    s.i32_const(first).i32_sub().i32_const(0);
    s.local_get(n).i32_const(first).i32_gt_u();
    s.select();
    s.array_new_default(T_VALS).local_set(rest);
    s.local_get(n).i32_const(first).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(rest).ref_as_non_null().i32_const(0);
    s.local_get(ARGS)
        .ref_cast_non_null(func::vals())
        .i32_const(first);
    s.local_get(n).i32_const(first).i32_sub();
    s.array_copy(T_VALS, T_VALS);
    s.end();
    s.local_get(rest).ref_as_non_null();
}

/// `setTimeout(callback, delay, ...args)` / `setInterval` / `setImmediate(callback, ...args)`.
fn set(cx: &mut dyn Cx, k: u8) -> Function {
    let clock = cx.imports().get(Import::ClockNow);
    let repeat = k == TIMER_SET_INTERVAL;
    let immediate = k == TIMER_SET_IMMEDIATE;
    let check = cx.helper(runtime::Helper::Prom(promise::Helper::CheckCallback));
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let b = if immediate {
        Builtin::Immediate
    } else {
        Builtin::Timeout
    };
    let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(b)));
    let list = if immediate {
        LIST_IMMEDIATES
    } else {
        LIST_TIMERS
    };
    let append = th(cx, Helper::Append(list));
    const CB: u32 = 3;
    const MS: u32 = 4;
    const N: u32 = 5;
    const REST: u32 = 6;
    const T: u32 = 7;
    const DELAY: u32 = 8;
    let mut f = Function::new([
        (1, jsval()),
        (1, ValType::F64),
        (1, ValType::I32),
        (1, rt(true, T_VALS)),
        (1, rt(true, T_TIMER)),
        (1, ValType::I64),
    ]);
    let mut s = f.instructions();
    promise::arg(cx, &mut s, 0);
    s.local_tee(CB).call(check);
    if !immediate {
        promise::arg(cx, &mut s, 1);
        s.call(to_number).local_tee(MS);
        // Not in [1, TIMEOUT_MAX] (NaN included): 1.
        s.f64_const(1.0.into()).f64_ge();
        s.local_get(MS).f64_const(TIMEOUT_MAX.into()).f64_le();
        s.i32_and().i32_eqz();
        s.if_(BlockType::Empty);
        s.f64_const(1.0.into()).local_set(MS);
        s.end();
        s.local_get(MS)
            .f64_const(NS_PER_MS.into())
            .f64_mul()
            .i64_trunc_f64_u()
            .local_set(DELAY);
    }
    // The timer object.
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    if immediate {
        s.i64_const(0);
    } else {
        now_ms(&mut s, clock);
        s.local_get(DELAY).i64_add();
    }
    if repeat {
        s.local_get(DELAY);
    } else {
        s.i64_const(-1);
    }
    s.local_get(CB);
    rest_args(&mut s, if immediate { 1 } else { 2 }, N, REST);
    s.local_get(DELAY);
    s.global_get(G_TIMER_ID)
        .i32_const(1)
        .i32_add()
        .global_set(G_TIMER_ID);
    s.global_get(G_TIMER_ID);
    s.i32_const(if immediate { FLAG_IMMEDIATE } else { 0 });
    s.struct_new(T_TIMER).local_tee(T);
    s.call(append);
    s.local_get(T);
    s.end();
    f
}

/// `process.nextTick(callback, ...args)` (T25): queue the callback and its arguments.
fn next_tick(cx: &mut dyn Cx) -> Function {
    let check = cx.helper(runtime::Helper::Prom(promise::Helper::CheckCallback));
    let append = th(cx, Helper::Append(LIST_TICKS));
    const CB: u32 = 3;
    const N: u32 = 4;
    const REST: u32 = 5;
    let mut f = Function::new([(1, jsval()), (1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    promise::arg(cx, &mut s, 0);
    s.local_tee(CB).call(check);
    s.local_get(CB).call(append);
    rest_args(&mut s, 1, N, REST);
    s.call(append);
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

/// `$append(v)` to list `l`: grown by doubling, at least 4 slots.
fn append(l: u8) -> Function {
    let (list, count) = list_globals(l);
    const V: u32 = 0;
    const CAP: u32 = 1;
    const NEW: u32 = 2;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    s.global_get(list).ref_is_null();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    s.global_get(count)
        .global_get(list)
        .ref_as_non_null()
        .array_len()
        .i32_eq();
    s.end();
    s.if_(BlockType::Empty);
    {
        s.global_get(count).i32_const(1).i32_shl().local_tee(CAP);
        s.i32_const(4);
        s.local_get(CAP).i32_const(4).i32_gt_u();
        s.select();
        s.array_new_default(T_VALS).local_set(NEW);
        s.global_get(count).i32_eqz().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(NEW).ref_as_non_null().i32_const(0);
        s.global_get(list).ref_as_non_null().i32_const(0);
        s.global_get(count);
        s.array_copy(T_VALS, T_VALS);
        s.end();
        s.local_get(NEW).global_set(list);
    }
    s.end();
    s.global_get(list).ref_as_non_null();
    s.global_get(count).local_get(V);
    s.array_set(T_VALS);
    s.global_get(count).i32_const(1).i32_add().global_set(count);
    s.end();
    f
}

/// Mark timer local `t` cleared and remove it from list `l`.
fn cancel(s: &mut InstructionSink, t: u32, remove: u32) {
    s.local_get(t).ref_as_non_null();
    get(s, t, F_FLAGS);
    s.i32_const(FLAG_CLEARED).i32_or();
    s.struct_set(T_TIMER, F_FLAGS);
    s.local_get(t).ref_as_non_null().call(remove);
}

/// `clearTimeout(timer)` / `clearInterval` (the same in node): a Timeout, or its id (a number or
/// a string, T25); anything else is ignored. `clearImmediate(immediate)`.
fn clear(cx: &mut dyn Cx, immediate: bool) -> Function {
    let list = if immediate {
        LIST_IMMEDIATES
    } else {
        LIST_TIMERS
    };
    let remove = th(cx, Helper::Remove(list));
    const V: u32 = 3;
    const T: u32 = 4;
    const I: u32 = 5;
    const ID: u32 = 6;
    let mut f = Function::new([
        (1, jsval()),
        (1, rt(true, T_TIMER)),
        (1, ValType::I32),
        (1, ValType::F64),
    ]);
    let mut s = f.instructions();
    promise::arg(cx, &mut s, 0);
    s.local_tee(V).ref_test_non_null(timer());
    s.if_(BlockType::Empty);
    {
        s.local_get(V).ref_cast_non_null(timer()).local_tee(T);
        s.struct_get(T_TIMER, F_FLAGS)
            .i32_const(FLAG_IMMEDIATE)
            .i32_and();
        if !immediate {
            s.i32_eqz();
        }
        s.if_(BlockType::Empty);
        cancel(&mut s, T, remove);
        s.end();
    }
    if !immediate {
        let to_number = cx.helper(runtime::Helper::ToNumber);
        let is_number = cx.helper(runtime::Helper::IsNumber);
        s.else_();
        s.local_get(V).call(is_number);
        runtime::is_string(&mut s, V);
        s.i32_or();
        s.if_(BlockType::Empty);
        {
            s.local_get(V).call(to_number).local_set(ID);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(I).global_get(G_TIMERS_N).i32_ge_u().br_if(1);
            nth(&mut s, LIST_TIMERS, I);
            s.local_tee(T).struct_get(T_TIMER, F_ID).f64_convert_i32_s();
            s.local_get(ID).f64_eq();
            s.if_(BlockType::Empty);
            cancel(&mut s, T, remove);
            s.br(2);
            s.end();
            s.local_get(I).i32_const(1).i32_add().local_set(I);
            s.br(0).end().end();
        }
        s.end();
    }
    s.end();
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

/// `timeout.refresh()`: due one delay from now, again active if it ran already (not if it was
/// cleared); it moves to the end of the list, like node's.
fn refresh(cx: &mut dyn Cx) -> Function {
    let clock = cx.imports().get(Import::ClockNow);
    let remove = th(cx, Helper::Remove(LIST_TIMERS));
    let append = th(cx, Helper::Append(LIST_TIMERS));
    method(cx, move |_, s| {
        s.struct_get(T_TIMER, F_FLAGS)
            .i32_const(FLAG_CLEARED)
            .i32_and();
        s.i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(3).ref_as_non_null().call(remove);
        s.local_get(3).ref_as_non_null();
        now_ms(s, clock);
        get(s, 3, F_DELAY);
        s.i64_add();
        s.struct_set(T_TIMER, F_DEADLINE);
        s.local_get(3).call(append);
        s.end();
        s.local_get(THIS);
    })
}

fn remove(l: u8) -> Function {
    let (list, count) = list_globals(l);
    const T: u32 = 0;
    const I: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        s.local_get(I).global_get(count).i32_ge_u().br_if(1);
        nth(&mut s, l, I);
        s.local_get(T).ref_eq();
        s.if_(BlockType::Empty);
        {
            // Shift the rest down (keeps the list's order: creation order).
            s.global_get(list).ref_as_non_null().local_get(I);
            s.global_get(list).ref_as_non_null();
            s.local_get(I).i32_const(1).i32_add();
            s.global_get(count)
                .local_get(I)
                .i32_sub()
                .i32_const(1)
                .i32_sub();
            s.array_copy(T_VALS, T_VALS);
            s.global_get(count).i32_const(1).i32_sub().global_set(count);
            s.global_get(list).ref_as_non_null();
            s.global_get(count);
            s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
            s.array_set(T_VALS);
            s.return_();
        }
        s.end();
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0);
    }
    s.end().end();
    s.end();
    f
}

fn next(cx: &mut dyn Cx) -> Function {
    const I: u32 = 0;
    const REFED: u32 = 1;
    const D: u32 = 2;
    const X: u32 = 3;
    let mut f = Function::new([(2, ValType::I32), (2, ValType::I64)]);
    let mut s = f.instructions();
    // Is element `I` of list `l` not unref'd?
    let refed = |s: &mut InstructionSink, l: u8| {
        nth(s, l, I);
        s.struct_get(T_TIMER, F_FLAGS)
            .i32_const(FLAG_UNREF)
            .i32_and();
        s.i32_eqz().local_get(REFED).i32_or().local_set(REFED);
    };
    s.i64_const(-1).local_set(D);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        s.local_get(I).global_get(G_TIMERS_N).i32_ge_u().br_if(1);
        refed(&mut s, LIST_TIMERS);
        nth(&mut s, LIST_TIMERS, I);
        s.struct_get(T_TIMER, F_DEADLINE).local_tee(X);
        s.local_get(D);
        s.local_get(X).local_get(D).i64_lt_u();
        s.local_get(D).i64_const(-1).i64_eq();
        s.i32_or();
        s.select().local_set(D);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0);
    }
    s.end().end();
    if cx.names("$immediates") {
        // Queued immediates: due now.
        s.global_get(G_IMMEDIATES_N).i32_eqz().i32_eqz();
        s.if_(BlockType::Empty);
        s.i64_const(0).local_set(D);
        s.end();
        s.i32_const(0).local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(I)
            .global_get(G_IMMEDIATES_N)
            .i32_ge_u()
            .br_if(1);
        refed(&mut s, LIST_IMMEDIATES);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0).end().end();
    }
    s.local_get(D).i64_const(-1).local_get(REFED).select();
    s.end();
    f
}

/// Call timer local `t`'s callback (node: with the timer as `this`), then the ticks and
/// microtasks; return the first unhandled rejection's reason, if any (from the function).
fn run_callback(cx: &mut dyn Cx, s: &mut InstructionSink, t: u32, r: u32) {
    let after = after_callback(cx);
    let unhandled = cx.helper(runtime::Helper::Prom(promise::Helper::Unhandled));
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let empty = cx.str("");
    get(s, t, F_CALLBACK);
    s.local_get(t);
    get(s, t, F_ARGS);
    s.global_get(empty);
    s.call(call_value).drop();
    s.call(after);
    s.call(unhandled).local_tee(r).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(r).return_();
    s.end();
}

fn run_due(cx: &mut dyn Cx) -> Function {
    let clock = cx.imports().get(Import::ClockNow);
    let remove = th(cx, Helper::Remove(LIST_TIMERS));
    let immediates = cx
        .names("$immediates")
        .then(|| th(cx, Helper::RunImmediates));
    const I: u32 = 0;
    const T: u32 = 1;
    const X: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, rt(true, T_TIMER)),
        (1, rt(true, T_TIMER)),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.loop_(BlockType::Empty);
    {
        // The earliest due timer (the list is in creation order: `<` keeps the first of a tie).
        s.ref_null(timer()).local_set(T);
        s.i32_const(0).local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        {
            s.local_get(I).global_get(G_TIMERS_N).i32_ge_u().br_if(1);
            nth(&mut s, LIST_TIMERS, I);
            s.local_tee(X).struct_get(T_TIMER, F_DEADLINE);
            s.global_get(G_LOOP_NOW).i64_le_u();
            s.if_(BlockType::Empty);
            {
                s.local_get(T).ref_is_null();
                s.if_(BlockType::Result(ValType::I32));
                s.i32_const(1);
                s.else_();
                get(&mut s, X, F_DEADLINE);
                get(&mut s, T, F_DEADLINE);
                s.i64_lt_u();
                s.end();
                s.if_(BlockType::Empty);
                s.local_get(X).local_set(T);
                s.end();
            }
            s.end();
            s.local_get(I).i32_const(1).i32_add().local_set(I);
            s.br(0);
        }
        s.end().end();
        s.local_get(T).ref_is_null();
        s.if_(BlockType::Empty);
        match immediates {
            Some(run) => s.call(run),
            None => s.ref_null(wasm_encoder::RefType::EQREF.heap_type),
        };
        s.return_();
        s.end();
        // An interval is due again one interval from now; a timeout is done.
        get(&mut s, T, F_INTERVAL);
        s.i64_const(0).i64_lt_s();
        s.if_(BlockType::Empty);
        s.local_get(T).ref_as_non_null().call(remove);
        s.else_();
        s.local_get(T).ref_as_non_null();
        now_ms(&mut s, clock);
        get(&mut s, T, F_INTERVAL);
        s.i64_add();
        s.struct_set(T_TIMER, F_DEADLINE);
        s.end();
        run_callback(cx, &mut s, T, R);
        s.br(0);
    }
    s.end();
    s.unreachable();
    s.end();
    f
}

/// The immediates queued before this pass, in order (those queued meanwhile wait for the next).
fn run_immediates(cx: &mut dyn Cx) -> Function {
    const L: u32 = 0;
    const N: u32 = 1;
    const I: u32 = 2;
    const T: u32 = 3;
    const R: u32 = 4;
    let mut f = Function::new([
        (1, rt(true, T_VALS)),
        (2, ValType::I32),
        (1, rt(true, T_TIMER)),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.global_get(G_IMMEDIATES).local_set(L);
    s.global_get(G_IMMEDIATES_N).local_set(N);
    s.ref_null(HeapType::Concrete(T_VALS))
        .global_set(G_IMMEDIATES);
    s.i32_const(0).global_set(G_IMMEDIATES_N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        s.local_get(I).local_get(N).i32_ge_u().br_if(1);
        s.local_get(L)
            .ref_as_non_null()
            .local_get(I)
            .array_get(T_VALS);
        s.ref_cast_non_null(timer()).local_set(T);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        // Cleared while queued (`clearImmediate` of one in this pass).
        get(&mut s, T, F_FLAGS);
        s.i32_const(FLAG_CLEARED).i32_and().br_if(0);
        run_callback(cx, &mut s, T, R);
        s.br(0);
    }
    s.end().end();
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    s.end();
    f
}

/// node's processTicksAndRejections: every queued `nextTick` callback (also those queued by
/// them), then the microtasks; again while ticks were queued.
fn run_ticks(cx: &mut dyn Cx) -> Function {
    let drain = cx.helper(runtime::Helper::Prom(promise::Helper::Drain));
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let empty = cx.str("");
    const I: u32 = 0;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.loop_(BlockType::Empty);
    {
        s.i32_const(0).local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        {
            s.local_get(I).global_get(G_TICKS_N).i32_ge_u().br_if(1);
            s.global_get(G_TICKS)
                .ref_as_non_null()
                .local_get(I)
                .array_get(T_VALS);
            s.global_get(G_UNDEFINED);
            s.global_get(G_TICKS).ref_as_non_null();
            s.local_get(I).i32_const(1).i32_add().array_get(T_VALS);
            s.ref_cast_non_null(func::vals());
            s.global_get(empty);
            s.local_get(I).i32_const(2).i32_add().local_set(I);
            s.call(call_value).drop();
            s.br(0);
        }
        s.end().end();
        // Done: forget them (the slots are reused).
        s.global_get(G_TICKS_N).i32_eqz().i32_eqz();
        s.if_(BlockType::Empty);
        s.global_get(G_TICKS).ref_as_non_null().i32_const(0);
        s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
        s.global_get(G_TICKS_N);
        s.array_fill(T_VALS);
        s.end();
        s.i32_const(0).global_set(G_TICKS_N);
        s.call(drain);
        s.global_get(G_TICKS_N).br_if(0);
    }
    s.end();
    s.end();
    f
}
