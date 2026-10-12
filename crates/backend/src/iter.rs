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
//! - [`ARRAY_KEYS`], [`ARRAY_ENTRIES`]: `Array.prototype.keys`/`entries` (T25), the same way.
//!
//! Arrays and strings skip the protocol while the built-in iteration is intact (T102: watched
//! entries, `object::WATCH_ITER`); a replaced `[Symbol.iterator]` or `next` makes them use it.

use crate::runtime::JsTest;
use js2wasi3_ir::ErrorKind;
use wasm_encoder::{BlockType, Catch, Function, InstructionSink, ValType};

use crate::array::{self, F_LENGTH};
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
/// `Array.prototype.keys` / `entries` on any object (T25): like [`ARRAY_LIKE`], the value is the
/// index / `[index, obj[index]]`.
pub const ARRAY_KEYS: i32 = 5;
pub const ARRAY_ENTRIES: i32 = 6;

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
    /// `Array.prototype.keys` / `entries` (T25): the same iterator objects.
    ProtoKeys,
    ProtoEntries,
    /// `%TypedArray%.prototype.values` / `keys` / `entries` in a module with resizable buffers
    /// (T47; the kind: [`ARRAY_LIKE`], [`ARRAY_KEYS`], [`ARRAY_ENTRIES`]): the same, after
    /// ValidateTypedArray (elsewhere they're the `Array.prototype` functions).
    TypedProto(i32),
    /// `%ArrayIteratorPrototype%.next`: `{ value, done }`.
    Next,
    /// `%ArrayIteratorPrototype%[Symbol.iterator]`: `this`.
    This,
    /// `for await` (T20), see `Intrinsic::AsyncIterOpen` & co. An async iterator's state is a
    /// `$vals` `[iterator, next]`; a sync one's is a `$iter`.
    AsyncOpen,
    IsAsync,
    AsyncNext,
    AsyncResult,
    AsyncReturn,
    HasReturn,
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
            Helper::ProtoKeys => "Array.prototype.keys",
            Helper::ProtoEntries => "Array.prototype.entries",
            Helper::TypedProto(ARRAY_KEYS) => "%TypedArray%.prototype.keys",
            Helper::TypedProto(ARRAY_ENTRIES) => "%TypedArray%.prototype.entries",
            Helper::TypedProto(_) => "%TypedArray%.prototype.values",
            Helper::Next => "%ArrayIteratorPrototype%.next",
            Helper::This => "%ArrayIteratorPrototype%[Symbol.iterator]",
            Helper::AsyncOpen => "async_iter_open",
            Helper::IsAsync => "is_async_iter",
            Helper::AsyncNext => "async_iter_next",
            Helper::AsyncResult => "async_iter_result",
            Helper::AsyncReturn => "async_iter_return",
            Helper::HasReturn => "iter_has_return",
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
            Helper::ProtoValues
            | Helper::ProtoKeys
            | Helper::ProtoEntries
            | Helper::TypedProto(_)
            | Helper::Next
            | Helper::This => (vec![v, v, v], vec![v]),
            Helper::AsyncOpen => (vec![v, v], vec![v]),
            Helper::IsAsync | Helper::HasReturn => (vec![v], vec![ValType::I32]),
            Helper::AsyncNext | Helper::AsyncResult => (vec![v], vec![v]),
            Helper::AsyncReturn => (vec![v, ValType::I32], vec![v]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Open => open(cx),
            Helper::Step => step(cx),
            Helper::Close => close(cx),
            Helper::Rest => rest(cx),
            Helper::Spread => spread(cx),
            Helper::ProtoValues => proto_values(cx, ARRAY_LIKE, false),
            Helper::ProtoKeys => proto_values(cx, ARRAY_KEYS, false),
            Helper::ProtoEntries => proto_values(cx, ARRAY_ENTRIES, false),
            Helper::TypedProto(kind) => proto_values(cx, kind, true),
            Helper::Next => next(cx),
            Helper::This => {
                let mut f = Function::new([]);
                f.instructions().local_get(1).end();
                f
            }
            Helper::AsyncOpen => async_open(cx),
            Helper::IsAsync => {
                let mut f = Function::new([]);
                f.instructions()
                    .local_get(0)
                    .ref_test_non_null(wasm_encoder::HeapType::Concrete(T_VALS))
                    .end();
                f
            }
            Helper::AsyncNext => {
                let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
                let what = cx.str("next");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                record(&mut s, 0, 1);
                record(&mut s, 0, 0);
                empty_args(&mut s);
                s.global_get(what).call(call_value);
                s.end();
                f
            }
            Helper::AsyncResult => {
                let inspect = cx.helper(runtime::Helper::Inspect);
                let concat = cx.helper(runtime::Helper::Concat);
                let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
                let (result, not_obj) = (cx.str("Iterator result "), cx.str(" is not an object"));
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.test_object(0)
                    .i32_eqz();
                s.if_(BlockType::Empty);
                s.global_get(result);
                s.local_get(0).call(inspect).call(concat);
                s.global_get(not_obj).call(concat).call(throw);
                s.end();
                s.local_get(0);
                s.end();
                f
            }
            Helper::AsyncReturn => async_return(cx),
            Helper::HasReturn => {
                let get = oh(cx, object::Helper::Get);
                let is_nullish = cx.helper(runtime::Helper::IsNullish);
                let ret = cx.str("return");
                let mut f = Function::new([]);
                let mut s = f.instructions();
                it(&mut s, 0);
                s.struct_get(T_ITER, F_KIND).i32_const(PROTOCOL).i32_ne();
                s.if_(BlockType::Empty);
                s.i32_const(0).return_();
                s.end();
                it(&mut s, 0);
                s.struct_get(T_ITER, F_OBJ).global_get(ret).call(get);
                s.call(is_nullish).i32_eqz();
                s.end();
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

/// Push element `i` of the async iteration record in local `st` (`[iterator, next]`).
fn record(s: &mut InstructionSink, st: u32, i: i32) {
    s.local_get(st)
        .ref_cast_non_null(wasm_encoder::HeapType::Concrete(T_VALS))
        .i32_const(i)
        .array_get(T_VALS);
}

/// GetIterator(v, async): `v[Symbol.asyncIterator]()` and its `next` as `[iterator, next]`, or
/// the sync iteration state when `v` has no async iterator method.
fn async_open(cx: &mut dyn Cx) -> Function {
    let open = ih(cx, Helper::Open);
    let get = oh(cx, object::Helper::Get);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let typeof_string = cx.helper(runtime::Helper::TypeofString);
    let describe = cx.helper(runtime::Helper::Prom(crate::promise::Helper::Describe));
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let sym = cx.helper(runtime::Helper::WellKnownSymbol(
        js2wasi3_ir::well_known_symbol("asyncIterator").expect("well-known"),
    ));
    let (space, not_fn, not_obj, next, what) = (
        cx.str(" "),
        cx.str(" is not a function"),
        cx.str("Result of the Symbol.asyncIterator method is not an object"),
        cx.str("next"),
        cx.str("[Symbol.asyncIterator]"),
    );
    const V: u32 = 0;
    const MSG: u32 = 1;
    const M: u32 = 2;
    const IT: u32 = 3;
    let mut f = Function::new([(2, jsval())]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.local_get(V).local_get(MSG).call(open).return_();
    s.end();
    s.local_get(V).call(sym).call(get).local_tee(M);
    s.call(is_nullish);
    s.if_(BlockType::Empty);
    s.local_get(V).local_get(MSG).call(open).return_();
    s.end();
    s.local_get(M).ref_test_non_null(func::function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(M).call(typeof_string);
    s.global_get(space).call(concat);
    s.local_get(M).call(describe).call(concat);
    s.global_get(not_fn).call(concat).call(throw);
    s.end();
    s.local_get(M).local_get(V);
    empty_args(&mut s);
    s.global_get(what).call(call_value).local_tee(IT);
    s.is_object().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_obj).call(throw);
    s.end();
    s.local_get(IT);
    s.local_get(IT).global_get(next).call(get);
    s.array_new_fixed(T_VALS, 2);
    s.end();
    f
}

/// AsyncIteratorClose's call: `iterator.return()` (to await), or Wasm null when there's no
/// `return` (or, when `throwing`, when getting or calling it threw: the original exception wins).
fn async_return(cx: &mut dyn Cx) -> Function {
    let get = oh(cx, object::Helper::Get);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let call_value = cx.helper(runtime::Helper::Func(func::Helper::CallValue));
    let ret = cx.str("return");
    const ST: u32 = 0;
    const THROWING: u32 = 1;
    const M: u32 = 2;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.block(BlockType::Result(jsval()));
    s.try_table(BlockType::Empty, [Catch::One { tag: TAG, label: 0 }]);
    record(&mut s, ST, 0);
    s.global_get(ret).call(get).local_tee(M);
    s.call(is_nullish);
    s.if_(BlockType::Empty);
    null_eq(&mut s);
    s.return_();
    s.end();
    s.local_get(M);
    record(&mut s, ST, 0);
    empty_args(&mut s);
    s.global_get(ret).call(call_value).return_();
    s.end();
    s.unreachable();
    s.end();
    // Caught.
    s.local_set(M);
    s.local_get(THROWING);
    s.if_(BlockType::Empty);
    null_eq(&mut s);
    s.return_();
    s.end();
    s.local_get(M).throw(TAG);
    s.end();
    f
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
    // Arrays and strings by index while the built-in iteration is intact (T102: `WATCH_ITER` covers
    // `Array.prototype[Symbol.iterator]` and `%ArrayIteratorPrototype%.next`, `WATCH_STRITER`
    // `String.prototype[Symbol.iterator]`). An array qualifies with `Array.prototype` as its prototype and
    // no own properties (an own `[Symbol.iterator]`); anything else takes the protocol.
    let intact = |s: &mut InstructionSink, bit: i32| {
        s.global_get(runtime::G_INTACT)
            .i32_const(bit >> object::WATCH_SHIFT)
            .i32_and()
            .i32_const(0)
            .i32_ne();
    };
    s.local_get(V).ref_test_non_null(array::array());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(runtime::T_ARRAY, object::F_PROTO)
        .global_get(runtime::G_PROTOS + js2wasi3_ir::Builtin::Array.index())
        .ref_eq();
    s.local_get(V)
        .ref_cast_non_null(array::array())
        .struct_get(runtime::T_ARRAY, object::F_COUNT)
        .i32_eqz()
        .i32_and();
    intact(&mut s, object::WATCH_ITER);
    s.i32_and();
    s.else_();
    s.i32_const(0);
    s.end();
    s.if_(BlockType::Empty);
    simple(&mut s, ARRAY);
    s.end();
    runtime::is_string(&mut s, V);
    intact(&mut s, object::WATCH_STRITER);
    s.i32_and();
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
    s.is_object().i32_eqz();
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
    let elem_at = cx.helper(runtime::Helper::Arr(crate::array::Helper::ElemAt));
    let get = oh(cx, object::Helper::Get);
    let prop_value = cx.helper(runtime::Helper::Prop(crate::props::Helper::Value));
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
    const AR: u32 = 5;
    const NM: u32 = 6;
    let mut f = Function::new([
        (3, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_ARRAY)),
        (1, rt(true, runtime::T_NUMS)),
    ]);
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
        s.struct_get(T_ITER, F_OBJ).ref_cast_non_null(array::array()).local_tee(AR);
        // T107: generic arrays read `elems` directly; number mode boxes (`elem_at`).
        s.struct_get(T_ARRAY, array::F_NUMS).local_set(NM);
        array::is_num_mode(&mut s, NM);
        s.i32_eqz();
        s.if_(BlockType::Result(jsval()));
        s.local_get(AR).struct_get(T_ARRAY, array::F_ELEMS).local_get(I).array_get(T_VALS);
        s.else_();
        s.local_get(AR).ref_as_non_null().local_get(I).call(elem_at);
        s.end();
        s.local_tee(R);
        // A hole reads through the prototype chain (T33); an accessor element (T32) calls its
        // getter.
        s.ref_is_null();
        s.if_(BlockType::Result(jsval()));
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ).local_get(I).ref_i31().call(get);
        s.else_();
        s.local_get(R);
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ).call(prop_value);
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
    // (`keys`/`entries` iterators only when the code names them, T25.)
    let keys_entries = cx.names("keys") || cx.names("entries");
    s.struct_get(T_ITER, F_KIND)
        .local_tee(N)
        .i32_const(ARRAY_LIKE);
    if keys_entries {
        s.i32_ge_s();
    } else {
        s.i32_eq();
    }
    s.if_(BlockType::Empty);
    {
        if cx.rab() {
            // T47: a typed array out of bounds (or on a detached buffer) throws.
            let oob = cx.helper(runtime::Helper::Typed(crate::typed::Helper::Op(
                js2wasi3_ir::TypedOp::OutOfBounds,
            )));
            let msg = cx.str("Cannot perform Array Iterator.prototype.next on a detached ArrayBuffer");
            it(&mut s, IT);
            s.struct_get(T_ITER, F_OBJ).ref_test_non_null(crate::typed::typed());
            s.if_(BlockType::Empty);
            it(&mut s, IT);
            s.struct_get(T_ITER, F_OBJ).call(oob);
            s.if_(BlockType::Empty);
            s.global_get(msg).call(throw);
            s.end();
            s.end();
        }
        s.local_get(I).f64_convert_i32_u();
        it(&mut s, IT);
        s.struct_get(T_ITER, F_OBJ).global_get(length).call(get);
        s.call(to_number).f64_ge();
        s.if_(BlockType::Empty);
        finish(&mut s);
        s.end();
        it(&mut s, IT);
        if keys_entries {
            // keys: the index; values: obj[i]; entries: [i, obj[i]].
            let new_array = cx.helper(runtime::Helper::Arr(array::Helper::NewGeneric));
            s.local_get(N).i32_const(ARRAY_KEYS).i32_eq();
            s.if_(BlockType::Result(jsval()));
            s.local_get(I).ref_i31();
            s.else_();
            it(&mut s, IT);
            s.struct_get(T_ITER, F_OBJ);
            s.local_get(I).ref_i31();
            s.call(get).local_set(R);
            s.local_get(N).i32_const(ARRAY_ENTRIES).i32_eq();
            s.if_(BlockType::Empty);
            s.local_get(I).ref_i31().local_get(R);
            s.array_new_fixed(T_VALS, 2).i32_const(2).call(new_array);
            s.local_set(R);
            s.end();
            s.local_get(R);
            s.end();
        } else {
            it(&mut s, IT);
            s.struct_get(T_ITER, F_OBJ);
            s.local_get(I).ref_i31();
            s.call(get);
        }
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
    s.is_object().i32_eqz();
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
        s.test_object(R)
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
    let new = array::ah(cx, array::Helper::NewGeneric);
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

/// `kind`: [`ARRAY_LIKE`] (`values`: arrays and strings take `Open`'s fast paths), [`ARRAY_KEYS`]
/// or [`ARRAY_ENTRIES`] (any object). `typed`: `%TypedArray%.prototype`'s (T47).
fn proto_values(cx: &mut dyn Cx, kind: i32, typed: bool) -> Function {
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
    s.test_object(THIS);
    runtime::is_string(&mut s, THIS);
    s.i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(msg).call(throw);
    s.end();
    if typed {
        // T47: ValidateTypedArray.
        let this_typed = cx.str("this is not a typed array.");
        let oob = cx.helper(runtime::Helper::Typed(crate::typed::Helper::Op(
            js2wasi3_ir::TypedOp::OutOfBounds,
        )));
        let name = match kind {
            ARRAY_KEYS => "keys",
            ARRAY_ENTRIES => "entries",
            _ => "values",
        };
        let msg = cx.str(&format!(
            "Cannot perform %TypedArray%.prototype.{name} on a detached ArrayBuffer"
        ));
        s.local_get(THIS).ref_test_non_null(crate::typed::typed()).i32_eqz();
        s.if_(BlockType::Empty);
        s.global_get(this_typed).call(throw);
        s.end();
        s.local_get(THIS).call(oob);
        s.if_(BlockType::Empty);
        s.global_get(msg).call(throw);
        s.end();
    }
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
    s.i32_const((kind == ARRAY_LIKE) as i32).i32_and();
    s.if_(BlockType::Result(jsval()));
    // (Not through `Open`: that would look up a replaced `[Symbol.iterator]`.)
    s.i32_const(ARRAY).i32_const(STRING);
    s.local_get(THIS).ref_test_non_null(array::array());
    s.select();
    s.local_get(THIS);
    null_eq(&mut s);
    s.i32_const(0);
    null_eq(&mut s);
    s.struct_new(T_ITER);
    s.else_();
    s.i32_const(kind);
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
    s.test_object(THIS);
    s.if_(BlockType::Empty);
    s.local_get(THIS)
        .cast_object()
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
