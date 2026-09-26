//! Iteration (T15): the state behind `for-of`, spread and array destructuring.
//!
//! `$iter { kind: mut i32, obj: eqref, next: eqref, index: mut i32, value: mut eqref }`, made by
//! [`Helper::Open`]:
//! - [`ARRAY`]: an array, by index; the length is read at every step (like `%ArrayIteratorPrototype%`).
//! - [`STRING`]: a string, by code point.
//! - [`PROTOCOL`]: anything else: `obj` is the iterator `v[Symbol.iterator]()` returned, `next` its
//!   `next` method, read once.
//! - [`DONE`]: finished (or closed).
//! - [`ARRAY_LIKE`]: `Array.prototype.values` on another object (T25): `length` and `obj[i]`.
//!
//! Arrays and strings don't look at `Symbol.iterator` (a replaced `Array.prototype[Symbol.iterator]`
//! isn't seen).

use js2wasi3_ir::ErrorKind;
use wasm_encoder::{BlockType, Catch, Function, InstructionSink, ValType};

use crate::array::{self, F_ELEMS, F_LENGTH};
use crate::runtime::{
    self, Cx, G_SYM_ITERATOR, G_UNDEFINED, T_ARRAY, T_ITER, T_STRING, T_VALS, TAG, jsval, rt,
    string,
};
use crate::{func, object};

pub const ARRAY: i32 = 0;
pub const STRING: i32 = 1;
pub const PROTOCOL: i32 = 2;
pub const DONE: i32 = 3;
/// Any other object iterated by `Array.prototype.values` (T25: `arguments`, array-likes): `length`
/// and `obj[i]` are read at every step.
pub const ARRAY_LIKE: i32 = 4;

pub const F_KIND: u32 = 0;
pub const F_OBJ: u32 = 1;
pub const F_NEXT: u32 = 2;
const F_INDEX: u32 = 3;
pub const F_VALUE: u32 = 4;

