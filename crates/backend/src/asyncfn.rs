//! Async functions (T19, design D3). The frames come from `ir::suspend`.
//!
//! A call of an `async function` makes a frame: a `$generator` (see [`crate::generator`]) that
//! never reaches JS, whose `$vals` frame has one slot more than the function has locals, holding
//! the promise the call returns. [`Helper::Step`] runs the resume function once:
//! - it returned: resolve the promise with the value (a thenable adopts, as in the spec);
//! - it threw: reject the promise;
//! - it suspended at an `await v` (`yielded` set, `v` returned): `PromiseResolve(v)`, then
//!   PerformPromiseThen on it with the frame itself as both handlers (no closures). When that
//!   promise settles, `promise::reaction_job` sees a frame as the handler and calls
//!   [`Helper::Step`] with mode next (fulfilled) or throw (rejected), which is Await's resumption.
//!   If `PromiseResolve` throws (a throwing `constructor` getter), the exception is thrown at the
//!   `await` instead.

use js2wasi3_ir::{RESUME_NEXT, RESUME_THROW};
use wasm_encoder::{BlockType, Catch, Function, HeapType, InstructionSink, ValType};

use crate::generator::{
    self, COMPLETED, EXECUTING, F_FRAME, F_RESUME, F_STATUS, F_YIELDED, SUSPENDED,
};
use crate::promise::{self, JOB_FULFILL, JOB_REJECT};
use crate::runtime::{
    self, Cx, G_UNDEFINED, T_GENERATOR, T_PROMISE, T_RESUME, T_VALS, TAG, jsval, rt,
};

// A settled promise's job kind is the resume mode of the frame waiting on it.
const _: () = assert!(JOB_FULFILL == RESUME_NEXT && JOB_REJECT == RESUME_THROW);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(resume: (ref $resume), slots: i32) -> jsval`: a new frame (`slots` locals) and its
    /// pending promise.
    New,
    /// `(frame: jsval) -> jsval`: run the body up to its first `await`; the promise.
    Start,
    /// `(frame: jsval, mode: i32, value: jsval)`: resume the body (see the module docs).
    Step,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_async_frame",
            Helper::Start => "async_start",
            Helper::Step => "async_step",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        match self {
            Helper::New => (vec![rt(false, T_RESUME), ValType::I32], vec![v]),
            Helper::Start => (vec![v], vec![v]),
            Helper::Step => (vec![v, ValType::I32, v], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => {
                let new_gen = cx.helper(runtime::Helper::Gen(generator::Helper::New));
                let new_promise = ph(cx, promise::Helper::New);
                const RESUME: u32 = 0;
                const SLOTS: u32 = 1;
                const G: u32 = 2;
                let mut f = Function::new([(1, jsval())]);
                let mut s = f.instructions();
                s.local_get(RESUME)
                    .local_get(SLOTS)
                    .i32_const(1)
                    .i32_add()
                    .call(new_gen)
                    .local_tee(G);
                s.ref_cast_non_null(generator::generator())
                    .struct_get(T_GENERATOR, F_FRAME);
                s.local_get(SLOTS).call(new_promise).array_set(T_VALS);
                s.local_get(G);
                s.end();
                f
            }
            Helper::Start => {
                let step = cx.helper(runtime::Helper::Async(Helper::Step));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0)
                    .i32_const(RESUME_NEXT)
                    .global_get(G_UNDEFINED);
                s.call(step);
                promise_of(&mut s, 0);
                s.end();
                f
            }
            Helper::Step => step(cx),
        }
    }
}

fn ph(cx: &mut dyn Cx, h: promise::Helper) -> u32 {
    cx.helper(runtime::Helper::Prom(h))
}

fn frame(s: &mut InstructionSink, local: u32) {
    s.local_get(local).ref_cast_non_null(generator::generator());
}

/// The frame's promise (its last slot), a `(ref $promise)`.
fn promise_of(s: &mut InstructionSink, local: u32) {
    frame(s, local);
    s.struct_get(T_GENERATOR, F_FRAME);
    frame(s, local);
    s.struct_get(T_GENERATOR, F_FRAME)
        .array_len()
        .i32_const(1)
        .i32_sub();
    s.array_get(T_VALS)
        .ref_cast_non_null(HeapType::Concrete(T_PROMISE));
}

fn step(cx: &mut dyn Cx) -> Function {
    let resolve = ph(cx, promise::Helper::Resolve);
    let reject = ph(cx, promise::Helper::Settle { rejected: true });
    let promise_resolve = ph(cx, promise::Helper::PromiseResolve);
    let await_then = ph(cx, promise::Helper::AwaitThen);
    const G: u32 = 0;
    const MODE: u32 = 1;
    const V: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    let status = |s: &mut InstructionSink, st: i32| {
        frame(s, G);
        s.i32_const(st).struct_set(T_GENERATOR, F_STATUS);
    };
    let catch = [Catch::One { tag: TAG, label: 0 }];
    s.loop_(BlockType::Empty);
    {
        status(&mut s, EXECUTING);
        frame(&mut s, G);
        s.i32_const(0).struct_set(T_GENERATOR, F_YIELDED);
        s.block(BlockType::Empty);
        s.block(BlockType::Result(jsval()));
        s.try_table(BlockType::Empty, catch.clone());
        s.local_get(G).local_get(MODE).local_get(V);
        frame(&mut s, G);
        s.struct_get(T_GENERATOR, F_RESUME);
        s.call_ref(T_RESUME).local_set(R);
        s.end();
        s.br(1);
        s.end();
        // The body threw: reject.
        s.local_set(R);
        status(&mut s, COMPLETED);
        promise_of(&mut s, G);
        s.local_get(R).call(reject);
        s.return_();
        s.end();
        frame(&mut s, G);
        s.struct_get(T_GENERATOR, F_YIELDED).i32_eqz();
        s.if_(BlockType::Empty);
        {
            // It returned: resolve.
            status(&mut s, COMPLETED);
            promise_of(&mut s, G);
            s.local_get(R).call(resolve);
            s.return_();
        }
        s.end();
        // `await R`.
        status(&mut s, SUSPENDED);
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
        s.br(0);
    }
    s.end();
    s.unreachable();
    s.end();
    f
}
