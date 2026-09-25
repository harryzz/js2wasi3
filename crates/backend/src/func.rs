//! Function objects and calls through values (T14, design D1).
//!
//! # Layout
//! `$function` is a final subtype of `$object`: the seven object fields, then `code: (ref $code)` and
//! `env: (ref null $vals)`. Every object helper takes a function unchanged (`ref.test $object` matches
//! it); only `typeof`, `console.log` and calls look at the extra fields.
//! - `code` has the uniform type `$code = (self, this, args: eqref) -> eqref`: `self` is the function
//!   object, `args` a `$vals` array with exactly the arguments passed. The lowering generates the code
//!   of each function used as a value (an adapter calling the typed function, see `crates/lower`).
//! - `env` holds what the function captured: cells (`$cell_f64`, `$cell`) and plain values.
//! - Own properties, made with the object: `length` and `name` (non-enumerable), and for a
//!   `function` (not an arrow function or a method) `prototype`, a new object whose `constructor` is
//!   the function. (Read-only attributes: F36.)
//!
//! # Function.prototype
//! Built like the other prototypes ([`crate::object`]): `length: 0`, `name: ""`, sentinels for
//! `apply`, `bind`, `toString`, and a real `call` ([`Helper::Call`]).

use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, HeapType, ValType};

use crate::object;
use crate::runtime::{
    self, Cx, G_UNDEFINED, T_BYTES, T_CODE, T_FUNCTION, T_I32S, T_KEYS, T_OBJECT, T_VALS, jsval,
    rt, string_ref,
};

/// `$function` field indices after the object's.
pub const F_CODE: u32 = 7;
pub const F_ENV: u32 = 8;

pub fn function() -> HeapType {
    HeapType::Concrete(T_FUNCTION)
}

