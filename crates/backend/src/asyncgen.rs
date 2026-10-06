//! Async generators (T20, design D3). The frames come from `ir::suspend`.
//!
//! An async generator object is a `$generator` of class [`object::CLASS_ASYNC_GEN`] with the
//! prototype `%AsyncGeneratorPrototype%`; its frame has one slot more than the function has
//! locals: the queue of requests, a `$vals` of `[mode, value, promise]` (`mode` as
//! `Intrinsic::ResumeMode`, an `i31`), the head being the one running.
//!
//! - `next`/`throw`/`return` ([`Helper::Method`]) make the promise, queue the request and, if the
//!   generator is suspended (at its start or at a `yield`), [`Helper::Step`] it. On a completed
//!   generator `next` fulfills `{ value: undefined, done: true }`, `throw` rejects and `return`
//!   awaits its value ([`Helper::AwaitReturn`]).
//! - [`Helper::Step`] runs the resume function. It returned: fulfill the head `{ value, done:
//!   true }` and drain the queue ([`Helper::Drain`]); it threw: reject the head and drain; an
//!   `await` (`yielded` 3): the frame waits on the promise like an async function's (the promise
//!   job calls `asyncfn`'s step, which comes here); a `yield` (the lowering awaited the value):
//!   fulfill the head `{ value, done: false }` and go on with the next request, if any.
//! - The state is the generator's `status`: [`SUSPENDED`], [`EXECUTING`] (also while awaiting a
//!   `return` value: new requests just queue), [`COMPLETED`].

use js2wasi3_ir::{Builtin, ErrorKind, RESUME_NEXT, RESUME_RETURN, RESUME_THROW};
use wasm_encoder::{BlockType, Catch, Function, HeapType, InstructionSink, ValType};

use crate::generator::{
    self, COMPLETED, EXECUTING, F_FRAME, F_RESUME, F_STATUS, F_YIELDED, SUSPENDED,
};
use crate::runtime::{
    self, Cx, G_UNDEFINED, T_BYTES, T_GENERATOR, T_I32S, T_KEYS, T_OBJECT, T_PROMISE, T_RESUME,
    T_VALS, TAG, jsval, rt,
};
use crate::{func, object, promise};

/// `yielded` of an `await` in an async generator (`Intrinsic::GenSuspend { awaiting }`).
pub const YIELDED_AWAIT: i32 = 3;

/// Settle kinds of [`Helper::Settle`].
const YIELD: i32 = 0;
const DONE: i32 = 1;
const REJECT: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(resume: (ref $resume), slots: i32) -> jsval`: a new suspended async generator.
    New,
    /// `(gen: jsval, mode: i32, value: jsval)`: resume it (see the module docs).
    Step,
    /// The code of `%AsyncGeneratorPrototype%.next` / `throw` / `return`.
    Method(i32),
    /// `(gen)`: AsyncGeneratorDrainQueue.
    Drain,
    /// `(gen, value)`: AsyncGeneratorAwaitReturn.
    AwaitReturn,
    /// The code of its promise's handlers (`env`: `[gen]`; `true`: rejected).
    ReturnHandler(bool),
    /// `(gen, value, kind: i32)`: remove the head request and settle its promise: `{ value, done:
    /// false }` (0), `{ value, done: true }` (1) or rejected with `value` (2).
    Settle,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_async_generator",
            Helper::Step => "async_generator_step",
            Helper::Method(RESUME_NEXT) => "%AsyncGeneratorPrototype%.next",
            Helper::Method(RESUME_THROW) => "%AsyncGeneratorPrototype%.throw",
            Helper::Method(_) => "%AsyncGeneratorPrototype%.return",
            Helper::Drain => "async_generator_drain_queue",
            Helper::AwaitReturn => "async_generator_await_return",
            Helper::ReturnHandler(_) => "async_generator_return_handler",
            Helper::Settle => "async_generator_settle",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let i = ValType::I32;
        match self {
            Helper::New => (vec![rt(false, T_RESUME), i], vec![v]),
            Helper::Step => (vec![v, i, v], vec![]),
            Helper::Method(_) | Helper::ReturnHandler(_) => (vec![v, v, v], vec![v]),
            Helper::Drain => (vec![v], vec![]),
            Helper::AwaitReturn => (vec![v, v], vec![]),
            Helper::Settle => (vec![v, v, i], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::Step => step(cx),
            Helper::Method(mode) => method(cx, mode),
            Helper::Drain => drain(cx),
            Helper::AwaitReturn => await_return(cx),
            Helper::ReturnHandler(rejected) => {
                let settle = ah(cx, Helper::Settle);
                let drain = ah(cx, Helper::Drain);
                let arg = cx.helper(runtime::Helper::Func(func::Helper::ArgGet));
                const SELF: u32 = 0;
                const ARGS: u32 = 2;
                const G: u32 = 3;
                let mut f = Function::new([(1, jsval())]);
                let mut s = f.instructions();
                s.local_get(SELF)
                    .ref_cast_non_null(func::function())
                    .struct_get(runtime::T_FUNCTION, func::F_ENV)
                    .ref_as_non_null()
                    .i32_const(0)
                    .array_get(T_VALS)
                    .local_set(G);
                status(&mut s, G, COMPLETED);
                s.local_get(G);
                s.local_get(ARGS).i32_const(0).call(arg);
                s.i32_const(if rejected { REJECT } else { DONE });
                s.call(settle);
                s.local_get(G).call(drain);
                s.global_get(G_UNDEFINED);
                s.end();
                f
            }
            Helper::Settle => settle(cx),
        }
    }
}

