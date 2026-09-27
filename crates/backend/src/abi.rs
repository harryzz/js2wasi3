//! The core side of the Canonical ABI for exports that take or return strings (design D1:
//! linear memory exists only for this).
//!
//! - One memory (exported as `memory`) and a bump allocator `cabi_realloc`. Nothing is freed
//!   one by one: every export adapter resets the bump pointer once it has copied its string
//!   arguments into GC strings, so memory holds at most one call's arguments and result.
//!   (Calls into a component instance aren't reentrant, so no string is in use across calls.)
//! - String encoding: UTF-8 (the component is embedded with `StringEncoding::UTF8`).
//!   Arguments are decoded into `$string` (UTF-16). Results are encoded to UTF-8, with **lone
//!   surrogates replaced by U+FFFD** (WIT strings are Unicode scalar values; JS strings needn't be).
//! - An adapter per such export: flat params `f64` / `(ptr, len)`, then the IR function; a string
//!   result is returned through an 8-byte return area `(ptr, len)` (a string result flattens to
//!   two values, more than the one allowed for flat results).
//! - An export that may throw (T12) also goes through an adapter: it catches the JS exception
//!   and returns `result<T, js-error>` through a return area (see [`adapter`]).

use js2wasi3_ir::{AbiSig, AbiType, ErrorKind};
use wasm_encoder::{BlockType, Catch, Function, InstructionSink, MemArg, ValType};

use crate::host::{self, Import};
use crate::runtime::{self, Cx, G_LOOP_NOW, G_WAIT, G_WAIT_AT, T_STRING};
use crate::{object, promise, timers};

/// First address the allocator hands out (keeps 0 an invalid pointer).
pub const HEAP_BASE: i32 = 8;