pub fn vals() -> HeapType {
    HeapType::Concrete(T_VALS)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(code: (ref $code), env: (ref null $vals), name: (ref $string), length: i32, ctor: i32) -> jsval`:
    /// a new function object (see the module docs).
    New,
    /// `(f, this: jsval, args: (ref $vals), what: (ref $string)) -> jsval`: call `f`, or throw
    /// the TypeError "`what` is not a function".
    CallValue,
    /// `(args: jsval, i: i32) -> jsval`: argument `i`, `undefined` past the end.
    ArgGet,
    /// The code of `Function.prototype.call` (a `$code`): call `this` with `args[0]` as `this`
    /// and the other arguments.
    Call,
    /// `Function.prototype.apply` (T15): call `this` with `args[0]` as `this` and the elements of the
    /// array-like `args[1]` as the arguments.
    Apply,
    /// `(f, this, args: jsval (an array-like), what: (ref $string)) -> jsval`: `f(...args)`.
    CallList,
    /// `(v: jsval) -> (ref $vals)`: CreateListFromArrayLike (`undefined`/`null`: empty).
    ListFromArrayLike,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_function",
            Helper::CallValue => "call_value",
            Helper::ArgGet => "arg_get",
            Helper::Call => "Function.prototype.call",
            Helper::Apply => "Function.prototype.apply",
            Helper::CallList => "call_list",
            Helper::ListFromArrayLike => "list_from_array_like",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let s = string_ref();
        match self {
            Helper::New => (
                vec![
                    rt(false, T_CODE),
                    rt(true, T_VALS),
                    s,
                    ValType::I32,
                    ValType::I32,
                ],
                vec![v],
            ),
            Helper::CallValue => (vec![v, v, rt(false, T_VALS), s], vec![v]),
            Helper::ArgGet => (vec![v, ValType::I32], vec![v]),
            Helper::Call | Helper::Apply => (vec![v, v, v], vec![v]),
            Helper::CallList => (vec![v, v, v, s], vec![v]),
            Helper::ListFromArrayLike => (vec![v], vec![rt(false, T_VALS)]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::CallValue => call_value(cx),
            Helper::ArgGet => arg_get(),
            Helper::Call => call(cx),
            Helper::Apply => apply(cx),
            Helper::CallList => call_list(cx),
            Helper::ListFromArrayLike => list_from_array_like(cx),
        }
    }
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn fh(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Func(h))
}

fn new(cx: &mut dyn Cx) -> Function {
    let proto = oh(cx, object::Helper::Proto(Builtin::Function));
    let obj_proto = oh(cx, object::Helper::Proto(Builtin::Object));
    let alloc = oh(cx, object::Helper::Alloc);
    let define = oh(cx, object::Helper::Define);
    let (length, name, prototype, constructor) = (
        cx.str("length"),
        cx.str("name"),
        cx.str("prototype"),
        cx.str("constructor"),
    );
    const CODE: u32 = 0;
    const ENV: u32 = 1;
    const NAME: u32 = 2;
    const LEN: u32 = 3;
    const CTOR: u32 = 4;
    const F: u32 = 5;
    const P: u32 = 6;
    let mut f = Function::new([(1, rt(true, T_FUNCTION)), (1, rt(true, T_OBJECT))]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(CODE).local_get(ENV);
    s.struct_new(T_FUNCTION).local_set(F);
    s.local_get(F).ref_as_non_null().global_get(length);
    s.local_get(LEN).ref_i31();
    s.i32_const(object::HIDDEN).call(define);
    s.local_get(F)
        .ref_as_non_null()
        .global_get(name)
        .local_get(NAME);
    s.i32_const(object::HIDDEN).call(define);
    s.local_get(CTOR);
    s.if_(BlockType::Empty);
    s.i32_const(object::CLASS_PLAIN)
        .call(obj_proto)
        .call(alloc)
        .local_tee(P)
        .ref_as_non_null();
    s.global_get(constructor).local_get(F);
    s.i32_const(object::HIDDEN).call(define);
    s.local_get(F).ref_as_non_null().global_get(prototype);
    s.local_get(P);
    s.i32_const(object::HIDDEN).call(define);
    s.end();
    s.local_get(F);
    s.end();
    f
}

fn call_value(cx: &mut dyn Cx) -> Function {
    let concat = cx.helper(runtime::Helper::Concat);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_fn = cx.str(" is not a function");
    const F: u32 = 0;
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const WHAT: u32 = 3;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(F).ref_test_non_null(function()).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(WHAT)
        .global_get(not_fn)
        .call(concat)
        .call(throw);
    s.unreachable();
    s.end();
    s.local_get(F).local_get(THIS).local_get(ARGS);
    s.local_get(F)
        .ref_cast_non_null(function())
        .struct_get(T_FUNCTION, F_CODE);
    s.call_ref(T_CODE);
    s.end();
    f
}

fn arg_get() -> Function {
    const ARGS: u32 = 0;
    const I: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(I);
    s.local_get(ARGS).ref_cast_non_null(vals()).array_len();
    s.i32_lt_u();
    s.if_(BlockType::Result(jsval()));
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .local_get(I)
        .array_get(T_VALS);
    s.else_();
    s.global_get(G_UNDEFINED);
    s.end();
    s.end();
    f
}

/// `Function.prototype.call`: `this` is the function to call.
fn call(cx: &mut dyn Cx) -> Function {
    let call_value = fh(cx, Helper::CallValue);
    let what = cx.str("Function.prototype.call called on a value that");
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    const N: u32 = 3;
    const REST: u32 = 4;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .array_len()
        .local_set(N);
    // rest = args[1..]
    s.i32_const(0);
    s.local_get(N).i32_const(1).i32_sub();
    s.local_get(N).i32_eqz().select();
    s.array_new_default(T_VALS).local_set(REST);
    s.local_get(N).i32_const(1).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(REST).ref_as_non_null().i32_const(0);
    s.local_get(ARGS).ref_cast_non_null(vals()).i32_const(1);
    s.local_get(N).i32_const(1).i32_sub();
    s.array_copy(T_VALS, T_VALS);
    s.end();
    s.local_get(THIS);
    s.local_get(N).i32_eqz();
    s.if_(BlockType::Result(jsval()));
    s.global_get(G_UNDEFINED);
    s.else_();
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .i32_const(0)
        .array_get(T_VALS);
    s.end();
    s.local_get(REST).ref_as_non_null();
    s.global_get(what);
    s.call(call_value);
    s.end();
    f
}

fn apply(cx: &mut dyn Cx) -> Function {
    let call_list = fh(cx, Helper::CallList);
    let what = cx.str("Function.prototype.apply was called on a value that");
    const THIS: u32 = 1;
    const ARGS: u32 = 2;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(THIS);
    for i in 0..2 {
        s.i32_const(i);
        s.local_get(ARGS).ref_cast_non_null(vals()).array_len();
        s.i32_lt_u();
        s.if_(BlockType::Result(jsval()));
        s.local_get(ARGS)
            .ref_cast_non_null(vals())
            .i32_const(i)
            .array_get(T_VALS);
        s.else_();
        s.global_get(G_UNDEFINED);
        s.end();
    }
    s.global_get(what);
    s.call(call_list);
    s.end();
    f
}

fn call_list(cx: &mut dyn Cx) -> Function {
    let list = fh(cx, Helper::ListFromArrayLike);
    let call_value = fh(cx, Helper::CallValue);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).local_get(1);
    s.local_get(2).call(list);
    s.local_get(3).call(call_value);
    s.end();
    f
}

/// Arrays copy their elements (holes: `undefined`); other objects: `length` and indices.
fn list_from_array_like(cx: &mut dyn Cx) -> Function {
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let get = oh(cx, object::Helper::Get);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let not_obj = cx.str("CreateListFromArrayLike called on non-object");
    let length = cx.str("length");
    const V: u32 = 0;
    const N: u32 = 1;
    const I: u32 = 2;
    const OUT: u32 = 3;
    const E: u32 = 4;
    let mut f = Function::new([(2, ValType::I32), (1, rt(true, T_VALS)), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.i32_const(0).array_new_default(T_VALS).return_();
    s.end();
    s.local_get(V)
        .ref_test_non_null(runtime::object())
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(not_obj).call(throw);
    s.end();
    s.local_get(V).ref_test_non_null(crate::array::array());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(V)
        .ref_cast_non_null(crate::array::array())
        .struct_get(runtime::T_ARRAY, crate::array::F_LENGTH);
    s.else_();
    s.local_get(V).global_get(length).call(get).call(to_number);
    s.i32_trunc_sat_f64_u();
    s.end();
    s.local_tee(N).array_new_default(T_VALS).local_set(OUT);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT).ref_as_non_null().local_get(I);
    s.local_get(V).local_get(I).ref_i31().call(get).local_tee(E);
    s.array_set(T_VALS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}