pub fn iter() -> wasm_encoder::HeapType {
    wasm_encoder::HeapType::Concrete(T_ITER)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(v, message: jsval (a string)) -> jsval`: a new `$iter`; TypeError `message` if `v` isn't iterable.
    Open,
    /// `(it) -> i32`: advance; 0 when done.
    Step,
    /// `(it, throwing: i32)`: close (call the iterator's `return`) unless it's done.
    Close,
    /// `(it) -> jsval`: the remaining values as a new array.
    Rest,
    /// `(arr, v, message)`: append the values of `v` to array `arr`.
    Spread,
    /// The code of `Array.prototype.values` / `[Symbol.iterator]` and `String.prototype[Symbol.iterator]`:
    /// an iterator object (prototype `%ArrayIteratorPrototype%`) holding a `$iter` under a private symbol.
    ProtoValues,
    /// `%ArrayIteratorPrototype%.next`: `{ value, done }`.
    Next,
    /// `%ArrayIteratorPrototype%[Symbol.iterator]`: `this`.
    This,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Open => "iter_open",
            Helper::Step => "iter_step",
            Helper::Close => "iter_close",
            Helper::Rest => "iter_rest",
            Helper::Spread => "array_spread",
            Helper::ProtoValues => "Array.prototype.values",
            Helper::Next => "%ArrayIteratorPrototype%.next",
            Helper::This => "%ArrayIteratorPrototype%[Symbol.iterator]",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        match self {
            Helper::Open => (vec![v, v], vec![v]),
            Helper::Step => (vec![v], vec![ValType::I32]),
            Helper::Close => (vec![v, ValType::I32], vec![]),
            Helper::Rest => (vec![v], vec![v]),
            Helper::Spread => (vec![v, v, v], vec![]),
            Helper::ProtoValues | Helper::Next | Helper::This => (vec![v, v, v], vec![v]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Open => open(cx),
            Helper::Step => step(cx),
            Helper::Close => close(cx),
            Helper::Rest => rest(cx),
            Helper::Spread => spread(cx),
            Helper::ProtoValues => proto_values(cx),
            Helper::Next => next(cx),
            Helper::This => {
                let mut f = Function::new([]);
                f.instructions().local_get(1).end();
                f
            }
        }
    }
}

fn ih(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Iter(h))
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn it(s: &mut InstructionSink, local: u32) {
    s.local_get(local).ref_cast_non_null(iter());
}

fn null_eq(s: &mut InstructionSink) {
    s.ref_null(wasm_encoder::RefType::EQREF.heap_type);
}

fn empty_args(s: &mut InstructionSink) {
    s.i32_const(0).array_new_default(T_VALS);
}

fn open(cx: &mut dyn Cx) -> Function {
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let get = oh(cx, object::Helper::Get);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_obj = cx.str("Result of the Symbol.iterator method is not an object");
    let method = cx.str("[Symbol.iterator]");
    let next = cx.str("next");
    const V: u32 = 0;
    const MSG: u32 = 1;
    const M: u32 = 2;
    const IT: u32 = 3;
    let mut f = Function::new([(2, jsval())]);
    let mut s = f.instructions();
    let simple = |s: &mut InstructionSink, kind: i32| {
        s.i32_const(kind).local_get(V);
        null_eq(s);
        s.i32_const(0);
        null_eq(s);
        s.struct_new(T_ITER).return_();
    };
    s.local_get(V).ref_test_non_null(array::array());
    s.if_(BlockType::Empty);
    simple(&mut s, ARRAY);
    s.end();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    simple(&mut s, STRING);
    s.end();
    let fail = |s: &mut InstructionSink| {
        s.local_get(MSG).ref_cast_non_null(string()).call(throw);
    };
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    fail(&mut s);
    s.end();
    s.local_get(V)
        .global_get(G_SYM_ITERATOR)
        .call(get)
        .local_tee(M);
    s.ref_test_non_null(func::function()).i32_eqz();
    s.if_(BlockType::Empty);
    fail(&mut s);
    s.end();
    s.local_get(M).local_get(V);
    empty_args(&mut s);
    s.global_get(method).call(call_value).local_tee(IT);
    s.ref_test_non_null(runtime::object()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_obj).call(throw);
    s.end();
    s.i32_const(PROTOCOL).local_get(IT);
    s.local_get(IT).global_get(next).call(get);
    s.i32_const(0);
    null_eq(&mut s);
    s.struct_new(T_ITER);
    s.end();
    f
}

fn step(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let to_boolean = cx.helper(runtime::Helper::ToBoolean);
    let inspect = cx.helper(runtime::Helper::Inspect);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (result, not_obj) = (cx.str("Iterator result "), cx.str(" is not an object"));
    let (done, value, next) = (cx.str("done"), cx.str("value"), cx.str("next"));
    let length = cx.str("length");
    let to_number = cx.helper(runtime::Helper::ToNumber);
    const IT: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const C: u32 = 3;
    const R: u32 = 4;
    let mut f = Function::new([(3, ValType::I32), (1, jsval())]);
    let mut s = f.instructions();
    let finish = |s: &mut InstructionSink| {
        it(s, IT);
        s.i32_const(DONE).struct_set(T_ITER, F_KIND);
        s.i32_const(0).return_();
    };
    let set_value = |s: &mut InstructionSink| {
        // (value on the stack below) — callers push `it` first.
        s.struct_set(T_ITER, F_VALUE);
    };
    it(&mut s, IT);
    s.struct_get(T_ITER, F_INDEX).local_set(I);
    // Arrays.
    it(&mut s, IT);
    s.struct_get(T_ITER, F_KIND).i32_const(ARRAY).i32_eq();
    s.if_(BlockType::Empty);
    {
        s.local_get(I);
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ)
            .ref_cast_non_null(array::array())
            .struct_get(T_ARRAY, F_LENGTH);
        s.i32_ge_u();
        s.if_(BlockType::Empty);
        finish(&mut s);
        s.end();
        it(&mut s, IT);
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ)
            .ref_cast_non_null(array::array())
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(I)
            .array_get(T_VALS)
            .local_tee(R);
        s.ref_is_null();
        s.if_(BlockType::Result(jsval()));
        s.global_get(G_UNDEFINED);
        s.else_();
        s.local_get(R);
        s.end();
        set_value(&mut s);
        it(&mut s, IT);
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .struct_set(T_ITER, F_INDEX);
        s.i32_const(1).return_();
    }
    s.end();
    // Array-likes (T25): ToLength(obj.length) and obj[i] at every step.
    it(&mut s, IT);
    s.struct_get(T_ITER, F_KIND).i32_const(ARRAY_LIKE).i32_eq();
    s.if_(BlockType::Empty);
    {
        s.local_get(I).f64_convert_i32_u();
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ).global_get(length).call(get);
        s.call(to_number).f64_ge();
        s.if_(BlockType::Empty);
        finish(&mut s);
        s.end();
        it(&mut s, IT);
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ);
        s.local_get(I).ref_i31();
        s.call(get);
        set_value(&mut s);
        it(&mut s, IT);
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .struct_set(T_ITER, F_INDEX);
        s.i32_const(1).return_();
    }
    s.end();
    // Strings: one code point (a surrogate pair or a single unit).
    it(&mut s, IT);
    s.struct_get(T_ITER, F_KIND).i32_const(STRING).i32_eq();
    s.if_(BlockType::Empty);
    {
        let str_ = |s: &mut InstructionSink| {
            it(s, IT);
            s.struct_get(T_ITER, F_OBJ).ref_cast_non_null(string());
        };
        str_(&mut s);
        s.array_len().local_set(N);
        s.local_get(I).local_get(N).i32_ge_u();
        s.if_(BlockType::Empty);
        finish(&mut s);
        s.end();
        str_(&mut s);
        s.local_get(I).array_get_u(T_STRING).local_set(C);
        // A high surrogate followed by a low one: two units.
        s.local_get(C)
            .i32_const(0xFC00)
            .i32_and()
            .i32_const(0xD800)
            .i32_eq();
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .local_get(N)
            .i32_lt_u();
        s.i32_and();
        s.if_(BlockType::Result(ValType::I32));
        str_(&mut s);
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .array_get_u(T_STRING)
            .i32_const(0xFC00)
            .i32_and()
            .i32_const(0xDC00)
            .i32_eq();
        s.else_();
        s.i32_const(0);
        s.end();
        s.i32_const(1).i32_add().local_set(N); // N = units in this code point
        it(&mut s, IT);
        s.local_get(N).array_new_default(T_STRING);
        s.local_tee(R);
        s.ref_cast_non_null(string()).i32_const(0);
        str_(&mut s);
        s.local_get(I).local_get(N);
        s.array_copy(T_STRING, T_STRING);
        s.local_get(R);
        set_value(&mut s);
        it(&mut s, IT);
        s.local_get(I)
            .local_get(N)
            .i32_add()
            .struct_set(T_ITER, F_INDEX);
        s.i32_const(1).return_();
    }
    s.end();
    // Done.
    it(&mut s, IT);
    s.struct_get(T_ITER, F_KIND).i32_const(DONE).i32_eq();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    // The protocol: r = next.call(obj); r must be an object.
    it(&mut s, IT);
    s.struct_get(T_ITER, F_NEXT);
    it(&mut s, IT);
    s.struct_get(T_ITER, F_OBJ);
    empty_args(&mut s);
    s.global_get(next).call(call_value).local_tee(R);
    s.ref_test_non_null(runtime::object()).i32_eqz();
    s.if_(BlockType::Empty);
    it(&mut s, IT);
    s.i32_const(DONE).struct_set(T_ITER, F_KIND);
    s.global_get(result);
    s.local_get(R).call(inspect).call(concat);
    s.global_get(not_obj).call(concat);
    s.call(throw);
    s.end();
    s.local_get(R).global_get(done).call(get).call(to_boolean);
    s.if_(BlockType::Empty);
    finish(&mut s);
    s.end();
    it(&mut s, IT);
    s.local_get(R).global_get(value).call(get);
    set_value(&mut s);
    s.i32_const(1);
    s.end();
    f
}

fn close(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let inspect = cx.helper(runtime::Helper::Inspect);
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (result, not_obj) = (cx.str("Iterator result "), cx.str(" is not an object"));
    let ret = cx.str("return");
    const IT: u32 = 0;
    const THROWING: u32 = 1;
    const M: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([(2, jsval())]);
    let mut s = f.instructions();
    it(&mut s, IT);
    s.struct_get(T_ITER, F_KIND).i32_const(PROTOCOL).i32_ne();
    s.if_(BlockType::Empty);
    it(&mut s, IT);
    s.i32_const(DONE).struct_set(T_ITER, F_KIND);
    s.return_();
    s.end();
    it(&mut s, IT);
    s.i32_const(DONE).struct_set(T_ITER, F_KIND);
    // Errors while closing because of a throw are dropped: the original exception wins.
    s.block(BlockType::Empty);
    s.block(BlockType::Result(jsval()));
    s.try_table(BlockType::Empty, [Catch::One { tag: TAG, label: 0 }]);
    {
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ)
            .global_get(ret)
            .call(get)
            .local_tee(M);
        s.call(is_nullish);
        s.if_(BlockType::Empty);
        s.return_();
        s.end();
        s.local_get(M);
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ);
        empty_args(&mut s);
        s.global_get(ret).call(call_value).local_set(R);
        s.local_get(THROWING).i32_eqz();
        s.local_get(R)
            .ref_test_non_null(runtime::object())
            .i32_eqz();
        s.i32_and();
        s.if_(BlockType::Empty);
        s.global_get(result);
        s.local_get(R).call(inspect).call(concat);
        s.global_get(not_obj).call(concat);
        s.call(throw);
        s.end();
    }
    s.end(); // try_table
    s.br(1);
    s.end();
    // Caught: rethrow unless we're closing because of a throw.
    s.local_set(R);
    s.local_get(THROWING).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(R).throw(TAG);
    s.end();
    s.end();
    s.end();
    f
}

fn rest(cx: &mut dyn Cx) -> Function {
    let step = ih(cx, Helper::Step);
    let new = array::ah(cx, array::Helper::New);
    let push = array::ah(cx, array::Helper::Push);
    const IT: u32 = 0;
    const A: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_ARRAY))]);
    let mut s = f.instructions();
    s.i32_const(0)
        .array_new_default(T_VALS)
        .i32_const(0)
        .call(new)
        .local_set(A);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(IT).call(step).i32_eqz().br_if(1);
    s.local_get(A).ref_as_non_null();
    it(&mut s, IT);
    s.struct_get(T_ITER, F_VALUE);
    s.call(push);
    s.br(0).end().end();
    s.local_get(A);
    s.end();
    f
}

fn spread(cx: &mut dyn Cx) -> Function {
    let open = ih(cx, Helper::Open);
    let step = ih(cx, Helper::Step);
    let push = array::ah(cx, array::Helper::Push);
    const ARR: u32 = 0;
    const V: u32 = 1;
    const MSG: u32 = 2;
    const IT: u32 = 3;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.local_get(V).local_get(MSG).call(open).local_set(IT);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(IT).call(step).i32_eqz().br_if(1);
    s.local_get(ARR).ref_cast_non_null(array::array());
    it(&mut s, IT);
    s.struct_get(T_ITER, F_VALUE);
    s.call(push);
    s.br(0).end().end();
    s.end();
    f
}

fn proto_values(cx: &mut dyn Cx) -> Function {
    let open = ih(cx, Helper::Open);
    let alloc = oh(cx, object::Helper::Alloc);
    let define = oh(cx, object::Helper::Define);
    let proto = oh(
        cx,
        object::Helper::Proto(js2wasi3_ir::Builtin::ArrayIterator),
    );
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let msg = cx.str("values() is only implemented for objects and strings (js2wasi3)");
    const THIS: u32 = 1;
    const O: u32 = 3;
    let mut f = Function::new([(1, rt(true, runtime::T_OBJECT))]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(runtime::object());
    runtime::is_string(&mut s, THIS);
    s.i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(msg).call(throw);
    s.end();
    s.i32_const(object::CLASS_PLAIN)
        .call(proto)
        .call(alloc)
        .local_tee(O)
        .ref_as_non_null();
    s.global_get(runtime::G_SYM_STATE);
    // Arrays and strings: `Open`'s fast paths; other objects by `length` and index (T25).
    s.local_get(THIS).ref_test_non_null(array::array());
    runtime::is_string(&mut s, THIS);
    s.i32_or();
    s.if_(BlockType::Result(jsval()));
    s.local_get(THIS).global_get(msg).call(open);
    s.else_();
    s.i32_const(ARRAY_LIKE);
    s.local_get(THIS);
    s.global_get(G_UNDEFINED);
    s.i32_const(0);
    s.global_get(G_UNDEFINED);
    s.struct_new(T_ITER);
    s.end();
    s.i32_const(object::HIDDEN).call(define);
    s.local_get(O);
    s.end();
    f
}

fn next(cx: &mut dyn Cx) -> Function {
    let step = ih(cx, Helper::Step);
    let own = oh(cx, object::Helper::OwnGet);
    let new_object = oh(cx, object::Helper::NewObject);
    let define = oh(cx, object::Helper::DefineProp);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let msg = cx.str("next method called on incompatible receiver");
    let (value, done) = (cx.str("value"), cx.str("done"));
    const THIS: u32 = 1;
    const ST: u32 = 3;
    const R: u32 = 4;
    const MORE: u32 = 5;
    let mut f = Function::new([(2, jsval()), (1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(THIS).ref_test_non_null(runtime::object());
    s.if_(BlockType::Empty);
    s.local_get(THIS)
        .ref_cast_non_null(runtime::object())
        .global_get(runtime::G_SYM_STATE)
        .call(own)
        .local_set(ST);
    s.end();
    s.local_get(ST).ref_test_non_null(iter()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(msg).call(throw);
    s.end();
    s.local_get(ST).call(step).local_set(MORE);
    s.call(new_object).local_set(R);
    s.local_get(R).global_get(value);
    s.local_get(MORE);
    s.if_(BlockType::Result(jsval()));
    it(&mut s, ST);
    s.struct_get(T_ITER, F_VALUE);
    s.else_();
    s.global_get(G_UNDEFINED);
    s.end();
    s.call(define);
    s.local_get(R).global_get(done);
    s.local_get(MORE);
    s.if_(BlockType::Result(jsval()));
    s.global_get(runtime::G_FALSE);
    s.else_();
    s.global_get(runtime::G_TRUE);
    s.end();
    s.call(define);
    s.local_get(R);
    s.end();
    f
}