/// Canonical ABI: more flat parameters than this are passed in memory instead.
pub const MAX_FLAT_PARAMS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    Realloc,
    Utf8ToString,
    StringToUtf8,
    Utf8Len,
    Utf8Encode,
    /// `()`: a library's `init` (T20, F11), run by the first export call. An `init` that threw
    /// throws its error again on every later call (like a failed module evaluation).
    Init,
    /// `()`: after a library call (T20, F65): run the queued jobs, then throw the reason of the
    /// first rejection nobody handled.
    Settle,
    /// `(promise: jsval) -> jsval`: the end of an async export's call (T20): run the queued jobs,
    /// then give the value the promise settled to. A rejection throws its reason, a promise
    /// still pending throws an `Error` (nothing could settle it later), and then, like
    /// [`Helper::Settle`], another rejection nobody handled throws its reason.
    Await,
    /// `() -> i32`: move an async call forward (T20): run the jobs; done (0) once its promise
    /// settled, it failed, a rejection went unhandled or nothing is left to wait for; else wait
    /// for the earliest timer (backpressure up, so no other call starts meanwhile) and return
    /// the callback code (`WAIT` on the set).
    Step,
    /// `()`: the wait ended: set the loop time and run the due timers; an exception or an
    /// unhandled rejection fails the call.
    Tick,
    /// `(event, index, payload: i32)`: an event for an async call's callback.
    Event,
    /// `()`: an async call is over: cancel the wait, lower the backpressure. Timers left run in
    /// a later async call.
    End,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Realloc => "cabi_realloc",
            Helper::Utf8ToString => "utf8_to_string",
            Helper::StringToUtf8 => "string_to_utf8",
            Helper::Utf8Len => "utf8_len",
            Helper::Utf8Encode => "utf8_encode",
            Helper::Init => "module_init",
            Helper::Settle => "settle_jobs",
            Helper::Await => "await_export",
            Helper::Step => "async_step",
            Helper::Tick => "async_tick",
            Helper::Event => "async_event",
            Helper::End => "async_end",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        match self {
            Helper::Realloc => (vec![I32; 4], vec![I32]),
            Helper::Utf8ToString => (vec![I32, I32], vec![runtime::string_ref()]),
            Helper::StringToUtf8 => (vec![runtime::string_ref()], vec![I32, I32]),
            Helper::Utf8Len => (vec![runtime::string_ref()], vec![I32]),
            Helper::Utf8Encode => (vec![runtime::string_ref(), I32], vec![]),
            Helper::Init | Helper::Settle => (vec![], vec![]),
            Helper::Await => (vec![runtime::jsval()], vec![runtime::jsval()]),
            Helper::Step => (vec![], vec![I32]),
            Helper::Tick | Helper::End => (vec![], vec![]),
            Helper::Event => (vec![I32; 3], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx, bump: u32) -> Function {
        match self {
            Helper::Realloc => realloc(bump),
            Helper::Utf8ToString => utf8_to_string(),
            Helper::StringToUtf8 => {
                let realloc = cx.helper(runtime::Helper::Abi(Helper::Realloc));
                let len = cx.helper(runtime::Helper::Abi(Helper::Utf8Len));
                let encode = cx.helper(runtime::Helper::Abi(Helper::Utf8Encode));
                string_to_utf8(realloc, len, encode)
            }
            Helper::Utf8Len => utf8_len(),
            Helper::Utf8Encode => utf8_encode(),
            Helper::Init => {
                let base = cx.host_globals();
                let (init, fails, jobs) = cx.init_func();
                let settle = jobs.then(|| cx.helper(runtime::Helper::Abi(Helper::Settle)));
                module_init(base, init, fails, settle)
            }
            Helper::Settle => {
                let drain = cx.helper(runtime::Helper::Prom(promise::Helper::Drain));
                let unhandled = cx.helper(runtime::Helper::Prom(promise::Helper::Unhandled));
                let mut f = Function::new([(1, runtime::jsval())]);
                let mut s = f.instructions();
                s.call(drain);
                s.call(unhandled).local_tee(0).ref_is_null();
                s.if_(BlockType::Empty);
                s.return_();
                s.end();
                s.local_get(0).throw(runtime::TAG);
                s.end();
                f
            }
            Helper::Await => {
                let drain = cx.helper(runtime::Helper::Prom(promise::Helper::Drain));
                let unhandled = cx.helper(runtime::Helper::Prom(promise::Helper::Unhandled));
                let new_error = cx.helper(runtime::Helper::Obj(object::Helper::NewError(
                    ErrorKind::Error,
                )));
                let pending = cx.str(
                    "the promise of an async export never settled (nothing is left that could settle it)",
                );
                const PROMISE: u32 = 0;
                const P: u32 = 1;
                const R: u32 = 2;
                let t = runtime::T_PROMISE;
                let mut f = Function::new([
                    (1, runtime::rt(true, runtime::T_PROMISE)),
                    (1, runtime::jsval()),
                ]);
                let mut s = f.instructions();
                s.call(drain);
                // An async function's promise: always a `$promise`.
                s.local_get(PROMISE)
                    .ref_cast_non_null(promise::promise())
                    .local_set(P);
                // The call's result: not an unhandled rejection.
                s.local_get(P)
                    .i32_const(1)
                    .struct_set(t, promise::P_HANDLED);
                s.local_get(P)
                    .struct_get(t, promise::P_STATE)
                    .i32_const(promise::REJECTED)
                    .i32_eq();
                s.if_(BlockType::Empty);
                s.local_get(P)
                    .struct_get(t, promise::P_RESULT)
                    .throw(runtime::TAG);
                s.end();
                s.call(unhandled).local_tee(R).ref_is_null().i32_eqz();
                s.if_(BlockType::Empty);
                s.local_get(R).throw(runtime::TAG);
                s.end();
                s.local_get(P)
                    .struct_get(t, promise::P_STATE)
                    .i32_const(promise::PENDING)
                    .i32_eq();
                s.if_(BlockType::Empty);
                s.global_get(pending)
                    .global_get(runtime::G_UNDEFINED)
                    .call(new_error)
                    .throw(runtime::TAG);
                s.end();
                s.local_get(P).struct_get(t, promise::P_RESULT);
                s.end();
                f
            }
            Helper::Step => step(cx),
            Helper::Tick => {
                let lib = cx.host_globals();
                let im = cx.imports();
                let run_due = cx.helper(runtime::Helper::Timer(timers::Helper::RunDue));
                const R: u32 = 0;
                let mut f = Function::new([(1, runtime::jsval())]);
                let mut s = f.instructions();
                s.call(im.get(Import::ClockNow)).global_set(G_LOOP_NOW);
                s.block(BlockType::Result(runtime::jsval()));
                s.try_table(
                    BlockType::Empty,
                    [Catch::One {
                        tag: runtime::TAG,
                        label: 0,
                    }],
                );
                s.call(run_due).local_tee(R).ref_is_null();
                s.if_(BlockType::Empty);
                s.return_();
                s.end();
                s.local_get(R).br(1);
                s.end();
                s.unreachable();
                s.end();
                s.global_set(lib + G_ERROR);
                s.i32_const(1).global_set(lib + G_FAILED);
                s.end();
                f
            }
            Helper::Event => {
                const EVENT: u32 = 0;
                const INDEX: u32 = 1;
                const PAYLOAD: u32 = 2;
                let im = cx.imports();
                let mut f = Function::new([]);
                if im.list.is_empty() {
                    // No timers: nothing to wait for.
                    f.instructions().end();
                    return f;
                }
                let tick = cx.helper(runtime::Helper::Abi(Helper::Tick));
                let mut s = f.instructions();
                // The wait ended: run the due timers.
                s.local_get(EVENT).i32_const(EVENT_SUBTASK).i32_eq();
                s.local_get(INDEX).global_get(G_WAIT).i32_eq().i32_and();
                s.global_get(G_WAIT).i32_eqz().i32_eqz().i32_and();
                s.local_get(PAYLOAD)
                    .i32_const(SUBTASK_RETURNED)
                    .i32_eq()
                    .i32_and();
                s.if_(BlockType::Empty);
                s.global_get(G_WAIT).call(im.get(Import::SubtaskDrop));
                s.i32_const(0).global_set(G_WAIT);
                s.call(tick);
                s.end();
                s.end();
                f
            }
            Helper::End => {
                let lib = cx.host_globals();
                let im = cx.imports();
                let timers = !im.list.is_empty();
                let disarm = timers.then(|| cx.helper(runtime::Helper::Host(host::Helper::Disarm)));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                if let Some(disarm) = disarm {
                    s.call(disarm);
                    s.global_get(lib + G_BP);
                    s.if_(BlockType::Empty);
                    s.call(im.get(Import::BackpressureDec));
                    s.i32_const(0).global_set(lib + G_BP);
                    s.end();
                }
                s.end();
                f
            }
        }
    }
}