fn ah(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::AsyncGen(h))
}

fn ph(cx: &mut dyn Cx, h: promise::Helper) -> u32 {
    cx.helper(runtime::Helper::Prom(h))
}

fn gen_(s: &mut InstructionSink, local: u32) {
    s.local_get(local).ref_cast_non_null(generator::generator());
}

fn status(s: &mut InstructionSink, g: u32, st: i32) {
    gen_(s, g);
    s.i32_const(st).struct_set(T_GENERATOR, F_STATUS);
}

/// Push the queue of the generator in local `g` (a `(ref $vals)`).
fn queue(s: &mut InstructionSink, g: u32) {
    gen_(s, g);
    s.struct_get(T_GENERATOR, F_FRAME);
    gen_(s, g);
    s.struct_get(T_GENERATOR, F_FRAME)
        .array_len()
        .i32_const(1)
        .i32_sub();
    s.array_get(T_VALS)
        .ref_cast_non_null(HeapType::Concrete(T_VALS));
}

/// Set the queue of the generator in local `g` to the `(ref $vals)` in local `q`.
fn set_queue(s: &mut InstructionSink, g: u32, q: u32) {
    gen_(s, g);
    s.struct_get(T_GENERATOR, F_FRAME);
    gen_(s, g);
    s.struct_get(T_GENERATOR, F_FRAME)
        .array_len()
        .i32_const(1)
        .i32_sub();
    s.local_get(q);
    s.array_set(T_VALS);
}

/// Is local `v` an async generator object? (An `i32`.)
pub fn is_async_gen(s: &mut InstructionSink, v: u32) {
    s.local_get(v).ref_test_non_null(generator::generator());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(v)
        .ref_cast_non_null(runtime::object())
        .struct_get(T_OBJECT, object::F_CLASS)
        .i32_const(object::CLASS_MASK)
        .i32_and()
        .i32_const(object::CLASS_ASYNC_GEN)
        .i32_eq();
    s.else_();
    s.i32_const(0);
    s.end();
}

fn new(cx: &mut dyn Cx) -> Function {
    let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
        Builtin::AsyncGenerator,
    )));
    const RESUME: u32 = 0;
    const SLOTS: u32 = 1;
    const FRAME: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    s.local_get(SLOTS)
        .i32_const(1)
        .i32_add()
        .array_new_default(T_VALS)
        .local_tee(FRAME);
    s.ref_as_non_null().local_get(SLOTS);
    s.i32_const(0).array_new_default(T_VALS);
    s.array_set(T_VALS);
    s.i32_const(object::CLASS_ASYNC_GEN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(RESUME);
    s.local_get(FRAME).ref_as_non_null();
    s.i32_const(0).i32_const(SUSPENDED).i32_const(0);
    s.struct_new(T_GENERATOR);
    s.end();
    f
}

