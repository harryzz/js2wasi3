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

use js2wasi3_ir::{AbiSig, AbiType, ErrType, ErrorKind, Type, WitType};
use wasm_encoder::{BlockType, Catch, Function, InstructionSink, MemArg, ValType};

use crate::host::{self, Import};
use crate::runtime::{self, Cx, G_LOOP_NOW, G_WAIT, G_WAIT_AT, T_STRING};
use crate::{array, object, promise, timers, wit};

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
    /// `(cp: i32) -> (ref $string)`: a `char` parameter as a one-character string (T20).
    FromCodePoint,
    /// `(s: (ref $string)) -> i32`: a `char` result: the first code point of `s` (a lone
    /// surrogate gives U+FFFD); traps on the empty string.
    CodePointAt0,
    /// `(args: jsval) -> jsval`: call the user world's import `i` (T20 slice 5, see
    /// [`import_adapter`]).
    Import(u32),
    /// `(promise: jsval, ret: i32)`: the user world's `async func` import `i` returned (T20
    /// slice 5b): lift its result from the return area `ret` and settle the call's promise.
    ImportDone(u32),
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
            Helper::FromCodePoint => "from_code_point",
            Helper::CodePointAt0 => "code_point_at_0",
            Helper::Import(_) => "call_import",
            Helper::ImportDone(_) => "import_done",
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
            Helper::FromCodePoint => (vec![I32], vec![runtime::string_ref()]),
            Helper::CodePointAt0 => (vec![runtime::string_ref()], vec![I32]),
            Helper::Import(_) => (vec![runtime::jsval()], vec![runtime::jsval()]),
            Helper::ImportDone(_) => (vec![runtime::jsval(), I32], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx, bump: u32) -> Function {
        match self {
            Helper::Realloc => realloc(bump),
            Helper::Import(i) => import_adapter(cx, bump, i),
            Helper::ImportDone(i) => import_done(cx, i),
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
            Helper::FromCodePoint => from_code_point(),
            Helper::CodePointAt0 => code_point_at_0(),
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
                const ENTRY: u32 = 3;
                let lib = cx.host_globals();
                let im = cx.imports();
                let timers = im.list.contains(&Import::ClockNow);
                let asyncs: Vec<u32> = (0..cx.user_import_count())
                    .filter(|i| cx.user_import(*i).1.is_async)
                    .collect();
                let mut f = Function::new([(1, runtime::jsval())]);
                if !timers && asyncs.is_empty() {
                    // Nothing to wait for.
                    f.instructions().end();
                    return f;
                }
                let tick = timers.then(|| cx.helper(runtime::Helper::Abi(Helper::Tick)));
                let get = cx.helper(runtime::Helper::Obj(object::Helper::Get));
                let set = cx.helper(runtime::Helper::Obj(object::Helper::Set));
                let boxed = cx.helper(runtime::Helper::Box);
                let to_number = cx.helper(runtime::Helper::ToNumber);
                let dones: Vec<(u32, u32)> = asyncs
                    .iter()
                    .map(|i| (*i, cx.helper(runtime::Helper::Abi(Helper::ImportDone(*i)))))
                    .collect();
                let mut s = f.instructions();
                if let Some(tick) = tick {
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
                    s.return_();
                    s.end();
                }
                if !dones.is_empty() {
                    // An `async func` import returned (T20 slice 5b): settle its promise.
                    let key = |s: &mut InstructionSink| {
                        s.global_get(lib + G_PENDING);
                        s.local_get(INDEX).f64_convert_i32_u().call(boxed);
                    };
                    let field = |s: &mut InstructionSink, k: f64| {
                        s.local_get(ENTRY).f64_const(k.into()).call(boxed).call(get);
                    };
                    s.local_get(EVENT).i32_const(EVENT_SUBTASK).i32_eq();
                    s.local_get(PAYLOAD)
                        .i32_const(SUBTASK_RETURNED)
                        .i32_eq()
                        .i32_and();
                    s.global_get(lib + G_PENDING).ref_is_null().i32_eqz().i32_and();
                    s.if_(BlockType::Empty);
                    key(&mut s);
                    s.call(get).local_tee(ENTRY);
                    s.global_get(runtime::G_UNDEFINED).ref_eq().i32_eqz();
                    s.if_(BlockType::Empty);
                    key(&mut s);
                    s.global_get(runtime::G_UNDEFINED).call(set);
                    s.global_get(lib + G_PENDING_N).i32_const(1).i32_sub();
                    s.global_set(lib + G_PENDING_N);
                    s.local_get(INDEX).call(im.get(Import::SubtaskDrop));
                    for (i, done) in dones {
                        field(&mut s, 2.0);
                        s.call(to_number).f64_const((i as f64).into()).f64_eq();
                        s.if_(BlockType::Empty);
                        field(&mut s, 0.0);
                        field(&mut s, 1.0);
                        s.call(to_number).i32_trunc_sat_f64_u();
                        s.call(done);
                        s.end();
                    }
                    s.end();
                    s.end();
                }
                s.end();
                f
            }
            Helper::End => {
                let lib = cx.host_globals();
                let im = cx.imports();
                let timers = im.list.contains(&Import::ClockNow);
                let disarm = timers.then(|| cx.helper(runtime::Helper::Host(host::Helper::Disarm)));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                if let Some(disarm) = disarm {
                    s.call(disarm);
                }
                if !im.list.is_empty() {
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

/// `$from_code_point(cp) -> (ref $string)`: see [`Helper::FromCodePoint`].
fn from_code_point() -> Function {
    const CP: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(CP).i32_const(0x10000).i32_lt_u();
    s.if_(BlockType::Result(runtime::string_ref()));
    s.local_get(CP).array_new_fixed(T_STRING, 1);
    s.else_();
    // A surrogate pair.
    s.local_get(CP)
        .i32_const(0x10000)
        .i32_sub()
        .i32_const(10)
        .i32_shr_u()
        .i32_const(0xd800)
        .i32_add();
    s.local_get(CP)
        .i32_const(0x3ff)
        .i32_and()
        .i32_const(0xdc00)
        .i32_add();
    s.array_new_fixed(T_STRING, 2);
    s.end();
    s.end();
    f
}

/// `$code_point_at_0(s) -> i32`: see [`Helper::CodePointAt0`].
fn code_point_at_0() -> Function {
    const S: u32 = 0;
    const U: u32 = 1;
    const V: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(S)
        .i32_const(0)
        .array_get_u(T_STRING)
        .local_tee(U);
    // Not a surrogate.
    s.i32_const(0xf800).i32_and().i32_const(0xd800).i32_ne();
    s.if_(BlockType::Empty);
    s.local_get(U).return_();
    s.end();
    // A high surrogate followed by a low one.
    s.local_get(U).i32_const(0xdc00).i32_lt_u();
    s.local_get(S).array_len().i32_const(1).i32_gt_u();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(S)
        .i32_const(1)
        .array_get_u(T_STRING)
        .local_tee(V);
    s.i32_const(0xfc00).i32_and().i32_const(0xdc00).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(U)
        .i32_const(0xd800)
        .i32_sub()
        .i32_const(10)
        .i32_shl();
    s.local_get(V).i32_const(0xdc00).i32_sub();
    s.i32_add().i32_const(0x10000).i32_add().return_();
    s.end();
    s.end();
    s.i32_const(0xfffd);
    s.end();
    f
}

/// Core params and results of a user world's import (T20 slice 5, a lowered function): its
/// params flattened (past 16 values a pointer to them), its result flattened (past 1 value a
/// pointer to a return area, as one more param).
pub fn import_type(sig: &AbiSig) -> (Vec<ValType>, Vec<ValType>) {
    let mut params = wit::flatten(&import_params(sig));
    if params.len() > max_flat_import_params(sig) {
        params = vec![ValType::I32];
    }
    if sig.is_async {
        // `[async-lower]`: a pointer to the return area, and the subtask's status.
        if sig.result.is_some() {
            params.push(ValType::I32);
        }
        return (params, vec![ValType::I32]);
    }
    let results = sig.result.map_or(Vec::new(), |t| wit::flatten(&sig.wit(t)));
    if results.len() > 1 {
        params.push(ValType::I32);
        return (params, Vec::new());
    }
    (params, results)
}

/// Canonical ABI: an async-lowered import takes at most 4 flat params (else a pointer).
fn max_flat_import_params(sig: &AbiSig) -> usize {
    if sig.is_async { 4 } else { MAX_FLAT_PARAMS }
}

/// An import's params as one tuple.
fn import_params(sig: &AbiSig) -> WitType {
    WitType::Tuple(sig.params.iter().map(|p| sig.wit(*p)).collect())
}

/// `$call_import(args) -> jsval` (T20 slice 5): call the user world's import `i` with the JS
/// values in the array `args` (missing ones `undefined`). They are stored as the params' tuple
/// with [`wit::lower_mem`] (so they convert like an export's result, and a value that doesn't
/// fit traps), then passed flat or by pointer; the result is lifted to a JS value. The memory
/// used (arguments, the return area, strings and lists the callee allocates) is freed after the
/// call: the bump pointer goes back to where it was.
fn import_adapter(cx: &mut dyn Cx, bump: u32, i: u32) -> Function {
    let (func, sig, _) = cx.user_import(i);
    let realloc = cx.helper(runtime::Helper::Abi(Helper::Realloc));
    let tuple = import_params(&sig);
    let result = sig.result.map(|t| sig.wit(t));
    let mut g = wit::Gen::new(1);
    let (saved, area, ret) = (
        g.local(ValType::I32),
        g.local(ValType::I32),
        g.local(ValType::I32),
    );
    let mut body = Vec::new();
    let mut s = InstructionSink::new(&mut body);
    let alloc = |s: &mut InstructionSink, t: &WitType, to: u32| {
        let (size, align) = wit::size_align(t);
        s.i32_const(0).i32_const(0).i32_const(align as i32).i32_const(size as i32);
        s.call(realloc).local_set(to);
    };
    s.global_get(bump).local_set(saved);
    let flat = wit::flatten(&tuple);
    if !flat.is_empty() {
        alloc(&mut s, &tuple, area);
        wit::lower_mem(cx, &mut g, &mut s, &tuple, 0, area, 0);
        if flat.len() > max_flat_import_params(&sig) {
            s.local_get(area);
        } else {
            wit::flat_load(&mut g, &mut s, &tuple, area, 0);
        }
    }
    if sig.is_async {
        import_start(cx, &mut g, &mut s, i, func, result.as_ref(), ret);
        s.end();
        let mut f = Function::new(g.decls());
        f.raw(body);
        return f;
    }
    let rflat = result.as_ref().map_or(Vec::new(), wit::flatten);
    match &result {
        Some(t) if rflat.len() > 1 => {
            alloc(&mut s, t, ret);
            s.local_get(ret).call(func);
            wit::lift_mem(cx, &mut g, &mut s, t, ret, 0);
        }
        Some(t) => {
            s.call(func);
            let slots: Vec<(u32, ValType)> = rflat.iter().map(|v| (g.local(*v), *v)).collect();
            for (l, _) in slots.iter().rev() {
                s.local_set(*l);
            }
            wit::lift_flat(cx, &mut g, &mut s, t, &slots);
        }
        None => {
            s.call(func).global_get(runtime::G_UNDEFINED);
        }
    }
    s.local_get(saved).global_set(bump);
    s.end();
    let mut f = Function::new(g.decls());
    f.raw(body);
    f
}

/// The rest of an `async func` import's adapter (T20 slice 5b), its arguments on the stack:
/// pass a return area, start the subtask and give a promise. It's settled at once if the
/// subtask returned, else when [`Helper::Event`] sees it return: until then the subtask is in
/// the call's waitable set and in the pending table (`handle` → `[promise, ret, i]`), and the
/// arguments and return area stay allocated (the next export call frees them).
fn import_start(
    cx: &mut dyn Cx,
    g: &mut wit::Gen,
    s: &mut InstructionSink,
    i: u32,
    func: u32,
    result: Option<&WitType>,
    ret: u32,
) {
    let lib = cx.host_globals();
    let im = cx.imports();
    let realloc = cx.helper(runtime::Helper::Abi(Helper::Realloc));
    let new_promise = cx.helper(runtime::Helper::Prom(promise::Helper::New));
    let done = cx.helper(runtime::Helper::Abi(Helper::ImportDone(i)));
    let set = cx.helper(runtime::Helper::Obj(object::Helper::Set));
    let boxed = cx.helper(runtime::Helper::Box);
    let new_array = cx.helper(runtime::Helper::Arr(array::Helper::New));
    let (st, p) = (g.local(ValType::I32), g.local(runtime::jsval()));
    if let Some(t) = result {
        let (size, align) = wit::size_align(t);
        s.i32_const(0).i32_const(0).i32_const(align as i32).i32_const(size as i32);
        s.call(realloc).local_tee(ret);
    }
    s.call(func).local_set(st);
    s.call(new_promise).local_set(p);
    s.local_get(st).i32_const(0xf).i32_and().i32_const(SUBTASK_RETURNED).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(p).local_get(ret).call(done);
    s.else_();
    {
        s.global_get(lib + G_WS).i32_eqz();
        s.if_(BlockType::Empty);
        s.call(im.get(Import::WaitableSetNew)).global_set(lib + G_WS);
        s.end();
        s.local_get(st).i32_const(4).i32_shr_u();
        s.global_get(lib + G_WS).call(im.get(Import::WaitableJoin));
        s.global_get(lib + G_PENDING).ref_is_null();
        s.if_(BlockType::Empty);
        s.array_new_fixed(runtime::T_VALS, 0).i32_const(0).call(new_array);
        s.global_set(lib + G_PENDING);
        s.end();
        s.global_get(lib + G_PENDING);
        s.local_get(st).i32_const(4).i32_shr_u().f64_convert_i32_u().call(boxed);
        s.local_get(p);
        s.local_get(ret).f64_convert_i32_u().call(boxed);
        s.f64_const((i as f64).into()).call(boxed);
        s.array_new_fixed(runtime::T_VALS, 3).i32_const(3).call(new_array);
        s.call(set);
        s.global_get(lib + G_PENDING_N).i32_const(1).i32_add();
        s.global_set(lib + G_PENDING_N);
    }
    s.end();
    s.local_get(p);
}

/// `$import_done(promise, ret)`: see [`Helper::ImportDone`]. The result is lifted from the
/// return area; a top-level `result` goes through the import's unwrap function: the promise is
/// fulfilled with what it returns, rejected with what it throws.
fn import_done(cx: &mut dyn Cx, i: u32) -> Function {
    let (_, sig, unwrap) = cx.user_import(i);
    let resolve = cx.helper(runtime::Helper::Prom(promise::Helper::Resolve));
    let reject = cx.helper(runtime::Helper::Prom(promise::Helper::Settle { rejected: true }));
    const P: u32 = 0;
    const RET: u32 = 1;
    let mut g = wit::Gen::new(2);
    let v = g.local(runtime::jsval());
    let mut body = Vec::new();
    let mut s = InstructionSink::new(&mut body);
    match sig.result.map(|t| sig.wit(t)) {
        Some(t) => wit::lift_mem(cx, &mut g, &mut s, &t, RET, 0),
        None => {
            s.global_get(runtime::G_UNDEFINED);
        }
    }
    s.local_set(v);
    if let Some(unwrap) = unwrap {
        s.block(BlockType::Result(runtime::jsval()));
        s.try_table(
            BlockType::Empty,
            [Catch::One {
                tag: runtime::TAG,
                label: 0,
            }],
        );
        s.local_get(v).call(unwrap).local_set(v);
        s.local_get(P).ref_cast_non_null(promise::promise());
        s.local_get(v).call(resolve);
        s.return_();
        s.end();
        s.unreachable();
        s.end();
        s.local_set(v);
        s.local_get(P).ref_cast_non_null(promise::promise());
        s.local_get(v).call(reject);
    } else {
        s.local_get(P).ref_cast_non_null(promise::promise());
        s.local_get(v).call(resolve);
    }
    s.end();
    let mut f = Function::new(g.decls());
    f.raw(body);
    f
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
    let timers = im.list.contains(&Import::ClockNow);
    // Something to wait for: timers, or (T20 slice 5b) `async func` imports.
    let waits = !im.list.is_empty();
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
        // Nothing to wait for (no timer, no import pending): done (the promise never settles).
        let idle = |s: &mut InstructionSink| {
            s.global_get(lib + G_PENDING_N).i32_eqz();
            s.if_(BlockType::Empty);
            s.i32_const(0).return_();
            s.end();
        };
        if waits && !timers {
            idle(&mut s);
        }
        if timers {
            let next = cx.helper(runtime::Helper::Timer(timers::Helper::Next));
            let disarm = cx.helper(runtime::Helper::Host(host::Helper::Disarm));
            let tick = cx.helper(runtime::Helper::Abi(Helper::Tick));
            s.call(next).local_tee(D).i64_const(0).i64_lt_s();
            s.if_(BlockType::Empty);
            idle(&mut s);
            s.else_();
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
                s.call(tick).br(3);
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
            s.end();
        }
        if waits {
            // No other call may start while this one waits: they would share the timers, the
            // pending imports and the memory they use.
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
/// backpressure is raised; the pending `async func` imports (slice 5b).
pub const LIB_GLOBALS: u32 = 9;
const G_INIT_STATE: u32 = 0;
const G_INIT_ERROR: u32 = 1;
const G_PROMISE: u32 = 2;
const G_FAILED: u32 = 3;
const G_ERROR: u32 = 4;
const G_WS: u32 = 5;
const G_BP: u32 = 6;
/// The pending `async func` imports (T20 slice 5b): subtask handle → `[promise, ret, import]`
/// (a JS array, made on first use), and how many there are.
const G_PENDING: u32 = 7;
const G_PENDING_N: u32 = 8;

/// The types of the [`LIB_GLOBALS`] (all start as 0 / null).
pub fn lib_global_types() -> [ValType; LIB_GLOBALS as usize] {
    use ValType::I32;
    let v = runtime::jsval();
    [I32, v, v, I32, v, I32, I32, v, I32]
}

/// What an [`adapter`] does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Call the function, return its result (or `result`).
    Sync,
    /// An async export's first half (T20): run `init`, call the function, keep its promise.
    Start,
    /// Its second half, once [`Helper::Step`] is done: the result goes to `task.return`
    /// (function index), as flat values (loaded from the return area) or past 16 a pointer.
    Finish(u32),
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

/// Core values of one scalar boundary type (Canonical ABI flattening).
pub(crate) fn flat(t: AbiType) -> Vec<ValType> {
    use ValType::{F32, F64, I32, I64};
    match t {
        AbiType::S64 | AbiType::U64 => vec![I64],
        AbiType::F32 => vec![F32],
        AbiType::F64 => vec![F64],
        AbiType::String => vec![I32, I32],
        AbiType::Compound(_) => unreachable!("compound types flatten with `wit::flatten`"),
        _ => vec![I32],
    }
}

/// Size and alignment of a scalar boundary type in memory.
pub(crate) fn size_align(t: AbiType) -> (u64, u64) {
    match t {
        AbiType::Bool | AbiType::S8 | AbiType::U8 => (1, 1),
        AbiType::S16 | AbiType::U16 => (2, 2),
        AbiType::S64 | AbiType::U64 | AbiType::F64 => (8, 8),
        AbiType::String => (8, 4),
        AbiType::Compound(_) => unreachable!("compound types: `wit::size_align`"),
        _ => (4, 4),
    }
}

/// Does an export's adapter use linear memory (strings, a `result`, compound types)?
pub fn needs_memory(sig: &AbiSig) -> bool {
    let mem = |t: &AbiType| matches!(t, AbiType::String | AbiType::Compound(_));
    params_in_memory(sig)
        || sig.throws
        || sig.params.iter().any(mem)
        || sig.result.as_ref().is_some_and(mem)
}

/// The flat values of an export's params (before the 16-value limit).
fn flat_param_values(sig: &AbiSig) -> Vec<ValType> {
    sig.params
        .iter()
        .flat_map(|p| wit::flatten(&sig.wit(*p)))
        .collect()
}

/// Core params of an export: past 16 flat values, a pointer to them in memory (a tuple).
pub fn flat_params(sig: &AbiSig) -> Vec<ValType> {
    let flat = flat_param_values(sig);
    if flat.len() > MAX_FLAT_PARAMS {
        vec![ValType::I32]
    } else {
        flat
    }
}

/// Do an export's params come in memory (more than 16 flat values)?
fn params_in_memory(sig: &AbiSig) -> bool {
    flat_param_values(sig).len() > MAX_FLAT_PARAMS
}

/// The `js-error` record of a generated world's throwing export.
fn js_error() -> WitType {
    let string = || WitType::Scalar(AbiType::String);
    WitType::Record(vec![
        ("name".into(), string()),
        ("message".into(), string()),
        ("stack".into(), WitType::Option(Box::new(string()))),
    ])
}

/// The whole WIT result type of an export (a `result<T, E>` if it throws).
fn result_type(sig: &AbiSig) -> Option<WitType> {
    let ok = sig.result.map(|t| sig.wit(t));
    if !sig.throws {
        return ok;
    }
    let err = match sig.err {
        ErrType::JsError => Some(js_error()),
        ErrType::Empty => None,
        ErrType::Of(t) => Some(sig.wit(t)),
    };
    Some(WitType::Result {
        ok: ok.map(Box::new),
        err: err.map(Box::new),
    })
}

/// Core result of an export: one flat value, or a pointer to a return area.
pub fn flat_result(sig: &AbiSig) -> Vec<ValType> {
    match result_type(sig).map(|t| wit::flatten(&t)) {
        None => vec![],
        Some(f) if f.len() <= 1 => f,
        Some(_) => vec![ValType::I32],
    }
}

/// The adapter for an export of `func` with signature `sig`.
///
/// Scalar parameters arrive as core values and are passed as [`AbiType::param_type`]s (integers
/// and `f32` as numbers, a `char` as a one-character string); compound ones as JS values
/// ([`wit::lift_flat`]). A scalar result comes back as an [`AbiType::result_type`] and is
/// narrowed to the WIT type (integers wrap like `ToInt32` and then to their width, 64-bit ones
/// saturate, a `char` is the first code point of its string); a compound one as a JS value,
/// stored with [`wit::lower_mem`].
///
/// If the function may throw (`sig.throws`), the WIT result is `result<T, E>`, returned through
/// a return area (discriminant u8 at 0, the payload at its alignment). A generated world's `E` is
/// `js-error`: `{ name: string, message: string, stack: option<string> }` from the exception (an
/// error gives its `name` and `message`; any other thrown value gives `name: ""` and its
/// `console.log` text; `stack` is `none`). A user world's `E` is the thrown value's own `payload`
/// property if it has one, else the thrown value unless it's an Error (then the call traps), as
/// in jco (T20). Without `throws` an exception (or an unhandled rejection) traps.
pub fn adapter(cx: &mut dyn Cx, bump: u32, func: u32, sig: &AbiSig, mode: Mode) -> Function {
    let lib = cx.host_globals();
    let memory = needs_memory(sig);
    let has = |f: &dyn Fn(&AbiType) -> bool| sig.params.iter().any(f);
    let utf8_to_string = has(&|p| *p == AbiType::String)
        .then(|| cx.helper(runtime::Helper::Abi(Helper::Utf8ToString)));
    let from_code_point = has(&|p| *p == AbiType::Char)
        .then(|| cx.helper(runtime::Helper::Abi(Helper::FromCodePoint)));
    let realloc = memory.then(|| cx.helper(runtime::Helper::Abi(Helper::Realloc)));
    let to_utf8 = (sig.result == Some(AbiType::String) || sig.throws)
        .then(|| cx.helper(runtime::Helper::Abi(Helper::StringToUtf8)));
    // A library's `init` runs first, its jobs after the call (T20).
    let init = (cx.has_init() && !matches!(mode, Mode::Finish(_)))
        .then(|| cx.helper(runtime::Helper::Abi(Helper::Init)));
    // An async export's promise is awaited (which runs the jobs), then its value converted.
    let settle =
        (sig.jobs && !sig.is_async).then(|| cx.helper(runtime::Helper::Abi(Helper::Settle)));
    let await_ = sig
        .is_async
        .then(|| cx.helper(runtime::Helper::Abi(Helper::Await)));
    // What converting the settled value (async) and the result needs.
    let settled = if sig.is_async {
        sig.result.map(|t| t.result_type())
    } else {
        None
    };
    let compound_result = matches!(sig.result, Some(AbiType::Compound(_)));
    let to_number = matches!(settled, Some(Type::F64 | Type::I32))
        .then(|| cx.helper(runtime::Helper::ToNumber));
    let to_int32 = (settled == Some(Type::I32)).then(|| cx.to_int32());
    let to_boolean = (settled == Some(Type::Bool)).then(|| cx.helper(runtime::Helper::ToBoolean));
    let value_to_string = ((settled == Some(Type::JsVal) && !compound_result) || sig.throws)
        .then(|| cx.helper(runtime::Helper::ToString));
    let code_point = (sig.result == Some(AbiType::Char))
        .then(|| cx.helper(runtime::Helper::Abi(Helper::CodePointAt0)));
    let nflat = if matches!(mode, Mode::Finish(_)) {
        0
    } else {
        flat_params(sig).len() as u32
    };
    let mut g = wit::Gen::new(nflat);
    // The (ptr, len) of a string, the return area, the result as an i32 / i64 / f32 / f64 /
    // JS value, the thrown value.
    let (ptr, len, ret, vi32) = (
        g.local(ValType::I32),
        g.local(ValType::I32),
        g.local(ValType::I32),
        g.local(ValType::I32),
    );
    let (vi64, vf32, vf64) = (
        g.local(ValType::I64),
        g.local(ValType::F32),
        g.local(ValType::F64),
    );
    let (vstr, exc) = (g.local(runtime::jsval()), g.local(runtime::jsval()));
    let mut body = Vec::new();
    let mut s = InstructionSink::new(&mut body);
    // The arguments as the IR function takes them.
    let args = |cx: &mut dyn Cx, g: &mut wit::Gen, s: &mut InstructionSink| {
        // The flat values: the core params, or past 16 locals loaded from the tuple they're in.
        let values = flat_param_values(sig);
        let slots: Vec<u32> = if params_in_memory(sig) {
            let tuple = WitType::Tuple(sig.params.iter().map(|p| sig.wit(*p)).collect());
            let at = g.local(ValType::I32);
            s.local_get(0).local_set(at);
            wit::flat_load(g, s, &tuple, at, 0);
            let slots: Vec<u32> = values.iter().map(|v| g.local(*v)).collect();
            for l in slots.iter().rev() {
                s.local_set(*l);
            }
            slots
        } else {
            (0..values.len() as u32).collect()
        };
        let mut i = 0;
        for p in &sig.params {
            if let AbiType::Compound(_) = p {
                let t = sig.wit(*p);
                let n = wit::flatten(&t).len();
                let slots: Vec<(u32, ValType)> =
                    (i..i + n).map(|j| (slots[j], values[j])).collect();
                wit::lift_flat(cx, g, s, &t, &slots);
                i += n;
                continue;
            }
            s.local_get(slots[i]);
            i += 1;
            match p {
                AbiType::Bool | AbiType::F64 => {}
                AbiType::S8 | AbiType::S16 | AbiType::S32 => {
                    s.f64_convert_i32_s();
                }
                AbiType::U8 | AbiType::U16 | AbiType::U32 => {
                    s.f64_convert_i32_u();
                }
                AbiType::S64 => {
                    s.f64_convert_i64_s();
                }
                AbiType::U64 => {
                    s.f64_convert_i64_u();
                }
                AbiType::F32 => {
                    s.f64_promote_f32();
                }
                AbiType::Char => {
                    s.call(from_code_point.unwrap());
                }
                AbiType::String => {
                    s.local_get(slots[i]).call(utf8_to_string.unwrap());
                    i += 1;
                }
                AbiType::Compound(_) => unreachable!(),
            }
        }
        // The arguments are GC strings now: free the memory they came in.
        if memory {
            s.i32_const(HEAP_BASE).global_set(bump);
        }
    };
    let alloc = |s: &mut InstructionSink, align: i32, size: i32| {
        s.i32_const(0)
            .i32_const(0)
            .i32_const(align)
            .i32_const(size)
            .call(realloc.expect("strings and results need memory"))
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
    let finish = |g: wit::Gen, body: Vec<u8>| {
        let mut f = Function::new(g.decls());
        f.raw(body);
        f
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
        args(cx, &mut g, &mut s);
        s.call(func).global_set(lib + G_PROMISE);
        s.i32_const(0).global_set(lib + G_FAILED);
        s.return_();
        s.end();
        s.unreachable();
        s.end();
        s.global_set(lib + G_ERROR);
        s.i32_const(1).global_set(lib + G_FAILED);
        s.end();
        return finish(g, body);
    }
    // Run the call; its result (the WIT type's core value, or a compound's JS value) goes to
    // a local.
    let produce = |cx: &mut dyn Cx, g: &mut wit::Gen, s: &mut InstructionSink| {
        if matches!(mode, Mode::Finish(_)) {
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
            args(cx, g, s);
            s.call(func);
        }
        if let Some(await_) = await_ {
            // The settled value (a `jsval`) as the result type.
            s.call(await_);
            match sig.result.map(|t| (t, t.result_type())) {
                None => {
                    s.drop();
                }
                Some((_, Type::F64)) => {
                    s.call(to_number.unwrap());
                }
                Some((_, Type::I32)) => {
                    s.call(to_number.unwrap()).call(to_int32.unwrap());
                }
                Some((_, Type::Bool)) => {
                    s.call(to_boolean.unwrap());
                }
                Some((AbiType::Compound(_), _)) => {}
                Some(_) => {
                    s.call(value_to_string.unwrap());
                }
            }
        }
        let Some(t) = sig.result else { return };
        match t {
            AbiType::Bool | AbiType::S32 | AbiType::U32 => {}
            AbiType::S8 => {
                s.i32_extend8_s();
            }
            AbiType::U8 => {
                s.i32_const(0xff).i32_and();
            }
            AbiType::S16 => {
                s.i32_extend16_s();
            }
            AbiType::U16 => {
                s.i32_const(0xffff).i32_and();
            }
            AbiType::S64 => {
                s.i64_trunc_sat_f64_s();
            }
            AbiType::U64 => {
                s.i64_trunc_sat_f64_u();
            }
            AbiType::F32 => {
                s.f32_demote_f64();
            }
            AbiType::F64 | AbiType::String | AbiType::Compound(_) => {}
            AbiType::Char => {
                s.ref_cast_non_null(runtime::string())
                    .call(code_point.unwrap());
            }
        }
        s.local_set(match t {
            AbiType::String | AbiType::Compound(_) => vstr,
            _ => match flat(t)[0] {
                ValType::I64 => vi64,
                ValType::F32 => vf32,
                ValType::F64 => vf64,
                _ => vi32,
            },
        });
    };
    let settle = |s: &mut InstructionSink| {
        if let Some(settle) = settle {
            s.call(settle);
        }
    };
    let full = result_type(sig);
    // The result in the return area: its one flat value, or the pointer. An async call's
    // (`Mode::Finish`) goes to `task.return`: flat up to 16 values.
    let task_return = match mode {
        Mode::Finish(f) => Some(f),
        _ => None,
    };
    let deliver = |g: &mut wit::Gen, s: &mut InstructionSink| {
        let t = full.as_ref().expect("a result");
        let n = wit::flatten(t).len();
        if n <= 1 || (task_return.is_some() && n <= MAX_FLAT_PARAMS) {
            wit::flat_load(g, s, t, ret, 0);
        } else {
            s.local_get(ret);
        }
        if let Some(f) = task_return {
            s.call(f);
        }
    };
    if !sig.throws {
        produce(cx, &mut g, &mut s);
        settle(&mut s);
        match sig.result {
            None => {}
            Some(AbiType::String) => {
                s.local_get(vstr).ref_cast_non_null(runtime::string());
                alloc(&mut s, 4, 8);
                store_string(&mut s, to_utf8.unwrap(), 0);
                deliver(&mut g, &mut s);
            }
            Some(AbiType::Compound(_)) => {
                let t = full.as_ref().unwrap();
                let (size, align) = wit::size_align(t);
                alloc(&mut s, align as i32, size.max(1) as i32);
                wit::lower_mem(cx, &mut g, &mut s, t, vstr, ret, 0);
                deliver(&mut g, &mut s);
            }
            Some(t) => {
                s.local_get(match flat(t)[0] {
                    ValType::I64 => vi64,
                    ValType::F32 => vf32,
                    ValType::F64 => vf64,
                    _ => vi32,
                });
                if let Some(f) = task_return {
                    s.call(f);
                }
            }
        }
        if sig.result.is_none()
            && let Some(f) = task_return
        {
            s.call(f);
        }
        s.end();
        return finish(g, body);
    }
    let to_utf8 = to_utf8.expect("errors carry strings");
    let value_to_string = value_to_string.expect("errors carry strings");
    // result<T, E>: payload offset, size, alignment.
    let (ok_t, err_t) = match full.as_ref() {
        Some(WitType::Result { ok, err }) => (ok.as_deref(), err.as_deref()),
        _ => unreachable!("a throwing export returns a `result`"),
    };
    let (payload, (size, align)) = wit::variant_layout(2, &[ok_t, err_t]);
    // Both cases fill the return area, then it's delivered.
    s.block(BlockType::Empty);
    s.block(BlockType::Result(runtime::jsval()));
    s.try_table(
        BlockType::Empty,
        [Catch::One {
            tag: runtime::TAG,
            label: 0,
        }],
    );
    // Inside the `try_table`, which takes no values from outside.
    produce(cx, &mut g, &mut s);
    settle(&mut s);
    alloc(&mut s, align as i32, size as i32);
    if let Some(t) = sig.result {
        match t {
            AbiType::String => {
                s.local_get(vstr).ref_cast_non_null(runtime::string());
                store_string(&mut s, to_utf8, payload);
            }
            AbiType::Compound(_) => {
                wit::lower_mem(cx, &mut g, &mut s, ok_t.unwrap(), vstr, ret, payload);
            }
            _ => {
                s.local_get(ret);
                match t {
                    AbiType::Bool | AbiType::S8 | AbiType::U8 => {
                        s.local_get(vi32).i32_store8(mem(payload, 0));
                    }
                    AbiType::S16 | AbiType::U16 => {
                        s.local_get(vi32).i32_store16(mem(payload, 1));
                    }
                    AbiType::S64 | AbiType::U64 => {
                        s.local_get(vi64).i64_store(mem(payload, 3));
                    }
                    AbiType::F32 => {
                        s.local_get(vf32).f32_store(mem(payload, 2));
                    }
                    AbiType::F64 => {
                        s.local_get(vf64).f64_store(mem(payload, 3));
                    }
                    _ => {
                        s.local_get(vi32).i32_store(mem(payload, 2));
                    }
                }
            }
        }
    }
    s.local_get(ret).i32_const(0).i32_store8(mem(0, 0));
    s.br(2);
    s.end();
    s.unreachable();
    s.end();
    // Caught: the err case.
    s.local_set(exc);
    alloc(&mut s, align as i32, size as i32);
    s.local_get(ret).i32_const(1).i32_store8(mem(0, 0));
    match sig.err {
        ErrType::JsError => {
            let describe = cx.helper(runtime::Helper::Describe);
            let get = cx.helper(runtime::Helper::Obj(object::Helper::Get));
            let empty = cx.str("");
            let (name, message) = (cx.str("name"), cx.str("message"));
            object::is_error(&mut s, exc);
            s.if_(BlockType::Result(runtime::string_ref()));
            s.local_get(exc)
                .global_get(name)
                .call(get)
                .call(value_to_string);
            s.else_();
            s.global_get(empty);
            s.end();
            store_string(&mut s, to_utf8, payload);
            object::is_error(&mut s, exc);
            s.if_(BlockType::Result(runtime::string_ref()));
            s.local_get(exc)
                .global_get(message)
                .call(get)
                .call(value_to_string);
            s.else_();
            s.local_get(exc).call(describe);
            s.end();
            store_string(&mut s, to_utf8, payload + 8);
            // stack: none
            s.local_get(ret)
                .i32_const(0)
                .i32_store8(mem(payload + 16, 0));
        }
        ErrType::Empty => {}
        ErrType::Of(_) => {
            // jco's getErrorPayload: an own `payload`, else the value (an Error rethrows).
            let get = cx.helper(runtime::Helper::Obj(object::Helper::Get));
            let has_own = cx.helper(runtime::Helper::Obj(object::Helper::HasOwn));
            let nullish = cx.helper(runtime::Helper::IsNullish);
            let key = cx.str("payload");
            s.local_get(exc).call(nullish).i32_eqz();
            s.if_(BlockType::Empty);
            s.local_get(exc).global_get(key).call(has_own);
            s.if_(BlockType::Empty);
            s.local_get(exc).global_get(key).call(get).local_set(exc);
            s.else_();
            object::is_error(&mut s, exc);
            s.if_(BlockType::Empty);
            s.local_get(exc).throw(runtime::TAG);
            s.end();
            s.end();
            s.end();
            wit::lower_mem(cx, &mut g, &mut s, err_t.unwrap(), exc, ret, payload);
        }
    }
    s.end();
    deliver(&mut g, &mut s);
    s.end();
    finish(g, body)
}

/// Core params of an async export's `task.return` (T20): its result flattened (a pointer to it
/// in memory past 16 values).
pub fn task_return_params(sig: &AbiSig) -> Vec<ValType> {
    match result_type(sig).map(|t| wit::flatten(&t)) {
        None => vec![],
        Some(f) if f.len() > MAX_FLAT_PARAMS => vec![ValType::I32],
        Some(f) => f,
    }
}

/// `[async-lift]<name>` (T20): start the call (`start`: the [`Mode::Start`] adapter), then
/// [`complete`] it.
pub fn async_lift(cx: &mut dyn Cx, start: u32, finish: u32, sig: &AbiSig) -> Function {
    let nflat = flat_params(sig).len() as u32;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    for i in 0..nflat {
        s.local_get(i);
    }
    s.call(start);
    complete(cx, &mut s, finish, nflat);
    s.end();
    f
}

/// `[callback][async-lift]<name>`: apply the event ([`Helper::Event`]), then [`complete`].
pub fn async_callback(cx: &mut dyn Cx, finish: u32) -> Function {
    let event = cx.helper(runtime::Helper::Abi(Helper::Event));
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(0).local_get(1).local_get(2).call(event);
    complete(cx, &mut s, finish, 3);
    s.end();
    f
}

/// [`Helper::Step`] the call; while it waits, return the callback code. When it's done, the
/// [`Mode::Finish`] adapter (`finish`) `task.return`s its result, the call [`Helper::End`]s,
/// and `EXIT`. Local `code`: an i32.
fn complete(cx: &mut dyn Cx, s: &mut InstructionSink, finish: u32, code: u32) {
    let step = cx.helper(runtime::Helper::Abi(Helper::Step));
    let end = cx.helper(runtime::Helper::Abi(Helper::End));
    s.call(step).local_tee(code);
    s.if_(BlockType::Empty);
    s.local_get(code).return_();
    s.end();
    s.call(finish);
    s.call(end);
    s.i32_const(CALLBACK_EXIT);
}

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