/// Canonical ABI constants (see [`crate::host`]).
const EVENT_SUBTASK: i32 = 1;
const SUBTASK_RETURNED: i32 = 2;
const CALLBACK_EXIT: i32 = 0;
const CALLBACK_WAIT: i32 = 2;

/// `$async_step() -> i32`: see [`Helper::Step`].
fn step(cx: &mut dyn Cx) -> Function {
    let lib = cx.host_globals();
    let im = cx.imports();
    let timers = !im.list.is_empty();
    let drain = cx.helper(runtime::Helper::Prom(promise::Helper::Drain));
    let ticks = cx
        .names("$ticks")
        .then(|| cx.helper(runtime::Helper::Timer(timers::Helper::RunTicks)));
    let unhandled = cx.helper(runtime::Helper::Prom(promise::Helper::Unhandled));
    const D: u32 = 0;
    const ST: u32 = 1;
    const R: u32 = 2;
    let mut f = Function::new([(1, ValType::I64), (1, ValType::I32), (1, runtime::jsval())]);
    let mut s = f.instructions();
    s.loop_(BlockType::Empty);
    {
        // The jobs (and the `nextTick` queue); an exception fails the call.
        s.block(BlockType::Empty);
        s.block(BlockType::Result(runtime::jsval()));
        s.try_table(
            BlockType::Empty,
            [Catch::One {
                tag: runtime::TAG,
                label: 0,
            }],
        );
        s.call(drain);
        if let Some(ticks) = ticks {
            s.call(ticks);
        }
        s.br(2);
        s.end();
        s.unreachable();
        s.end();
        s.global_set(lib + G_ERROR);
        s.i32_const(1).global_set(lib + G_FAILED);
        s.i32_const(0).return_();
        s.end();
        // Done: failed, settled, or a rejection nobody handled.
        s.global_get(lib + G_FAILED);
        s.if_(BlockType::Empty);
        s.i32_const(0).return_();
        s.end();
        s.global_get(lib + G_PROMISE)
            .ref_cast_non_null(promise::promise())
            .struct_get(runtime::T_PROMISE, promise::P_STATE)
            .i32_const(promise::PENDING)
            .i32_ne();
        s.if_(BlockType::Empty);
        s.i32_const(0).return_();
        s.end();
        // (`unhandled` forgets the rejections it looked at: keep the reason.)
        s.call(unhandled).local_tee(R).ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(R).global_set(lib + G_ERROR);
        s.i32_const(1).global_set(lib + G_FAILED);
        s.i32_const(0).return_();
        s.end();
        if timers {
            let next = cx.helper(runtime::Helper::Timer(timers::Helper::Next));
            let disarm = cx.helper(runtime::Helper::Host(host::Helper::Disarm));
            let tick = cx.helper(runtime::Helper::Abi(Helper::Tick));
            // Nothing to wait for: done (the promise never settles).
            s.call(next).local_tee(D).i64_const(0).i64_lt_s();
            s.if_(BlockType::Empty);
            s.i32_const(0).return_();
            s.end();
            // Keep a wait that ends no later than the earliest deadline (like `host::Helper::Loop`).
            s.global_get(G_WAIT).i32_eqz();
            s.global_get(G_WAIT_AT).local_get(D).i64_gt_u();
            s.i32_or();
            s.if_(BlockType::Empty);
            {
                s.call(disarm);
                s.local_get(D).call(im.get(Import::WaitUntil)).local_tee(ST);
                s.i32_const(0xf)
                    .i32_and()
                    .i32_const(SUBTASK_RETURNED)
                    .i32_eq();
                s.if_(BlockType::Empty);
                // It ended at once: run the timers and look again.
                s.call(tick).br(2);
                s.end();
                s.local_get(ST).i32_const(4).i32_shr_u().global_set(G_WAIT);
                s.local_get(D).global_set(G_WAIT_AT);
                s.global_get(lib + G_WS).i32_eqz();
                s.if_(BlockType::Empty);
                s.call(im.get(Import::WaitableSetNew))
                    .global_set(lib + G_WS);
                s.end();
                s.global_get(G_WAIT)
                    .global_get(lib + G_WS)
                    .call(im.get(Import::WaitableJoin));
            }
            s.end();
            // No other call may start while this one waits: they would share the timers.
            s.global_get(lib + G_BP).i32_eqz();
            s.if_(BlockType::Empty);
            s.call(im.get(Import::BackpressureInc));
            s.i32_const(1).global_set(lib + G_BP);
            s.end();
            s.global_get(lib + G_WS)
                .i32_const(4)
                .i32_shl()
                .i32_const(CALLBACK_WAIT)
                .i32_or()
                .return_();
        }
    }
    s.end();
    s.i32_const(0);
    s.end();
    f
}

/// Library globals (after the bump pointer, where a command has its host globals): the `init`
/// state (0: not run, 1: done, 2: threw) and the error it threw; for an async call (T20) its
/// promise, whether it failed and the error, the waitable set (0: none yet) and whether
/// backpressure is raised.
pub const LIB_GLOBALS: u32 = 7;
const G_INIT_STATE: u32 = 0;
const G_INIT_ERROR: u32 = 1;
const G_PROMISE: u32 = 2;
const G_FAILED: u32 = 3;
const G_ERROR: u32 = 4;
const G_WS: u32 = 5;
const G_BP: u32 = 6;

