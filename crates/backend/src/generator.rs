//! Generator objects (T18, design D3). The frames come from `ir::suspend`.
//!
//! # Layout
//! `$generator` is a final subtype of `$object`: the seven object fields, then `resume: (ref
//! $resume)` (the resume function `(gen, mode: i32, value) -> jsval` the transform made),
//! `frame: (ref $vals)` (the saved locals, boxed), `point: mut i32` (where it's suspended, 0: the
//! start), `status: mut i32` ([`SUSPENDED`], [`EXECUTING`], [`COMPLETED`]) and `yielded: mut i32`
//! (set by [`Helper::Suspend`] during a resume: 0 returned, 1 yielded a value, 2 yielded an
//! iterator result as is (`yield*`)).
//!
//! # %GeneratorPrototype%
//! `next`, `return`, `throw` ([`Helper::Resume`] with the mode) and `[Symbol.iterator]` (`this`).
//! A generator's own prototype is `%GeneratorPrototype%` directly: generator functions have no
//! `prototype` property yet.
//!
//! # `yield*`
//! [`Helper::DelegateOpen`] starts iterating like `for-of` (arrays and strings by index, without
//! an iterator object); [`Helper::DelegateStep`] does one `next`/`throw`/`return` on it (see
//! `Intrinsic::DelegateStep`).

use js2wasi3_ir::{Builtin, ErrorKind, RESUME_NEXT, RESUME_RETURN, RESUME_THROW};
use wasm_encoder::{BlockType, Catch, Function, HeapType, InstructionSink, ValType};

use crate::runtime::{
    self, Cx, G_FALSE, G_TRUE, G_UNDEFINED, T_BYTES, T_GENERATOR, T_I32S, T_ITER, T_KEYS, T_RESUME,
    T_VALS, TAG, jsval, rt,
};
use crate::{func, iter, object, promise};

/// `$generator` field indices after the object's.
pub const F_RESUME: u32 = 7;
pub const F_FRAME: u32 = 8;
pub const F_POINT: u32 = 9;
pub const F_STATUS: u32 = 10;
pub const F_YIELDED: u32 = 11;

pub const SUSPENDED: i32 = 0;
pub const EXECUTING: i32 = 1;
pub const COMPLETED: i32 = 2;