fn settle(cx: &mut dyn Cx) -> Function {
    let resolve = ph(cx, promise::Helper::Resolve);
    let reject = ph(cx, promise::Helper::Settle { rejected: true });
    let result = cx.helper(runtime::Helper::Gen(generator::Helper::IterResult));
    const G: u32 = 0;
    const V: u32 = 1;
    const K: u32 = 2;
    const Q: u32 = 3;
    const REST: u32 = 4;
    const P: u32 = 5;
    let mut f = Function::new([(2, rt(true, T_VALS)), (1, rt(true, T_PROMISE))]);
    let mut s = f.instructions();
    queue(&mut s, G);
    s.local_tee(Q);
    s.ref_as_non_null().i32_const(0).array_get(T_VALS);
    s.ref_cast_non_null(HeapType::Concrete(T_VALS));
    s.i32_const(2).array_get(T_VALS);
    s.ref_cast_non_null(HeapType::Concrete(T_PROMISE))
        .local_set(P);
    // Remove the head.
    s.local_get(Q)
        .ref_as_non_null()
        .array_len()
        .i32_const(1)
        .i32_sub();
    s.array_new_default(T_VALS).local_set(REST);
    s.local_get(REST).ref_as_non_null().i32_const(0);
    s.local_get(Q).ref_as_non_null().i32_const(1);
    s.local_get(REST).ref_as_non_null().array_len();
    s.array_copy(T_VALS, T_VALS);
    set_queue(&mut s, G, REST);
    s.local_get(K).i32_const(REJECT).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(P).ref_as_non_null().local_get(V).call(reject);
    s.else_();
    s.local_get(P).ref_as_non_null();
    s.local_get(V).local_get(K).call(result);
    s.call(resolve);
    s.end();
    s.end();
    f
}