/// The types of the [`LIB_GLOBALS`] (all start as 0 / null).
pub fn lib_global_types() -> [ValType; LIB_GLOBALS as usize] {
    use ValType::I32;
    let v = runtime::jsval();
    [I32, v, v, I32, v, I32, I32]
}

/// What an [`adapter`] does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Call the function, return its result (or `result`).
    Sync,
    /// An async export's first half (T20): run `init`, call the function, keep its promise.
    Start,
    /// Its second half, once [`Helper::Step`] is done: the `result` in a return area.
    Finish,
}

/// `$module_init()`: see [`Helper::Init`]. `settle`: drain the jobs `init` queued.
fn module_init(base: u32, init: u32, fails: bool, settle: Option<u32>) -> Function {
    let mut f = Function::new([(1, runtime::jsval())]);
    let mut s = f.instructions();
    s.global_get(base + G_INIT_STATE).i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    s.return_();
    s.end();
    if !fails {
        s.i32_const(1).global_set(base + G_INIT_STATE);
        s.call(init);
        s.end();
        return f;
    }
    s.global_get(base + G_INIT_STATE).i32_const(2).i32_eq();
    s.if_(BlockType::Empty);
    s.global_get(base + G_INIT_ERROR).throw(runtime::TAG);
    s.end();
    s.i32_const(1).global_set(base + G_INIT_STATE);
    s.block(BlockType::Result(runtime::jsval()));
    s.try_table(
        BlockType::Empty,
        [Catch::One {
            tag: runtime::TAG,
            label: 0,
        }],
    );
    s.call(init);
    if let Some(settle) = settle {
        s.call(settle);
    }
    s.return_();
    s.end();
    s.unreachable();
    s.end();
    s.local_tee(0).global_set(base + G_INIT_ERROR);
    s.i32_const(2).global_set(base + G_INIT_STATE);
    s.local_get(0).throw(runtime::TAG);
    s.end();
    f
}

fn mem(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}

/// Core params of an export with boundary types `params`.
pub fn flat_params(params: &[AbiType]) -> Vec<ValType> {
    params
        .iter()
        .flat_map(|p| match p {
            AbiType::F64 => vec![ValType::F64],
            AbiType::String => vec![ValType::I32, ValType::I32],
        })
        .collect()
}

/// Core result of an export: a string, or a `result` (if it may throw), comes back through a
/// pointer to a return area.
pub fn flat_result(sig: &AbiSig) -> Vec<ValType> {
    match sig.result {
        _ if sig.throws => vec![ValType::I32],
        None => vec![],
        Some(AbiType::F64) => vec![ValType::F64],
        Some(AbiType::String) => vec![ValType::I32],
    }
}

