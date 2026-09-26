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

use js2wasi3_ir::{
    Builtin, TIMER_CLEAR_INTERVAL, TIMER_CLEAR_TIMEOUT, TIMER_NAMES, TIMER_SET_INTERVAL,
};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::host::{self, Import};
use crate::runtime::{
    self, Cx, G_LOOP_NOW, G_TIMER_FNS, G_TIMERS, G_TIMERS_N, G_UNDEFINED, T_BYTES, T_I32S, T_KEYS,
    T_TIMER, T_VALS, jsval, rt,
};
use crate::{func, object, promise};

/// `$timer` field indices after the object's.
pub const F_DEADLINE: u32 = 7;
pub const F_INTERVAL: u32 = 8;
pub const F_CALLBACK: u32 = 9;
pub const F_ARGS: u32 = 10;

/// The longest delay, in ms (node's TIMEOUT_MAX).
const TIMEOUT_MAX: f64 = 2147483647.0;
const NS_PER_MS: f64 = 1e6;

/// Push the current time truncated to a whole ms (ns, an `i64`).
fn now_ms(s: &mut InstructionSink) {
    s.call(host::import(Import::ClockNow));
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
    /// `(t: (ref $timer))`: remove `t` from the active timers (if it's there).
    Remove,
    /// `() -> i64`: the earliest deadline, or -1 when there are no timers.
    Next,
    /// `() -> eqref`: run the timers due at the loop time (see the module docs). Exceptions from
    /// a callback or a job propagate. Stops at the first unhandled rejection after a callback's
    /// jobs: its reason (Wasm null: none).
    RunDue,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Fn(_) => "timer_function",
            Helper::Code(k) => TIMER_NAMES[k as usize],
            Helper::Remove => "remove_timer",
            Helper::Next => "next_timer",
            Helper::RunDue => "run_due_timers",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        match self {
            Helper::Fn(_) | Helper::RunDue => (vec![], vec![v]),
            Helper::Code(_) => (vec![v, v, v], vec![v]),
            Helper::Remove => (vec![rt(false, T_TIMER)], vec![]),
            Helper::Next => (vec![], vec![ValType::I64]),
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
            Helper::Code(k) if k == TIMER_CLEAR_TIMEOUT || k == TIMER_CLEAR_INTERVAL => clear(cx),
            Helper::Code(k) => set(cx, k == TIMER_SET_INTERVAL),
            Helper::Remove => remove(),
            Helper::Next => next(),
            Helper::RunDue => run_due(cx),
        }
    }
}

fn th(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Timer(h))
}

fn get(s: &mut InstructionSink, local: u32, field: u32) {
    s.local_get(local)
        .ref_cast_non_null(timer())
        .struct_get(T_TIMER, field);
}

/// Push timer `i` of the list (a `(ref $timer)`).
fn nth(s: &mut InstructionSink, i: u32) {
    s.global_get(G_TIMERS).ref_as_non_null().local_get(i);
    s.array_get(T_VALS).ref_cast_non_null(timer());
}