pub fn generator() -> HeapType {
    HeapType::Concrete(T_GENERATOR)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(resume: (ref $resume), slots: i32) -> jsval`: a new suspended generator.
    New,
    /// `(g: jsval, point: i32, yielded: i32)`: the resume function suspends at `point`.
    Suspend,
    /// `(this, mode: i32, value) -> jsval`: `next` / `throw` / `return` (`mode` as
    /// `Intrinsic::ResumeMode`): the iterator result.
    Resume,
    /// The code of `%GeneratorPrototype%.next` / `throw` / `return`.
    Method(i32),
    /// `(value, done: i32) -> jsval`: `{ value, done }`.
    IterResult,
    /// `(v) -> jsval`: `yield* v`'s iteration state.
    DelegateOpen,
    /// `(it, mode: i32, received) -> jsval`: one step of `yield*`.
    DelegateStep,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_generator",
            Helper::Suspend => "generator_suspend",
            Helper::Resume => "generator_resume",
            Helper::Method(RESUME_NEXT) => "%GeneratorPrototype%.next",
            Helper::Method(RESUME_THROW) => "%GeneratorPrototype%.throw",
            Helper::Method(_) => "%GeneratorPrototype%.return",
            Helper::IterResult => "create_iter_result_object",
            Helper::DelegateOpen => "delegate_open",
            Helper::DelegateStep => "delegate_step",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let i = ValType::I32;
        match self {
            Helper::New => (vec![rt(false, T_RESUME), i], vec![v]),
            Helper::Suspend => (vec![v, i, i], vec![]),
            Helper::Resume | Helper::DelegateStep => (vec![v, i, v], vec![v]),
            Helper::Method(_) => (vec![v, v, v], vec![v]),
            Helper::IterResult => (vec![v, i], vec![v]),
            Helper::DelegateOpen => (vec![v], vec![v]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::Suspend => {
                const G: u32 = 0;
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(G).ref_cast_non_null(generator());
                s.local_get(1).struct_set(T_GENERATOR, F_POINT);
                s.local_get(G).ref_cast_non_null(generator());
                s.local_get(2).struct_set(T_GENERATOR, F_YIELDED);
                s.end();
                f
            }
            Helper::Resume => resume(cx),
            Helper::Method(mode) => {
                let resume = gh(cx, Helper::Resume);
                let arg = cx.helper(runtime::Helper::Func(func::Helper::ArgGet));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(1).i32_const(mode);
                s.local_get(2).i32_const(0).call(arg);
                s.call(resume);
                s.end();
                f
            }
            Helper::IterResult => {
                let new_object = oh(cx, object::Helper::NewObject);
                let define = oh(cx, object::Helper::DefineProp);
                let (value, done) = (cx.str("value"), cx.str("done"));
                const V: u32 = 0;
                const DONE: u32 = 1;
                const R: u32 = 2;
                let mut f = Function::new([(1, jsval())]);
                let mut s = f.instructions();
                s.call(new_object).local_set(R);
                s.local_get(R).global_get(value).local_get(V).call(define);
                s.local_get(R).global_get(done);
                s.global_get(G_TRUE).global_get(G_FALSE).local_get(DONE);
                s.typed_select(jsval());
                s.call(define);
                s.local_get(R);
                s.end();
                f
            }
            Helper::DelegateOpen => {
                let msg = cx.helper(runtime::Helper::Prom(promise::Helper::NotIterable));
                let open = cx.helper(runtime::Helper::Iter(iter::Helper::Open));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0).local_get(0).call(msg).call(open);
                s.end();
                f
            }
            Helper::DelegateStep => delegate_step(cx),
        }
    }
}

fn gh(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Gen(h))
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn gen_(s: &mut InstructionSink, local: u32) {
    s.local_get(local).ref_cast_non_null(generator());
}

fn new(cx: &mut dyn Cx) -> Function {
    let proto = oh(cx, object::Helper::Proto(Builtin::Generator));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(0);
    s.local_get(1).array_new_default(T_VALS);
    s.i32_const(0).i32_const(SUSPENDED).i32_const(0);
    s.struct_new(T_GENERATOR);
    s.end();
    f
}

/// GeneratorResume / GeneratorResumeAbrupt.
fn resume(cx: &mut dyn Cx) -> Function {
    let result = gh(cx, Helper::IterResult);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let describe = cx.helper(runtime::Helper::Prom(promise::Helper::Describe));
    let concat = cx.helper(runtime::Helper::Concat);
    let running = cx.str("Generator is already running");
    let incompatible = [RESUME_NEXT, RESUME_THROW, RESUME_RETURN].map(|m| {
        let name = match m {
            RESUME_NEXT => "next",
            RESUME_THROW => "throw",
            _ => "return",
        };
        cx.str(&format!(
            "Method [Generator].prototype.{name} called on incompatible receiver "
        ))
    });
    const THIS: u32 = 0;
    const MODE: u32 = 1;
    const V: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(generator()).i32_eqz();
    s.if_(BlockType::Empty);
    {
        s.global_get(incompatible[0]);
        s.global_get(incompatible[1]);
        s.local_get(MODE).i32_const(RESUME_NEXT).i32_eq();
        s.typed_select(runtime::string_ref());
        s.global_get(incompatible[2]);
        s.local_get(MODE).i32_const(RESUME_RETURN).i32_ne();
        s.typed_select(runtime::string_ref());
        s.local_get(THIS).call(describe).call(concat);
        s.call(throw);
    }
    s.end();
    gen_(&mut s, THIS);
    s.struct_get(T_GENERATOR, F_STATUS)
        .i32_const(EXECUTING)
        .i32_eq();
    s.if_(BlockType::Empty);
    s.global_get(running).call(throw);
    s.end();
    gen_(&mut s, THIS);
    s.struct_get(T_GENERATOR, F_STATUS)
        .i32_const(COMPLETED)
        .i32_eq();
    s.if_(BlockType::Empty);
    {
        s.local_get(MODE).i32_const(RESUME_THROW).i32_eq();
        s.if_(BlockType::Empty);
        s.local_get(V).throw(TAG);
        s.end();
        s.local_get(V).global_get(G_UNDEFINED);
        s.local_get(MODE).i32_const(RESUME_RETURN).i32_eq();
        s.typed_select(jsval());
        s.i32_const(1).call(result).return_();
    }
    s.end();
    gen_(&mut s, THIS);
    s.i32_const(EXECUTING).struct_set(T_GENERATOR, F_STATUS);
    gen_(&mut s, THIS);
    s.i32_const(0).struct_set(T_GENERATOR, F_YIELDED);
    // An exception completes the generator.
    s.block(BlockType::Empty);
    s.block(BlockType::Result(jsval()));
    s.try_table(BlockType::Empty, [Catch::One { tag: TAG, label: 0 }]);
    s.local_get(THIS).local_get(MODE).local_get(V);
    gen_(&mut s, THIS);
    s.struct_get(T_GENERATOR, F_RESUME);
    s.call_ref(T_RESUME).local_set(R);
    s.end();
    s.br(1);
    s.end();
    s.local_set(R);
    gen_(&mut s, THIS);
    s.i32_const(COMPLETED).struct_set(T_GENERATOR, F_STATUS);
    s.local_get(R).throw(TAG);
    s.end();
    gen_(&mut s, THIS);
    s.struct_get(T_GENERATOR, F_YIELDED).i32_eqz();
    s.if_(BlockType::Empty);
    {
        gen_(&mut s, THIS);
        s.i32_const(COMPLETED).struct_set(T_GENERATOR, F_STATUS);
        s.local_get(R).i32_const(1).call(result).return_();
    }
    s.end();
    gen_(&mut s, THIS);
    s.i32_const(SUSPENDED).struct_set(T_GENERATOR, F_STATUS);
    gen_(&mut s, THIS);
    s.struct_get(T_GENERATOR, F_YIELDED).i32_const(2).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(R).return_();
    s.end();
    s.local_get(R).i32_const(0).call(result);
    s.end();
    f
}

fn delegate_step(cx: &mut dyn Cx) -> Function {
    let step = cx.helper(runtime::Helper::Iter(iter::Helper::Step));
    let close = cx.helper(runtime::Helper::Iter(iter::Helper::Close));
    let result = gh(cx, Helper::IterResult);
    let get = oh(cx, object::Helper::Get);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let inspect = cx.helper(runtime::Helper::Inspect);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let no_throw = cx.str("The iterator does not provide a 'throw' method.");
    let (result_s, not_obj) = (cx.str("Iterator result "), cx.str(" is not an object"));
    let names = [cx.str("next"), cx.str("throw"), cx.str("return")];
    const IT: u32 = 0;
    const MODE: u32 = 1;
    const V: u32 = 2;
    const M: u32 = 3;
    const R: u32 = 4;
    let mut f = Function::new([(2, jsval())]);
    let mut s = f.instructions();
    let it = |s: &mut InstructionSink| {
        s.local_get(IT).ref_cast_non_null(iter::iter());
    };
    let hole = |s: &mut InstructionSink| {
        s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
    };
    // Arrays and strings (no iterator object): `next` only.
    it(&mut s);
    s.struct_get(T_ITER, iter::F_KIND)
        .i32_const(iter::PROTOCOL)
        .i32_ne();
    s.if_(BlockType::Empty);
    {
        s.local_get(MODE).i32_const(RESUME_RETURN).i32_eq();
        s.if_(BlockType::Empty);
        hole(&mut s);
        s.return_();
        s.end();
        s.local_get(MODE).i32_const(RESUME_THROW).i32_eq();
        s.if_(BlockType::Empty);
        s.local_get(IT).i32_const(0).call(close);
        s.global_get(no_throw).call(throw);
        s.end();
        s.local_get(IT).call(step);
        s.if_(BlockType::Result(jsval()));
        it(&mut s);
        s.struct_get(T_ITER, iter::F_VALUE);
        s.i32_const(0).call(result);
        s.else_();
        s.global_get(G_UNDEFINED).i32_const(1).call(result);
        s.end();
        s.return_();
    }
    s.end();
    // The method: `next` (read once, when iteration started), else `throw` / `return`.
    s.local_get(MODE).i32_const(RESUME_NEXT).i32_eq();
    s.if_(BlockType::Result(jsval()));
    it(&mut s);
    s.struct_get(T_ITER, iter::F_NEXT);
    s.else_();
    it(&mut s);
    s.struct_get(T_ITER, iter::F_OBJ);
    s.global_get(names[1]).global_get(names[2]);
    s.local_get(MODE).i32_const(RESUME_THROW).i32_eq();
    s.typed_select(runtime::string_ref());
    s.call(get);
    s.end();
    s.local_tee(M);
    s.call(is_nullish);
    s.local_get(MODE).i32_const(RESUME_NEXT).i32_ne();
    s.i32_and();
    s.if_(BlockType::Empty);
    {
        s.local_get(MODE).i32_const(RESUME_RETURN).i32_eq();
        s.if_(BlockType::Empty);
        hole(&mut s);
        s.return_();
        s.end();
        // No `throw`: close the inner iterator, then a TypeError.
        s.local_get(IT).i32_const(0).call(close);
        s.global_get(no_throw).call(throw);
    }
    s.end();
    s.local_get(M);
    it(&mut s);
    s.struct_get(T_ITER, iter::F_OBJ);
    s.local_get(V).array_new_fixed(T_VALS, 1);
    s.global_get(names[0]);
    s.global_get(names[1]).global_get(names[2]);
    s.local_get(MODE).i32_const(RESUME_THROW).i32_eq();
    s.typed_select(runtime::string_ref());
    s.local_get(MODE).i32_const(RESUME_NEXT).i32_eq();
    s.typed_select(runtime::string_ref());
    s.call(call_value).local_tee(R);
    s.ref_test_non_null(runtime::object()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(result_s);
    s.local_get(R).call(inspect).call(concat);
    s.global_get(not_obj).call(concat);
    s.call(throw);
    s.end();
    s.local_get(R);
    s.end();
    f
}
