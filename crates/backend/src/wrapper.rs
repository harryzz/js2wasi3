//! Primitive wrapper objects (T30): `new String("ab")`, `new Number(1)`, `new Boolean(false)`,
//! `Object(prim)`.
//!
//! A wrapper is a `$wrapper`, a final subtype of `$object` with one more field, `prim` (the
//! [[StringData]] / [[NumberData]] / [[BooleanData]] / [[SymbolData]]), so every object path
//! (properties, prototype, `typeof`, identity) works unchanged and `ref.test` tells wrappers
//! apart. A String wrapper is made with its exotic own properties as ordinary ones: the indices
//! (enumerable, read-only, non-configurable) and `length` (hidden too), so keys, `for-in`,
//! `hasOwnProperty` and failed writes need nothing new.

use crate::runtime::JsTest;
use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, HeapType, ValType};

use crate::object::{self, CLASS_WRAPPER, FIXED, HIDDEN, READONLY};
use crate::runtime::{
    self, Cx, G_FALSE, G_TRUE, G_UNDEFINED, T_BYTES, T_I32S, T_KEYS, T_OBJECT, T_STRING, T_VALS,
    T_WRAPPER, jsval, object as object_ty, rt, string, symbol,
};

/// The `prim` field.
pub const F_PRIM: u32 = 7;