fn step(cx: &mut dyn Cx) -> Function {
    let settle = ah(cx, Helper::Settle);
    let drain = ah(cx, Helper::Drain);
    let promise_resolve = ph(cx, promise::Helper::PromiseResolve);
    let await_then = ph(cx, promise::Helper::AwaitThen);
    const G: u32 = 0;
    const MODE: u32 = 1;
    const V: u32 = 2;
    const R: u32 = 3;
    const Y: u32 = 4;
    const REQ: u32 = 5;
    let mut f = Function::new([(1, jsval()), (1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    let catch = [Catch::One { tag: TAG, label: 0 }];
    s.loop_(BlockType::Empty);
    {
        status(&mut s, G, EXECUTING);
        gen_(&mut s, G);
        s.i32_const(0).struct_set(T_GENERATOR, F_YIELDED);
        s.block(BlockType::Empty);
        s.block(BlockType::Result(jsval()));
        s.try_table(BlockType::Empty, catch.clone());
        s.local_get(G).local_get(MODE).local_get(V);
        gen_(&mut s, G);
        s.struct_get(T_GENERATOR, F_RESUME);
        s.call_ref(T_RESUME).local_set(R);
        s.end();
        s.br(1);
        s.end();
        // The body threw: reject the head, drain.
        s.local_set(R);
        status(&mut s, G, COMPLETED);
        s.local_get(G).local_get(R).i32_const(REJECT).call(settle);
        s.local_get(G).call(drain);
        s.return_();
        s.end();
        gen_(&mut s, G);
        s.struct_get(T_GENERATOR, F_YIELDED).local_tee(Y);
        s.i32_eqz();
        s.if_(BlockType::Empty);
        {
            // It returned.
            status(&mut s, G, COMPLETED);
            s.local_get(G).local_get(R).i32_const(DONE).call(settle);
            s.local_get(G).call(drain);
            s.return_();
        }
        s.end();
        s.local_get(Y).i32_const(YIELDED_AWAIT).i32_eq();
        s.if_(BlockType::Empty);
        {
            // `await R` (as in an async function).
            s.block(BlockType::Result(jsval()));
            s.try_table(BlockType::Empty, catch);
            s.local_get(R).call(promise_resolve);
            s.ref_cast_non_null(HeapType::Concrete(T_PROMISE));
            s.local_get(G).local_get(G);
            s.ref_null(HeapType::Concrete(T_PROMISE));
            s.call(await_then);
            s.return_();
            s.end();
            s.unreachable();
            s.end();
            // PromiseResolve threw: throw it at the `await`.
            s.local_set(V);
            s.i32_const(RESUME_THROW).local_set(MODE);
            s.br(1);
        }
        s.end();
        // A `yield`: fulfill the head, go on with the next request.
        status(&mut s, G, SUSPENDED);
        s.local_get(G).local_get(R).i32_const(YIELD).call(settle);
        queue(&mut s, G);
        s.array_len().i32_eqz();
        s.if_(BlockType::Empty);
        s.return_();
        s.end();
        queue(&mut s, G);
        s.i32_const(0).array_get(T_VALS);
        s.ref_cast_non_null(HeapType::Concrete(T_VALS))
            .local_tee(REQ);
        s.i32_const(0).array_get(T_VALS);
        s.ref_cast_non_null(HeapType::I31)
            .i31_get_s()
            .local_set(MODE);
        s.local_get(REQ)
            .ref_as_non_null()
            .i32_const(1)
            .array_get(T_VALS);
        s.local_set(V);
        s.br(0);
    }
    s.end();
    s.end();
    f
}

fn method(cx: &mut dyn Cx, mode: i32) -> Function {
    let new = ph(cx, promise::Helper::New);
    let resolve = ph(cx, promise::Helper::Resolve);
    let reject = ph(cx, promise::Helper::Settle { rejected: true });
    let new_error = cx.helper(runtime::Helper::Obj(object::Helper::NewError(
        ErrorKind::TypeError,
    )));
    let describe = ph(cx, promise::Helper::Describe);
    let concat = cx.helper(runtime::Helper::Concat);
    let result = cx.helper(runtime::Helper::Gen(generator::Helper::IterResult));
    let arg = cx.helper(runtime::Helper::Func(func::Helper::ArgGet));
    let step = ah(cx, Helper::Step);
    let await_return = ah(cx, Helper::AwaitReturn);
    let name = match mode {
        RESUME_NEXT => "next",
        RESUME_THROW => "throw",
        _ => "return",
    };
    let incompatible = cx.str(&format!("{name} method called on incompatible receiver "));
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const P: u32 = 3;
    const V: u32 = 4;
    const ST: u32 = 5;
    const Q: u32 = 6;
    const NEWQ: u32 = 7;
    let mut f = Function::new([
        (1, rt(true, T_PROMISE)),
        (1, jsval()),
        (1, ValType::I32),
        (2, rt(true, T_VALS)),
    ]);
    let mut s = f.instructions();
    s.call(new).local_set(P);
    s.local_get(ARGS).i32_const(0).call(arg).local_set(V);
    is_async_gen(&mut s, THIS);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    {
        s.local_get(P).ref_as_non_null();
        s.global_get(incompatible);
        s.local_get(THIS).call(describe).call(concat);
        s.global_get(G_UNDEFINED).call(new_error);
        s.call(reject);
        s.local_get(P).return_();
    }
    s.end();
    gen_(&mut s, THIS);
    s.struct_get(T_GENERATOR, F_STATUS).local_tee(ST);
    s.i32_const(COMPLETED).i32_eq();
    if mode != RESUME_RETURN {
        s.if_(BlockType::Empty);
        s.local_get(P).ref_as_non_null();
        if mode == RESUME_NEXT {
            s.global_get(G_UNDEFINED).i32_const(1).call(result);
            s.call(resolve);
        } else {
            s.local_get(V).call(reject);
        }
        s.local_get(P).return_();
        s.end();
    } else {
        s.drop();
    }
    // Queue the request.
    queue(&mut s, THIS);
    s.local_tee(Q)
        .ref_as_non_null()
        .array_len()
        .i32_const(1)
        .i32_add();
    s.array_new_default(T_VALS).local_set(NEWQ);
    s.local_get(NEWQ).ref_as_non_null().i32_const(0);
    s.local_get(Q).ref_as_non_null().i32_const(0);
    s.local_get(Q).ref_as_non_null().array_len();
    s.array_copy(T_VALS, T_VALS);
    s.local_get(NEWQ).ref_as_non_null();
    s.local_get(Q).ref_as_non_null().array_len();
    s.i32_const(mode).ref_i31();
    s.local_get(V);
    s.local_get(P);
    s.array_new_fixed(T_VALS, 3);
    s.array_set(T_VALS);
    set_queue(&mut s, THIS, NEWQ);
    if mode == RESUME_RETURN {
        // A completed generator awaits the value.
        s.local_get(ST).i32_const(COMPLETED).i32_eq();
        s.if_(BlockType::Empty);
        status(&mut s, THIS, EXECUTING);
        s.local_get(THIS).local_get(V).call(await_return);
        s.local_get(P).return_();
        s.end();
    }
    s.local_get(ST).i32_const(SUSPENDED).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(THIS).i32_const(mode).local_get(V).call(step);
    s.end();
    s.local_get(P);
    s.end();
    f
}

fn drain(cx: &mut dyn Cx) -> Function {
    let settle = ah(cx, Helper::Settle);
    let await_return = ah(cx, Helper::AwaitReturn);
    const G: u32 = 0;
    const REQ: u32 = 1;
    const MODE: u32 = 2;
    let mut f = Function::new([(1, rt(true, T_VALS)), (1, ValType::I32)]);
    let mut s = f.instructions();
    status(&mut s, G, COMPLETED);
    s.loop_(BlockType::Empty);
    {
        queue(&mut s, G);
        s.array_len().i32_eqz();
        s.if_(BlockType::Empty);
        s.return_();
        s.end();
        queue(&mut s, G);
        s.i32_const(0).array_get(T_VALS);
        s.ref_cast_non_null(HeapType::Concrete(T_VALS))
            .local_tee(REQ);
        s.i32_const(0).array_get(T_VALS);
        s.ref_cast_non_null(HeapType::I31)
            .i31_get_s()
            .local_tee(MODE);
        s.i32_const(RESUME_RETURN).i32_eq();
        s.if_(BlockType::Empty);
        status(&mut s, G, EXECUTING);
        s.local_get(G);
        s.local_get(REQ)
            .ref_as_non_null()
            .i32_const(1)
            .array_get(T_VALS);
        s.call(await_return);
        s.return_();
        s.end();
        s.local_get(G);
        s.local_get(MODE).i32_const(RESUME_THROW).i32_eq();
        s.if_(BlockType::Result(jsval()));
        s.local_get(REQ)
            .ref_as_non_null()
            .i32_const(1)
            .array_get(T_VALS);
        s.else_();
        s.global_get(G_UNDEFINED);
        s.end();
        s.i32_const(REJECT).i32_const(DONE);
        s.local_get(MODE).i32_const(RESUME_THROW).i32_eq();
        s.select();
        s.call(settle);
        s.br(0);
    }
    s.end();
    s.end();
    f
}

fn await_return(cx: &mut dyn Cx) -> Function {
    let promise_resolve = ph(cx, promise::Helper::PromiseResolve);
    let perform_then = ph(cx, promise::Helper::PerformThen);
    let settle = ah(cx, Helper::Settle);
    let drain = ah(cx, Helper::Drain);
    let new_function = cx.helper(runtime::Helper::Func(func::Helper::New));
    let handlers = [false, true].map(|r| {
        let code = ah(cx, Helper::ReturnHandler(r));
        cx.declare_func(code);
        code
    });
    let empty = cx.str("");
    const G: u32 = 0;
    const V: u32 = 1;
    const P: u32 = 2;
    const ENV: u32 = 3;
    let mut f = Function::new([(1, jsval()), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    s.block(BlockType::Empty);
    s.block(BlockType::Result(jsval()));
    s.try_table(BlockType::Empty, [Catch::One { tag: TAG, label: 0 }]);
    s.local_get(V).call(promise_resolve).local_set(P);
    s.end();
    s.br(1);
    s.end();
    // PromiseResolve threw: reject the head.
    s.local_set(V);
    status(&mut s, G, COMPLETED);
    s.local_get(G).local_get(V).i32_const(REJECT).call(settle);
    s.local_get(G).call(drain);
    s.return_();
    s.end();
    s.local_get(G).array_new_fixed(T_VALS, 1).local_set(ENV);
    s.local_get(P)
        .ref_cast_non_null(HeapType::Concrete(T_PROMISE));
    for code in handlers {
        promise::native(cx, &mut s, new_function, code, Some(ENV), empty, 1);
    }
    s.ref_null(HeapType::Concrete(T_PROMISE));
    s.call(perform_then);
    s.end();
    f
}