/// The adapter for an export of `func` with signature `sig`.
///
/// If the function may throw (`sig.throws`), the WIT result is `result<T, js-error>`, returned
/// through a return area: discriminant (u8) at 0, payload at the payload's alignment (8 for an
/// `f64`, else 4). The `js-error` payload is `{ name: string, message: string, stack: option<string> }`
/// at offsets 0, 8 and 16 (option discriminant; `stack` is always `none`). An error gives its
/// `name` and `message`; any other thrown value gives `name: ""` and its `console.log` text.
pub fn adapter(cx: &mut dyn Cx, bump: u32, func: u32, sig: &AbiSig, mode: Mode) -> Function {
    let lib = cx.host_globals();
    let utf8_to_string = cx.helper(runtime::Helper::Abi(Helper::Utf8ToString));
    let realloc = cx.helper(runtime::Helper::Abi(Helper::Realloc));
    let to_utf8 = (sig.result == Some(AbiType::String) || sig.throws)
        .then(|| cx.helper(runtime::Helper::Abi(Helper::StringToUtf8)));
    // A library's `init` runs first, its jobs after the call (T20).
    let init = (cx.has_init() && mode != Mode::Finish)
        .then(|| cx.helper(runtime::Helper::Abi(Helper::Init)));
    // An async export's promise is awaited (which runs the jobs), then its value converted.
    let settle =
        (sig.jobs && !sig.is_async).then(|| cx.helper(runtime::Helper::Abi(Helper::Settle)));
    let await_ = sig
        .is_async
        .then(|| cx.helper(runtime::Helper::Abi(Helper::Await)));
    let to_number = (sig.is_async && sig.result == Some(AbiType::F64))
        .then(|| cx.helper(runtime::Helper::ToNumber));
    let value_to_string = (sig.is_async && sig.result == Some(AbiType::String))
        .then(|| cx.helper(runtime::Helper::ToString));
    let nflat = if mode == Mode::Finish {
        0
    } else {
        flat_params(&sig.params).len() as u32
    };
    // Locals after the params: the (ptr, len) of a string, the return area, an f64 result,
    // the thrown value.
    let (ptr, len, ret, num, exc) = (nflat, nflat + 1, nflat + 2, nflat + 3, nflat + 4);
    let mut f = Function::new([(3, ValType::I32), (1, ValType::F64), (1, runtime::jsval())]);
    let mut s = f.instructions();
    // The arguments as the IR function takes them.
    let args = |s: &mut InstructionSink| {
        let mut i = 0;
        for p in &sig.params {
            match p {
                AbiType::F64 => {
                    s.local_get(i);
                    i += 1;
                }
                AbiType::String => {
                    s.local_get(i).local_get(i + 1).call(utf8_to_string);
                    i += 2;
                }
            }
        }
        // The arguments are GC strings now: free the memory they came in.
        s.i32_const(HEAP_BASE).global_set(bump);
    };
    let alloc = |s: &mut InstructionSink, align: i32, size: i32| {
        s.i32_const(0)
            .i32_const(0)
            .i32_const(align)
            .i32_const(size)
            .call(realloc)
            .local_set(ret);
    };
    // Store the string on the stack as (ptr, len) at `ret + offset`.
    let store_string = |s: &mut InstructionSink, to_utf8: u32, offset: u64| {
        s.call(to_utf8).local_set(len).local_set(ptr);
        s.local_get(ret).local_get(ptr).i32_store(mem(offset, 2));
        s.local_get(ret)
            .local_get(len)
            .i32_store(mem(offset + 4, 2));
    };
    if mode == Mode::Start {
        // Start the call; its promise (or the error `init` threw) waits in the library globals.
        s.block(BlockType::Result(runtime::jsval()));
        s.try_table(
            BlockType::Empty,
            [Catch::One {
                tag: runtime::TAG,
                label: 0,
            }],
        );
        if let Some(init) = init {
            s.call(init);
        }
        args(&mut s);
        s.call(func).global_set(lib + G_PROMISE);
        s.i32_const(0).global_set(lib + G_FAILED);
        s.return_();
        s.end();
        s.unreachable();
        s.end();
        s.global_set(lib + G_ERROR);
        s.i32_const(1).global_set(lib + G_FAILED);
        s.end();
        return f;
    }
    if !sig.throws {
        if let Some(init) = init {
            s.call(init);
        }
        args(&mut s);
        s.call(func);
        if let Some(to_utf8) = to_utf8 {
            s.ref_cast_non_null(runtime::string());
            alloc(&mut s, 4, 8);
            store_string(&mut s, to_utf8, 0);
            s.local_get(ret);
        }
        s.end();
        return f;
    }
    let to_utf8 = to_utf8.expect("errors carry strings");
    let describe = cx.helper(runtime::Helper::Describe);
    let get = cx.helper(runtime::Helper::Obj(object::Helper::Get));
    let to_string = cx.helper(runtime::Helper::ToString);
    let empty = cx.str("");
    let (name, message) = (cx.str("name"), cx.str("message"));
    // result<T, js-error>: payload offset (= alignment) and size.
    let payload: u64 = if sig.result == Some(AbiType::F64) {
        8
    } else {
        4
    };
    let size = (payload as i32 + JS_ERROR_SIZE + payload as i32 - 1) & -(payload as i32);
    s.block(BlockType::Result(runtime::jsval()));
    s.try_table(
        BlockType::Empty,
        [Catch::One {
            tag: runtime::TAG,
            label: 0,
        }],
    );
    // Inside the `try_table`, which takes no values from outside.
    if mode == Mode::Finish {
        // The call failed to start (`init` threw), or a job or timer threw.
        s.global_get(lib + G_FAILED);
        s.if_(BlockType::Empty);
        s.global_get(lib + G_ERROR).throw(runtime::TAG);
        s.end();
        s.global_get(lib + G_PROMISE);
    } else {
        if let Some(init) = init {
            s.call(init);
        }
        args(&mut s);
        s.call(func);
    }
    if let Some(await_) = await_ {
        s.call(await_);
        match sig.result {
            None => {
                s.drop();
            }
            Some(AbiType::F64) => {
                s.call(to_number.unwrap());
            }
            Some(AbiType::String) => {
                s.call(value_to_string.unwrap());
            }
        }
    }
    // The result waits in a local while the jobs run.
    let settle = |s: &mut InstructionSink| {
        if let Some(settle) = settle {
            s.call(settle);
        }
    };
    match sig.result {
        None => {
            settle(&mut s);
            alloc(&mut s, payload as i32, size);
        }
        Some(AbiType::F64) => {
            s.local_set(num);
            settle(&mut s);
            alloc(&mut s, payload as i32, size);
            s.local_get(ret).local_get(num).f64_store(mem(payload, 3));
        }
        Some(AbiType::String) => {
            s.local_set(exc);
            settle(&mut s);
            s.local_get(exc).ref_cast_non_null(runtime::string());
            alloc(&mut s, payload as i32, size);
            store_string(&mut s, to_utf8, payload);
        }
    }
    s.local_get(ret).i32_const(0).i32_store8(mem(0, 0));
    s.local_get(ret).return_();
    s.end();
    s.unreachable();
    s.end();
    // Caught: the err case.
    s.local_set(exc);
    alloc(&mut s, payload as i32, size);
    s.local_get(ret).i32_const(1).i32_store8(mem(0, 0));
    object::is_error(&mut s, exc);
    s.if_(BlockType::Result(runtime::string_ref()));
    s.local_get(exc).global_get(name).call(get).call(to_string);
    s.else_();
    s.global_get(empty);
    s.end();
    store_string(&mut s, to_utf8, payload);
    object::is_error(&mut s, exc);
    s.if_(BlockType::Result(runtime::string_ref()));
    s.local_get(exc)
        .global_get(message)
        .call(get)
        .call(to_string);
    s.else_();
    s.local_get(exc).call(describe);
    s.end();
    store_string(&mut s, to_utf8, payload + 8);
    // stack: none
    s.local_get(ret)
        .i32_const(0)
        .i32_store8(mem(payload + 16, 0));
    s.local_get(ret);
    s.end();
    f
}