pub fn wrapper() -> HeapType {
    HeapType::Concrete(T_WRAPPER)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(prim: jsval, proto: (ref null $object)) -> jsval`: a new wrapper of `prim`.
    New,
    /// `(v) -> jsval`: ToObject: objects unchanged, primitives wrapped, TypeError for
    /// `undefined`/`null`.
    ToObject,
    /// `(v) -> jsval`: the primitive of a wrapper, anything else unchanged.
    Unwrap,
    /// `(v) -> jsval`: a wrapper's class name (`String`, `Number`, `Boolean`, `Symbol`: its
    /// builtinTag, or `Symbol.prototype[Symbol.toStringTag]`), `undefined` for anything else.
    Tag,
    /// The construct code of the `String`/`Number`/`Boolean` value: `(self, new_target, args)`:
    /// the conversion (the prelude constructor called as a function), wrapped, with the prototype
    /// from `new_target`.
    Construct(Builtin),
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_wrapper",
            Helper::ToObject => "to_object",
            Helper::Unwrap => "unwrap",
            Helper::Tag => "wrapper_tag",
            Helper::Construct(_) => "wrapper_construct",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        match self {
            Helper::New => (vec![v, rt(true, T_OBJECT)], vec![v]),
            Helper::ToObject | Helper::Unwrap | Helper::Tag => (vec![v], vec![v]),
            Helper::Construct(_) => (vec![v, v, v], vec![v]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::ToObject => to_object(cx),
            Helper::Unwrap => {
                let mut f = Function::new([]);
                let mut s = f.instructions();
                s.local_get(0).ref_test_non_null(wrapper());
                s.if_(BlockType::Result(jsval()));
                s.local_get(0)
                    .ref_cast_non_null(wrapper())
                    .struct_get(T_WRAPPER, F_PRIM);
                s.else_();
                s.local_get(0);
                s.end();
                s.end();
                f
            }
            Helper::Construct(b) => construct(cx, b),
            Helper::Tag => tag(cx),
        }
    }
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

fn wh(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Wrap(h))
}

/// `$new_wrapper(prim, proto)`.
fn new(cx: &mut dyn Cx) -> Function {
    let define = oh(cx, object::Helper::Define);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let length = cx.str("length");
    const P: u32 = 0;
    const PROTO: u32 = 1;
    const O: u32 = 2;
    const N: u32 = 3;
    const I: u32 = 4;
    let mut f = Function::new([(1, rt(true, T_WRAPPER)), (2, ValType::I32)]);
    let mut s = f.instructions();
    s.i32_const(CLASS_WRAPPER).local_get(PROTO);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(P);
    s.struct_new(T_WRAPPER).local_set(O);
    // A string (not a symbol): its indices and `length`.
    runtime::is_string(&mut s, P);
    s.if_(BlockType::Empty);
    {
        s.local_get(P)
            .ref_cast_non_null(string())
            .array_len()
            .local_set(N);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(I).local_get(N).i32_ge_u().br_if(1);
        s.local_get(O).ref_as_non_null();
        s.local_get(I).f64_convert_i32_u().call(number_to_string);
        s.ref_cast_non_null(string());
        s.local_get(P)
            .ref_cast_non_null(string())
            .local_get(I)
            .array_get_u(T_STRING);
        s.array_new_fixed(T_STRING, 1);
        s.i32_const(READONLY | FIXED).call(define);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0).end().end();
        s.local_get(O).ref_as_non_null().global_get(length);
        s.local_get(N).ref_i31();
        s.i32_const(HIDDEN | READONLY | FIXED).call(define);
    }
    s.end();
    s.local_get(O).ref_as_non_null();
    s.end();
    f
}

/// `$to_object(v)`.
fn to_object(cx: &mut dyn Cx) -> Function {
    let new = wh(cx, Helper::New);
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let msg = cx.str("Cannot convert undefined or null to object");
    let protos = [
        oh(cx, object::Helper::Proto(Builtin::Symbol)),
        oh(cx, object::Helper::Proto(Builtin::String)),
        oh(cx, object::Helper::Proto(Builtin::Number)),
        oh(cx, object::Helper::Proto(Builtin::Boolean)),
    ];
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(object_ty());
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.local_get(V).call(is_nullish);
    s.if_(BlockType::Empty);
    s.global_get(msg).call(throw);
    s.end();
    let wrap = |s: &mut wasm_encoder::InstructionSink, proto: u32| {
        s.if_(BlockType::Empty);
        s.local_get(V).call(proto).call(new).return_();
        s.end();
    };
    s.local_get(V).ref_test_non_null(symbol());
    wrap(&mut s, protos[0]);
    if cx.bigint() {
        // T40.
        let proto = oh(cx, object::Helper::Proto(Builtin::BigInt));
        crate::bigint::is_bigint(&mut s, V);
        wrap(&mut s, proto);
    }
    s.test_string(V);
    wrap(&mut s, protos[1]);
    s.local_get(V).call(is_number);
    wrap(&mut s, protos[2]);
    s.local_get(V).global_get(G_TRUE).ref_eq();
    s.local_get(V).global_get(G_FALSE).ref_eq();
    s.i32_or();
    wrap(&mut s, protos[3]);
    s.local_get(V);
    s.end();
    f
}

/// The construct code of `String`/`Number`/`Boolean` (see [`Helper::Construct`]).
fn construct(cx: &mut dyn Cx, b: Builtin) -> Function {
    let (code, _) = cx.builtin_ctor(b).expect("a linked constructor");
    let new = wh(cx, Helper::New);
    let proto = oh(cx, object::Helper::Proto(b));
    let get = oh(cx, object::Helper::Get);
    let prototype = cx.str("prototype");
    const SELF: u32 = 0;
    const NT: u32 = 1;
    const ARGS: u32 = 2;
    const P: u32 = 3;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    if b == Builtin::String {
        // `new String(symbol)` throws (`String(symbol)` describes it).
        let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
        let msg = cx.str("Cannot convert a Symbol value to a string");
        s.local_get(ARGS).ref_cast_non_null(HeapType::Concrete(T_VALS)).array_len();
        s.if_(BlockType::Empty);
        s.local_get(ARGS)
            .ref_cast_non_null(HeapType::Concrete(T_VALS))
            .i32_const(0)
            .array_get(T_VALS)
            .ref_test_non_null(symbol());
        s.if_(BlockType::Empty);
        s.global_get(msg).call(throw);
        s.end();
        s.end();
    }
    s.local_get(SELF)
        .global_get(G_UNDEFINED)
        .local_get(ARGS)
        .call(code);
    // GetPrototypeFromConstructor: `new_target.prototype` if it's an object.
    s.local_get(NT).local_get(SELF).ref_eq();
    s.if_(BlockType::Result(rt(true, T_OBJECT)));
    s.call(proto);
    s.else_();
    s.local_get(NT).global_get(prototype).call(get).local_tee(P);
    s.ref_test_non_null(object_ty());
    s.if_(BlockType::Result(rt(true, T_OBJECT)));
    s.local_get(P).ref_cast_non_null(object_ty());
    s.else_();
    s.call(proto);
    s.end();
    s.end();
    s.call(new);
    s.end();
    f
}

/// `$wrapper_tag(v)` (see [`Helper::Tag`]).
fn tag(cx: &mut dyn Cx) -> Function {
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let names = [
        cx.str("Symbol"),
        cx.str("String"),
        cx.str("Number"),
        cx.str("Boolean"),
    ];
    const V: u32 = 0;
    const P: u32 = 1;
    let mut f = Function::new([(1, jsval())]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(wrapper()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(V)
        .ref_cast_non_null(wrapper())
        .struct_get(T_WRAPPER, F_PRIM)
        .local_set(P);
    s.global_get(names[0]);
    s.global_get(names[1]);
    s.global_get(names[2]);
    s.global_get(names[3]);
    s.local_get(P).call(is_number);
    s.typed_select(jsval());
    s.test_string(P);
    s.typed_select(jsval());
    s.local_get(P).ref_test_non_null(symbol());
    s.typed_select(jsval());
    if cx.bigint() {
        let name = cx.str("BigInt");
        s.global_get(name);
        crate::bigint::is_bigint(&mut s, P);
        s.i32_eqz().typed_select(jsval());
    }
    s.end();
    f
}