/// `setTimeout(callback, delay, ...args)` / `setInterval`.
fn set(cx: &mut dyn Cx, repeat: bool) -> Function {
    let check = cx.helper(runtime::Helper::Prom(promise::Helper::CheckCallback));
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(Builtin::Object)));
    const ARGS: u32 = 2;
    const CB: u32 = 3;
    const MS: u32 = 4;
    const N: u32 = 5;
    const REST: u32 = 6;
    const T: u32 = 7;
    const CAP: u32 = 8;
    let mut f = Function::new([
        (1, jsval()),
        (1, ValType::F64),
        (1, ValType::I32),
        (1, rt(true, T_VALS)),
        (1, rt(true, T_TIMER)),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    promise::arg(cx, &mut s, 0);
    s.local_tee(CB).call(check);
    promise::arg(cx, &mut s, 1);
    s.call(to_number).local_tee(MS);
    // Not in [1, TIMEOUT_MAX] (NaN included): 1.
    s.f64_const(1.0.into()).f64_ge();
    s.local_get(MS).f64_const(TIMEOUT_MAX.into()).f64_le();
    s.i32_and().i32_eqz();
    s.if_(BlockType::Empty);
    s.f64_const(1.0.into()).local_set(MS);
    s.end();
    // The extra arguments.
    s.local_get(ARGS)
        .ref_cast_non_null(func::vals())
        .array_len()
        .local_tee(N);
    s.i32_const(2).i32_sub().i32_const(0);
    s.local_get(N).i32_const(2).i32_gt_u();
    s.select();
    s.array_new_default(T_VALS).local_set(REST);
    s.local_get(N).i32_const(2).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(REST).ref_as_non_null().i32_const(0);
    s.local_get(ARGS)
        .ref_cast_non_null(func::vals())
        .i32_const(2);
    s.local_get(N).i32_const(2).i32_sub();
    s.array_copy(T_VALS, T_VALS);
    s.end();
    // The timer object.
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    now_ms(&mut s);
    s.local_get(MS)
        .f64_const(NS_PER_MS.into())
        .f64_mul()
        .i64_trunc_f64_u();
    s.i64_add();
    if repeat {
        s.local_get(MS)
            .f64_const(NS_PER_MS.into())
            .f64_mul()
            .i64_trunc_f64_u();
    } else {
        s.i64_const(-1);
    }
    s.local_get(CB);
    s.local_get(REST).ref_as_non_null();
    s.struct_new(T_TIMER).local_set(T);
    // Append it to the list (grown by doubling, at least 4).
    s.global_get(G_TIMERS).ref_is_null();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    s.global_get(G_TIMERS_N)
        .global_get(G_TIMERS)
        .ref_as_non_null()
        .array_len()
        .i32_eq();
    s.end();
    s.if_(BlockType::Empty);
    {
        s.global_get(G_TIMERS_N)
            .i32_const(1)
            .i32_shl()
            .local_tee(CAP);
        s.i32_const(4);
        s.local_get(CAP).i32_const(4).i32_gt_u();
        s.select();
        s.array_new_default(T_VALS).local_set(REST);
        s.global_get(G_TIMERS_N).i32_eqz().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(REST).ref_as_non_null().i32_const(0);
        s.global_get(G_TIMERS).ref_as_non_null().i32_const(0);
        s.global_get(G_TIMERS_N);
        s.array_copy(T_VALS, T_VALS);
        s.end();
        s.local_get(REST).global_set(G_TIMERS);
    }
    s.end();
    s.global_get(G_TIMERS).ref_as_non_null();
    s.global_get(G_TIMERS_N).local_get(T);
    s.array_set(T_VALS);
    s.global_get(G_TIMERS_N)
        .i32_const(1)
        .i32_add()
        .global_set(G_TIMERS_N);
    s.local_get(T);
    s.end();
    f
}

/// `clearTimeout(timer)` / `clearInterval` (the same in node): anything but a timer is ignored.
fn clear(cx: &mut dyn Cx) -> Function {
    let remove = th(cx, Helper::Remove);
    const T: u32 = 3;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    promise::arg(cx, &mut s, 0);
    s.local_tee(T).ref_test_non_null(timer());
    s.if_(BlockType::Empty);
    s.local_get(T).ref_cast_non_null(timer()).call(remove);
    s.end();
    s.global_get(G_UNDEFINED);
    s.end();
    f
}

fn remove() -> Function {
    const T: u32 = 0;
    const I: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        s.local_get(I).global_get(G_TIMERS_N).i32_ge_u().br_if(1);
        nth(&mut s, I);
        s.local_get(T).ref_eq();
        s.if_(BlockType::Empty);
        {
            // Shift the rest down (keeps the list's order: creation order).
            s.global_get(G_TIMERS).ref_as_non_null().local_get(I);
            s.global_get(G_TIMERS).ref_as_non_null();
            s.local_get(I).i32_const(1).i32_add();
            s.global_get(G_TIMERS_N)
                .local_get(I)
                .i32_sub()
                .i32_const(1)
                .i32_sub();
            s.array_copy(T_VALS, T_VALS);
            s.global_get(G_TIMERS_N)
                .i32_const(1)
                .i32_sub()
                .global_set(G_TIMERS_N);
            s.global_get(G_TIMERS).ref_as_non_null();
            s.global_get(G_TIMERS_N);
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

fn next() -> Function {
    const I: u32 = 0;
    const D: u32 = 1;
    const X: u32 = 2;
    let mut f = Function::new([(1, ValType::I32), (2, ValType::I64)]);
    let mut s = f.instructions();
    s.i64_const(-1).local_set(D);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        s.local_get(I).global_get(G_TIMERS_N).i32_ge_u().br_if(1);
        nth(&mut s, I);
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
    s.local_get(D);
    s.end();
    f
}

fn run_due(cx: &mut dyn Cx) -> Function {
    let remove = th(cx, Helper::Remove);
    let drain = cx.helper(runtime::Helper::Prom(promise::Helper::Drain));
    let unhandled = cx.helper(runtime::Helper::Prom(promise::Helper::Unhandled));
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let empty = cx.str("");
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
            nth(&mut s, I);
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
        s.ref_null(wasm_encoder::RefType::EQREF.heap_type).return_();
        s.end();
        // An interval is due again one interval from now; a timeout is done.
        get(&mut s, T, F_INTERVAL);
        s.i64_const(0).i64_lt_s();
        s.if_(BlockType::Empty);
        s.local_get(T).ref_as_non_null().call(remove);
        s.else_();
        s.local_get(T).ref_as_non_null();
        now_ms(&mut s);
        get(&mut s, T, F_INTERVAL);
        s.i64_add();
        s.struct_set(T_TIMER, F_DEADLINE);
        s.end();
        // node calls it with the timer as `this`.
        get(&mut s, T, F_CALLBACK);
        s.local_get(T);
        get(&mut s, T, F_ARGS);
        s.global_get(empty);
        s.call(call_value).drop();
        s.call(drain);
        s.call(unhandled).local_tee(R).ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(R).return_();
        s.end();
        s.br(0);
    }
    s.end();
    s.unreachable();
    s.end();
    f
}