/// Core params of an async export's `task.return` (T20): its `result<T, js-error>` flattened.
/// The cases' values are joined: `f64` with the first `i32` of `js-error` makes an `i64`.
pub fn task_return_params(sig: &AbiSig) -> Vec<ValType> {
    use ValType::{I32, I64};
    assert!(sig.throws, "async exports return a `result`");
    let first = if sig.result == Some(AbiType::F64) {
        I64
    } else {
        I32
    };
    let mut v = vec![I32, first];
    v.extend([I32; 6]);
    v
}

/// `[async-lift]<name>` (T20): start the call (`start`: the [`Mode::Start`] adapter), then
/// [`Helper::Step`]; when that's done, [`complete`] it.
pub fn async_lift(
    cx: &mut dyn Cx,
    start: u32,
    finish: u32,
    task_return: u32,
    sig: &AbiSig,
) -> Function {
    let step = cx.helper(runtime::Helper::Abi(Helper::Step));
    let end = cx.helper(runtime::Helper::Abi(Helper::End));
    let nflat = flat_params(&sig.params).len() as u32;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    for i in 0..nflat {
        s.local_get(i);
    }
    s.call(start);
    complete(&mut s, step, end, finish, task_return, sig, nflat);
    s.end();
    f
}

/// `[callback][async-lift]<name>`: apply the event ([`Helper::Event`]), then as in [`async_lift`].
pub fn async_callback(cx: &mut dyn Cx, finish: u32, task_return: u32, sig: &AbiSig) -> Function {
    let step = cx.helper(runtime::Helper::Abi(Helper::Step));
    let end = cx.helper(runtime::Helper::Abi(Helper::End));
    let event = cx.helper(runtime::Helper::Abi(Helper::Event));
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(0).local_get(1).local_get(2).call(event);
    complete(&mut s, step, end, finish, task_return, sig, 3);
    s.end();
    f
}

/// Step the call; while it waits, return the callback code. When it's done: the `result` from
/// `finish` (a return area) goes to `task.return` as flat values, the call [`Helper::End`]s, and
/// `EXIT`. Values a case doesn't have are ignored by the lift. Locals `l`, `l + 1`: i32.
fn complete(
    s: &mut InstructionSink,
    step: u32,
    end: u32,
    finish: u32,
    task_return: u32,
    sig: &AbiSig,
    l: u32,
) {
    let (code, ret) = (l, l + 1);
    s.call(step).local_tee(code);
    s.if_(BlockType::Empty);
    s.local_get(code).return_();
    s.end();
    s.call(finish).local_set(ret);
    let payload: u64 = if sig.result == Some(AbiType::F64) {
        8
    } else {
        4
    };
    s.local_get(ret).i32_load8_u(mem(0, 0));
    if sig.result == Some(AbiType::F64) {
        // `ok`: the f64's bits; `err`: `name`'s pointer.
        s.local_get(ret).i64_load(mem(payload, 3));
        s.local_get(ret)
            .i32_load(mem(payload, 2))
            .i64_extend_i32_u();
        s.local_get(ret).i32_load8_u(mem(0, 0)).i32_eqz();
        s.select();
    } else {
        s.local_get(ret).i32_load(mem(payload, 2));
    }
    for off in [4, 8, 12, 16, 20, 24] {
        if off == 16 {
            s.local_get(ret).i32_load8_u(mem(payload + off, 0));
        } else {
            s.local_get(ret).i32_load(mem(payload + off, 2));
        }
    }
    s.call(task_return);
    s.call(end);
    s.i32_const(CALLBACK_EXIT);
}

/// Size of the `js-error` record: two strings and an `option<string>` (u8 + padding + string).
const JS_ERROR_SIZE: i32 = 8 + 8 + 12;

/// `cabi_realloc(old_ptr, old_size, align, new_size) -> ptr`: bump allocation, growing memory.
fn realloc(bump: u32) -> Function {
    const OLD: u32 = 0;
    const OLD_SIZE: u32 = 1;
    const ALIGN: u32 = 2;
    const SIZE: u32 = 3;
    const PTR: u32 = 4;
    const END: u32 = 5;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    // ptr = (bump + align - 1) & -align
    s.global_get(bump)
        .local_get(ALIGN)
        .i32_add()
        .i32_const(1)
        .i32_sub();
    s.i32_const(0)
        .local_get(ALIGN)
        .i32_sub()
        .i32_and()
        .local_tee(PTR);
    s.local_get(SIZE).i32_add().local_tee(END);
    // Grow if end > memory size in bytes.
    s.memory_size(0).i32_const(16).i32_shl().i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(END)
        .memory_size(0)
        .i32_const(16)
        .i32_shl()
        .i32_sub();
    s.i32_const(0xffff).i32_add().i32_const(16).i32_shr_u();
    s.memory_grow(0).i32_const(-1).i32_eq();
    s.if_(BlockType::Empty);
    s.unreachable();
    s.end();
    s.end();
    s.local_get(END).global_set(bump);
    // realloc: copy the old contents
    s.local_get(OLD);
    s.if_(BlockType::Empty);
    s.local_get(PTR).local_get(OLD);
    s.local_get(OLD_SIZE)
        .local_get(SIZE)
        .local_get(OLD_SIZE)
        .local_get(SIZE)
        .i32_lt_u()
        .select();
    s.memory_copy(0, 0);
    s.end();
    s.local_get(PTR);
    s.end();
    f
}

/// `$utf8_to_string(ptr, len) -> (ref $string)`. The input is valid UTF-8 (the host checks).
fn utf8_to_string() -> Function {
    const PTR: u32 = 0;
    const LEN: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const B: u32 = 4;
    const CP: u32 = 5;
    const J: u32 = 6;
    const R: u32 = 7;
    let mut f = Function::new([
        (5, ValType::I32),
        (
            1,
            ValType::Ref(wasm_encoder::RefType {
                nullable: true,
                heap_type: runtime::string(),
            }),
        ),
    ]);
    let mut s = f.instructions();
    let byte = |s: &mut InstructionSink, k: u64| {
        s.local_get(PTR)
            .local_get(I)
            .i32_add()
            .i32_load8_u(mem(k, 0));
    };
    let cont = |s: &mut InstructionSink, k: u64| {
        byte(s, k);
        s.i32_const(0x3f).i32_and();
    };
    let advance = |s: &mut InstructionSink, by: i32| {
        s.local_get(I).i32_const(by).i32_add().local_set(I);
    };
    // Pass 1: count UTF-16 units (4-byte sequences need two).
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    byte(&mut s, 0);
    s.local_set(B);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.local_get(B).i32_const(0x80).i32_lt_u();
    s.if_(BlockType::Empty);
    advance(&mut s, 1);
    s.else_();
    s.local_get(B).i32_const(0xe0).i32_lt_u();
    s.if_(BlockType::Empty);
    advance(&mut s, 2);
    s.else_();
    s.local_get(B).i32_const(0xf0).i32_lt_u();
    s.if_(BlockType::Empty);
    advance(&mut s, 3);
    s.else_();
    advance(&mut s, 4);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    s.end();
    s.end();
    s.br(0).end().end();
    // Pass 2: decode.
    s.local_get(N).array_new_default(T_STRING).local_set(R);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    byte(&mut s, 0);
    s.local_set(B);
    s.local_get(B).i32_const(0x80).i32_lt_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(B).local_set(CP);
        advance(&mut s, 1);
    }
    s.else_();
    s.local_get(B).i32_const(0xe0).i32_lt_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(B)
            .i32_const(0x1f)
            .i32_and()
            .i32_const(6)
            .i32_shl();
        cont(&mut s, 1);
        s.i32_or().local_set(CP);
        advance(&mut s, 2);
    }
    s.else_();
    s.local_get(B).i32_const(0xf0).i32_lt_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(B)
            .i32_const(0x0f)
            .i32_and()
            .i32_const(12)
            .i32_shl();
        cont(&mut s, 1);
        s.i32_const(6).i32_shl().i32_or();
        cont(&mut s, 2);
        s.i32_or().local_set(CP);
        advance(&mut s, 3);
    }
    s.else_();
    {
        // 4 bytes: a surrogate pair. Write the high surrogate here, the low one below.
        s.local_get(B)
            .i32_const(0x07)
            .i32_and()
            .i32_const(18)
            .i32_shl();
        cont(&mut s, 1);
        s.i32_const(12).i32_shl().i32_or();
        cont(&mut s, 2);
        s.i32_const(6).i32_shl().i32_or();
        cont(&mut s, 3);
        s.i32_or().i32_const(0x10000).i32_sub().local_set(CP);
        s.local_get(R).local_get(J);
        s.local_get(CP)
            .i32_const(10)
            .i32_shr_u()
            .i32_const(0xd800)
            .i32_add();
        s.array_set(T_STRING);
        s.local_get(J).i32_const(1).i32_add().local_set(J);
        s.local_get(CP)
            .i32_const(0x3ff)
            .i32_and()
            .i32_const(0xdc00)
            .i32_add()
            .local_set(CP);
        advance(&mut s, 4);
    }
    s.end();
    s.end();
    s.end();
    s.local_get(R)
        .local_get(J)
        .local_get(CP)
        .array_set(T_STRING);
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$string_to_utf8(s) -> (ptr, len)`: encode into freshly allocated memory. Lone surrogates
/// become U+FFFD.
fn string_to_utf8(realloc: u32, utf8_len: u32, utf8_encode: u32) -> Function {
    const S: u32 = 0;
    const LEN: u32 = 1;
    const PTR: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(S).call(utf8_len).local_set(LEN);
    s.i32_const(0)
        .i32_const(0)
        .i32_const(1)
        .local_get(LEN)
        .call(realloc)
        .local_set(PTR);
    s.local_get(S).local_get(PTR).call(utf8_encode);
    s.local_get(PTR).local_get(LEN);
    s.end();
    f
}

/// Push "unit I is a high surrogate followed by a low surrogate" (reads U = s[I]).
fn surrogate_pair(s: &mut InstructionSink, str_: u32, i: u32, n: u32, u: u32) {
    s.local_get(u)
        .i32_const(0xfc00)
        .i32_and()
        .i32_const(0xd800)
        .i32_eq();
    s.local_get(i)
        .i32_const(1)
        .i32_add()
        .local_get(n)
        .i32_lt_u();
    s.i32_and();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(str_)
        .local_get(i)
        .i32_const(1)
        .i32_add()
        .array_get_u(T_STRING);
    s.i32_const(0xfc00).i32_and().i32_const(0xdc00).i32_eq();
    s.else_();
    s.i32_const(0);
    s.end();
}

fn inc(s: &mut InstructionSink, l: u32, by: i32) {
    s.local_get(l).i32_const(by).i32_add().local_set(l);
}

/// `$utf8_len(s) -> i32`: the UTF-8 byte length of `s` (lone surrogates count as U+FFFD).
fn utf8_len() -> Function {
    const S: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const U: u32 = 3;
    const LEN: u32 = 4;
    let mut f = Function::new([(4, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(S).array_len().local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(S)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_set(U);
    s.local_get(U).i32_const(0x80).i32_lt_u();
    s.if_(BlockType::Empty);
    inc(&mut s, LEN, 1);
    s.else_();
    s.local_get(U).i32_const(0x800).i32_lt_u();
    s.if_(BlockType::Empty);
    inc(&mut s, LEN, 2);
    s.else_();
    surrogate_pair(&mut s, S, I, N, U);
    s.if_(BlockType::Empty);
    inc(&mut s, LEN, 4);
    inc(&mut s, I, 1);
    s.else_();
    // Other BMP characters and lone surrogates (→ U+FFFD) are 3 bytes.
    inc(&mut s, LEN, 3);
    s.end();
    s.end();
    s.end();
    inc(&mut s, I, 1);
    s.br(0).end().end();
    s.local_get(LEN);
    s.end();
    f
}

/// `$utf8_encode(s, p)`: write the UTF-8 of `s` at `p` (`$utf8_len(s)` bytes). Lone
/// surrogates become U+FFFD.
fn utf8_encode() -> Function {
    const S: u32 = 0;
    const P: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const U: u32 = 4;
    const CP: u32 = 5;
    let mut f = Function::new([(4, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(S).array_len().local_set(N);
    let store = |s: &mut InstructionSink, value: &dyn Fn(&mut InstructionSink)| {
        s.local_get(P);
        value(s);
        s.i32_store8(mem(0, 0));
        s.local_get(P).i32_const(1).i32_add().local_set(P);
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(S)
        .local_get(I)
        .array_get_u(T_STRING)
        .local_set(U);
    s.local_get(U).i32_const(0x80).i32_lt_u();
    s.if_(BlockType::Empty);
    store(&mut s, &|s| {
        s.local_get(U);
    });
    s.else_();
    s.local_get(U).i32_const(0x800).i32_lt_u();
    s.if_(BlockType::Empty);
    store(&mut s, &|s| {
        s.local_get(U)
            .i32_const(6)
            .i32_shr_u()
            .i32_const(0xc0)
            .i32_or();
    });
    store(&mut s, &|s| {
        s.local_get(U)
            .i32_const(0x3f)
            .i32_and()
            .i32_const(0x80)
            .i32_or();
    });
    s.else_();
    surrogate_pair(&mut s, S, I, N, U);
    s.if_(BlockType::Empty);
    {
        // cp = 0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
        s.local_get(U)
            .i32_const(0xd800)
            .i32_sub()
            .i32_const(10)
            .i32_shl();
        s.local_get(S)
            .local_get(I)
            .i32_const(1)
            .i32_add()
            .array_get_u(T_STRING);
        s.i32_const(0xdc00).i32_sub().i32_add();
        s.i32_const(0x10000).i32_add().local_set(CP);
        store(&mut s, &|s| {
            s.local_get(CP)
                .i32_const(18)
                .i32_shr_u()
                .i32_const(0xf0)
                .i32_or();
        });
        store(&mut s, &|s| {
            s.local_get(CP)
                .i32_const(12)
                .i32_shr_u()
                .i32_const(0x3f)
                .i32_and()
                .i32_const(0x80)
                .i32_or();
        });
        store(&mut s, &|s| {
            s.local_get(CP)
                .i32_const(6)
                .i32_shr_u()
                .i32_const(0x3f)
                .i32_and()
                .i32_const(0x80)
                .i32_or();
        });
        store(&mut s, &|s| {
            s.local_get(CP)
                .i32_const(0x3f)
                .i32_and()
                .i32_const(0x80)
                .i32_or();
        });
        inc(&mut s, I, 1);
    }
    s.else_();
    {
        // A lone surrogate becomes U+FFFD.
        s.local_get(U)
            .i32_const(0xf800)
            .i32_and()
            .i32_const(0xd800)
            .i32_eq();
        s.if_(BlockType::Empty);
        s.i32_const(0xfffd).local_set(U);
        s.end();
        store(&mut s, &|s| {
            s.local_get(U)
                .i32_const(12)
                .i32_shr_u()
                .i32_const(0xe0)
                .i32_or();
        });
        store(&mut s, &|s| {
            s.local_get(U)
                .i32_const(6)
                .i32_shr_u()
                .i32_const(0x3f)
                .i32_and()
                .i32_const(0x80)
                .i32_or();
        });
        store(&mut s, &|s| {
            s.local_get(U)
                .i32_const(0x3f)
                .i32_and()
                .i32_const(0x80)
                .i32_or();
        });
    }
    s.end();
    s.end();
    s.end();
    inc(&mut s, I, 1);
    s.br(0).end().end();
    s.end();
    f
}
