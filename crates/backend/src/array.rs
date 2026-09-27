//! Arrays (T15, design D1).
//!
//! # Layout
//! `$array` is a final subtype of `$object` (named properties still live in the dictionary part):
//! `elems: mut (ref $vals)`, the dense store, and `length: mut i32`. Elements at or past `length`
//! are Wasm null, and so are holes. Index keys and `length` never go to the dictionary part: the
//! object helpers ask [`Helper::OwnGet`] / [`Helper::Define`] here first. Indices at or past
//! [`MAX_DENSE`] trap (there is no sparse storage yet).
//!
//! `eattrs: mut (ref null $bytes)` (T32) holds each element's attribute bits (`crate::object`'s
//! `HIDDEN`, `READONLY`, `FIXED`, `ACCESSOR`); null (the usual case) or past its end means the default
//! ones. An accessor element's slot holds its `$accessor`, as in the dictionary part, so a reader
//! of `elems` that may meet one resolves it with `props::Helper::Value`. The `class` flags
//! `SEALED`/`FROZEN` apply to every element on top of these, and `LENGTH_READONLY` to `length`.
//!
//! # Array.prototype
//! Built like the other prototypes (`crate::object`), with real `push`, `pop`, `join` and
//! `toString`; the other methods are sentinels (T21).

use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::object;
use crate::runtime::{
    self, Cx, G_UNDEFINED, T_ARRAY, T_BYTES, T_I32S, T_KEYS, T_STRING, T_VALS, jsval, rt,
    string_ref,
};

/// `$array` field indices after the object's.
pub const F_ELEMS: u32 = 7;
pub const F_LENGTH: u32 = 8;
pub const F_EATTRS: u32 = 9;

/// Indices from here on trap.
pub const MAX_DENSE: i32 = 1 << 26;

pub fn array() -> HeapType {
    HeapType::Concrete(T_ARRAY)
}

fn vals() -> HeapType {
    HeapType::Concrete(T_VALS)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(elems: (ref $vals), length: i32) -> (ref $array)`: a new array using `elems` as its store.
    New,
    /// `(a: (ref $array), k: (ref $string)) -> eqref`: own index or `length`; Wasm null otherwise.
    OwnGet,
    /// `(a: (ref $array), k: (ref $string), v: jsval) -> i32`: set an index or `length`; 0 if `k`
    /// is neither (the caller uses the dictionary part).
    Define,
    /// `(a: (ref $array), i: i32, v: jsval)`: `a[i] = v` for an index `i` ≥ 0 (grows the array).
    SetIndex,
    /// `(a: (ref $array), v: jsval)`: `a.length = v` (RangeError for an invalid length).
    SetLength,
    /// `(a: (ref $array), v: jsval)`: append (literals, spread, rest).
    Push,
    /// `(a: (ref $array), sep: (ref $string)) -> (ref $string)`: `a.join(sep)`.
    Join,
    /// `(a: (ref $array), all: i32) -> (ref $keys)`: the keys of the elements that aren't holes,
    /// ascending (without `all`, only the enumerable ones).
    IndexKeys,
    /// The code (a `$code`) of `Array.prototype.push`.
    ProtoPush,
    /// `Array.prototype.pop`.
    ProtoPop,
    /// `Array.prototype.join`.
    ProtoJoin,
    /// `Array.prototype.toString` (`join()`).
    ProtoToString,
    /// `(args: jsval (a `$vals`), k: i32) -> jsval`: a new array of `args[k..]` (rest parameters).
    FromArgs,
    /// `(a: (ref $array), i: i32) -> i32`: element `i`'s attribute bits, `SEALED`/`FROZEN` included.
    Attrs,
    /// `(a: (ref $array), i: i32, attrs: i32)`: set element `i`'s attribute bits.
    SetAttrs,
    /// `(a: (ref $array), len: i32)`: set a valid length (shrinking stops at an element that isn't
    /// configurable, with a TypeError).
    Resize,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_array",
            Helper::OwnGet => "array_own_get",
            Helper::Define => "array_define",
            Helper::SetIndex => "array_set_index",
            Helper::SetLength => "array_set_length",
            Helper::Push => "array_push",
            Helper::Join => "array_join",
            Helper::IndexKeys => "array_index_keys",
            Helper::ProtoPush => "Array.prototype.push",
            Helper::ProtoPop => "Array.prototype.pop",
            Helper::ProtoJoin => "Array.prototype.join",
            Helper::ProtoToString => "Array.prototype.toString",
            Helper::FromArgs => "array_from_args",
            Helper::Attrs => "array_attrs",
            Helper::SetAttrs => "array_set_attrs",
            Helper::Resize => "array_resize",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let a = rt(false, T_ARRAY);
        let s = string_ref();
        match self {
            Helper::New => (vec![rt(false, T_VALS), ValType::I32], vec![a]),
            Helper::OwnGet => (vec![a, s], vec![v]),
            Helper::Define => (vec![a, s, v], vec![ValType::I32]),
            Helper::SetIndex => (vec![a, ValType::I32, v], vec![]),
            Helper::SetLength | Helper::Push => (vec![a, v], vec![]),
            Helper::Join => (vec![a, s], vec![s]),
            Helper::IndexKeys => (vec![a, ValType::I32], vec![rt(false, T_KEYS)]),
            Helper::ProtoPush | Helper::ProtoPop | Helper::ProtoJoin | Helper::ProtoToString => {
                (vec![v, v, v], vec![v])
            }
            Helper::FromArgs => (vec![v, ValType::I32], vec![v]),
            Helper::Attrs => (vec![a, ValType::I32], vec![ValType::I32]),
            Helper::SetAttrs => (vec![a, ValType::I32, ValType::I32], vec![]),
            Helper::Resize => (vec![a, ValType::I32], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::OwnGet => own_get(cx),
            Helper::Define => define(cx),
            Helper::SetIndex => set_index(cx),
            Helper::SetLength => set_length(cx),
            Helper::Push => push(cx),
            Helper::Join => join(cx),
            Helper::IndexKeys => index_keys(cx),
            Helper::ProtoPush => proto_push(cx),
            Helper::ProtoPop => proto_pop(cx),
            Helper::ProtoJoin => proto_join(cx),
            Helper::ProtoToString => proto_to_string(cx),
            Helper::FromArgs => from_args(cx),
            Helper::Attrs => attrs(),
            Helper::SetAttrs => set_attrs(),
            Helper::Resize => resize(cx),
        }
    }
}

pub fn ah(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Arr(h))
}

fn oh(cx: &mut dyn Cx, h: object::Helper) -> u32 {
    cx.helper(runtime::Helper::Obj(h))
}

/// The real methods of `Array.prototype`: (name, code helper, `length`).
pub fn proto_methods() -> [(&'static str, Helper, i32); 4] {
    [
        ("push", Helper::ProtoPush, 1),
        ("pop", Helper::ProtoPop, 0),
        ("join", Helper::ProtoJoin, 1),
        ("toString", Helper::ProtoToString, 0),
    ]
}

fn new(cx: &mut dyn Cx) -> Function {
    let proto = oh(cx, object::Helper::Proto(Builtin::Array));
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(0).local_get(1);
    s.ref_null(HeapType::Concrete(T_BYTES));
    s.struct_new(T_ARRAY);
    s.end();
    f
}

fn attrs() -> Function {
    const A: u32 = 0;
    const I: u32 = 1;
    const B: u32 = 2;
    const R: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_BYTES)), (1, ValType::I32)]);
    let mut s = f.instructions();
    // The class flags: sealed → FIXED, frozen → READONLY | FIXED.
    s.i32_const(object::FIXED).i32_const(0);
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::SEALED | object::FROZEN)
        .i32_and();
    s.select();
    s.i32_const(object::READONLY).i32_const(0);
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::FROZEN)
        .i32_and();
    s.select();
    s.i32_or().local_set(R);
    s.local_get(A).struct_get(T_ARRAY, F_EATTRS).local_tee(B);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(I)
        .local_get(B)
        .ref_as_non_null()
        .array_len()
        .i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(B)
        .ref_as_non_null()
        .local_get(I)
        .array_get_u(T_BYTES)
        .local_get(R)
        .i32_or()
        .local_set(R);
    s.end();
    s.end();
    s.local_get(R);
    s.end();
    f
}

/// Store element attributes, making or growing `eattrs` (to the store's capacity) when needed.
fn set_attrs() -> Function {
    const A: u32 = 0;
    const I: u32 = 1;
    const V: u32 = 2;
    const B: u32 = 3;
    const NEW: u32 = 4;
    let mut f = Function::new([(2, rt(true, T_BYTES))]);
    let mut s = f.instructions();
    s.local_get(A).struct_get(T_ARRAY, F_EATTRS).local_set(B);
    s.local_get(B).ref_is_null();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(1);
    s.else_();
    s.local_get(I)
        .local_get(B)
        .ref_as_non_null()
        .array_len()
        .i32_ge_u();
    s.end();
    s.if_(BlockType::Empty);
    // Default attributes need no store.
    s.local_get(V).i32_eqz();
    s.if_(BlockType::Empty);
    s.return_();
    s.end();
    s.local_get(A).struct_get(T_ARRAY, F_ELEMS).array_len();
    s.local_get(I).i32_const(1).i32_add();
    s.local_get(A).struct_get(T_ARRAY, F_ELEMS).array_len();
    s.local_get(I).i32_const(1).i32_add();
    s.i32_gt_u();
    s.select();
    s.array_new_default(T_BYTES).local_set(NEW);
    s.local_get(B).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(NEW).ref_as_non_null().i32_const(0);
    s.local_get(B).ref_as_non_null().i32_const(0);
    s.local_get(B).ref_as_non_null().array_len();
    s.array_copy(T_BYTES, T_BYTES);
    s.end();
    s.local_get(A).local_get(NEW).struct_set(T_ARRAY, F_EATTRS);
    s.local_get(NEW).local_set(B);
    s.end();
    s.local_get(B)
        .ref_as_non_null()
        .local_get(I)
        .local_get(V)
        .i32_const(object::HIDDEN | object::READONLY | object::FIXED | object::ACCESSOR)
        .i32_and()
        .array_set(T_BYTES);
    s.end();
    f
}

/// Push `k`'s array index as an i64 (-1: not an index).
fn index_of(cx: &mut dyn Cx, s: &mut InstructionSink, k: u32) {
    let index = oh(cx, object::Helper::ArrayIndex);
    s.local_get(k).call(index);
}

fn own_get(cx: &mut dyn Cx) -> Function {
    let str_eq = cx.helper(runtime::Helper::StrEq);
    let length = cx.str("length");
    const A: u32 = 0;
    const K: u32 = 1;
    const I: u32 = 2;
    let mut f = Function::new([(1, ValType::I64)]);
    let mut s = f.instructions();
    index_of(cx, &mut s, K);
    s.local_tee(I);
    s.local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i64_extend_i32_u();
    s.i64_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(A)
        .struct_get(T_ARRAY, F_ELEMS)
        .local_get(I)
        .i32_wrap_i64()
        .array_get(T_VALS)
        .return_();
    s.end();
    s.local_get(K).global_get(length).call(str_eq);
    s.if_(BlockType::Empty);
    // Below MAX_DENSE: always an i31.
    s.local_get(A).struct_get(T_ARRAY, F_LENGTH).ref_i31();
    s.return_();
    s.end();
    s.ref_null(HeapType::Abstract {
        shared: false,
        ty: wasm_encoder::AbstractHeapType::Eq,
    });
    s.end();
    f
}

fn define(cx: &mut dyn Cx) -> Function {
    let str_eq = cx.helper(runtime::Helper::StrEq);
    let set_index = ah(cx, Helper::SetIndex);
    let set_length = ah(cx, Helper::SetLength);
    let length = cx.str("length");
    const A: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const I: u32 = 3;
    let mut f = Function::new([(1, ValType::I64)]);
    let mut s = f.instructions();
    index_of(cx, &mut s, K);
    s.local_tee(I).i64_const(0).i64_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(I).i64_const(MAX_DENSE as i64).i64_ge_u();
    s.if_(BlockType::Empty);
    s.unreachable(); // sparse arrays aren't implemented
    s.end();
    s.local_get(A)
        .local_get(I)
        .i32_wrap_i64()
        .local_get(V)
        .call(set_index);
    s.i32_const(1).return_();
    s.end();
    s.local_get(K).global_get(length).call(str_eq);
    s.if_(BlockType::Empty);
    s.local_get(A).local_get(V).call(set_length);
    s.i32_const(1).return_();
    s.end();
    s.i32_const(0);
    s.end();
    f
}

/// Make room for index `i` (capacity at least `i + 1`, doubling).
fn ensure(s: &mut InstructionSink, a: u32, i: u32, tmp_cap: u32, tmp_new: u32) {
    s.local_get(i);
    s.local_get(a).struct_get(T_ARRAY, F_ELEMS).array_len();
    s.i32_ge_u();
    s.if_(BlockType::Empty);
    // cap = max(i + 1, 2 * old, 4)
    s.local_get(a)
        .struct_get(T_ARRAY, F_ELEMS)
        .array_len()
        .i32_const(1)
        .i32_shl()
        .local_tee(tmp_cap);
    s.local_get(i).i32_const(1).i32_add();
    s.local_get(tmp_cap)
        .local_get(i)
        .i32_const(1)
        .i32_add()
        .i32_gt_u();
    s.select().local_tee(tmp_cap);
    s.i32_const(4)
        .local_get(tmp_cap)
        .i32_const(4)
        .i32_gt_u()
        .select();
    s.array_new_default(T_VALS).local_set(tmp_new);
    s.local_get(tmp_new).ref_as_non_null().i32_const(0);
    s.local_get(a).struct_get(T_ARRAY, F_ELEMS).i32_const(0);
    s.local_get(a).struct_get(T_ARRAY, F_LENGTH);
    s.array_copy(T_VALS, T_VALS);
    s.local_get(a)
        .local_get(tmp_new)
        .ref_as_non_null()
        .struct_set(T_ARRAY, F_ELEMS);
    s.end();
}

fn set_index(cx: &mut dyn Cx) -> Function {
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (read_only, of_array) = (
        cx.str("Cannot assign to read only property '"),
        cx.str("' of object '[object Array]'"),
    );
    let (add, not_ext) = (
        cx.str("Cannot add property "),
        cx.str(", object is not extensible"),
    );
    let attrs = ah(cx, Helper::Attrs);
    let call_value = cx.helper(runtime::Helper::Func(crate::func::Helper::CallValue));
    let setter = cx.str("setter");
    let (set_prop, only_getter) = (
        cx.str("Cannot set property "),
        cx.str(" of [object Array] which has only a getter"),
    );
    let length_read_only =
        cx.str("Cannot assign to read only property 'length' of object '[object Array]'");
    const A: u32 = 0;
    const I: u32 = 1;
    const V: u32 = 2;
    const CAP: u32 = 3;
    const NEW: u32 = 4;
    const T: u32 = 5;
    const X: u32 = 6;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, rt(true, T_VALS)),
        (1, ValType::I32),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.local_get(I).i32_const(MAX_DENSE).i32_ge_u();
    s.if_(BlockType::Empty);
    s.unreachable();
    s.end();
    // Element attributes (T32): an accessor's setter, a read-only element; past the end, a
    // read-only `length`.
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_lt_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(A)
            .struct_get(T_ARRAY, F_EATTRS)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(A).local_get(I).call(attrs).local_tee(T);
        s.i32_const(object::ACCESSOR).i32_and();
        s.if_(BlockType::Empty);
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(I)
            .array_get(T_VALS)
            .ref_cast_non_null(HeapType::Concrete(runtime::T_ACCESSOR))
            .struct_get(runtime::T_ACCESSOR, 1)
            .local_tee(X);
        s.global_get(G_UNDEFINED).ref_eq();
        s.if_(BlockType::Empty);
        s.global_get(set_prop);
        s.local_get(I).f64_convert_i32_u().call(number_to_string);
        s.call(concat);
        s.global_get(only_getter).call(concat);
        s.call(throw);
        s.end();
        s.local_get(X).local_get(A);
        s.local_get(V).array_new_fixed(T_VALS, 1);
        s.global_get(setter).call(call_value).drop();
        s.return_();
        s.end();
        s.local_get(T).i32_const(object::READONLY).i32_and();
        s.if_(BlockType::Empty);
        s.global_get(read_only);
        s.local_get(I).f64_convert_i32_u().call(number_to_string);
        s.call(concat);
        s.global_get(of_array).call(concat);
        s.call(throw);
        s.end();
        s.end();
    }
    s.else_();
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::LENGTH_READONLY)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(length_read_only).call(throw);
    s.end();
    s.end();
    // Frozen: elements are read-only; not extensible: no new elements (T16).
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::NO_EXTEND | object::FROZEN)
        .i32_and();
    s.if_(BlockType::Empty);
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_lt_u();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(A)
        .struct_get(T_ARRAY, F_ELEMS)
        .local_get(I)
        .array_get(T_VALS)
        .ref_is_null()
        .i32_eqz();
    s.else_();
    s.i32_const(0);
    s.end();
    s.if_(BlockType::Empty);
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::FROZEN)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(read_only);
    s.local_get(I).f64_convert_i32_u().call(number_to_string);
    s.call(concat);
    s.global_get(of_array).call(concat);
    s.call(throw);
    s.end();
    s.else_();
    s.global_get(add);
    s.local_get(I).f64_convert_i32_u().call(number_to_string);
    s.call(concat);
    s.global_get(not_ext).call(concat);
    s.call(throw);
    s.end();
    s.end();
    ensure(&mut s, A, I, CAP, NEW);
    s.local_get(A)
        .struct_get(T_ARRAY, F_ELEMS)
        .local_get(I)
        .local_get(V)
        .array_set(T_VALS);
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_ge_u();
    s.if_(BlockType::Empty);
    s.local_get(A)
        .local_get(I)
        .i32_const(1)
        .i32_add()
        .struct_set(T_ARRAY, F_LENGTH);
    s.end();
    s.end();
    f
}

fn set_length(cx: &mut dyn Cx) -> Function {
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let to_int32 = cx.to_int32();
    let resize = ah(cx, Helper::Resize);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::RangeError));
    let invalid = cx.str("Invalid array length");
    const A: u32 = 0;
    const V: u32 = 1;
    const N: u32 = 2;
    const N2: u32 = 3;
    let type_error = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let read_only =
        cx.str("Cannot assign to read only property 'length' of object '[object Array]'");
    let mut f = Function::new([(2, ValType::F64)]);
    let mut s = f.instructions();
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::FROZEN | object::LENGTH_READONLY)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(read_only).call(type_error);
    s.end();
    // ArraySetLength: ToUint32(v) must equal ToNumber(v) (two conversions, as the spec does).
    s.local_get(V).call(to_number).local_set(N);
    s.local_get(V).call(to_number).local_set(N2);
    s.local_get(N).call(to_int32).f64_convert_i32_u();
    s.local_get(N2).f64_ne();
    s.if_(BlockType::Empty);
    s.global_get(invalid).call(throw);
    s.end();
    s.local_get(A).local_get(N2).i32_trunc_f64_u().call(resize);
    s.end();
    f
}

/// `(a, len: i32)`: set the length (a valid one): shrinking deletes from the end and stops,
/// with node's TypeError, at an element that isn't configurable; growing grows the store.
fn resize(cx: &mut dyn Cx) -> Function {
    let this = ah(cx, Helper::Resize);
    let attrs = ah(cx, Helper::Attrs);
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let type_error = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (cannot, of_array) = (
        cx.str("Cannot delete property '"),
        cx.str("' of [object Array]"),
    );
    const A: u32 = 0;
    const L: u32 = 1;
    const NEW: u32 = 2;
    const K: u32 = 3;
    const B: u32 = 4;
    let mut f = Function::new([
        (1, rt(true, T_VALS)),
        (1, ValType::I32),
        (1, rt(true, T_BYTES)),
    ]);
    let mut s = f.instructions();
    s.local_get(L).i32_const(MAX_DENSE).i32_gt_u();
    s.if_(BlockType::Empty);
    s.unreachable(); // sparse arrays aren't implemented
    s.end();
    s.local_get(L)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_lt_u();
    s.if_(BlockType::Empty);
    {
        // Elements that aren't configurable stop the deletion (T32).
        s.local_get(A).struct_get(T_ARRAY, F_EATTRS).ref_is_null().i32_eqz();
        s.local_get(A)
            .struct_get(T_ARRAY, object::F_CLASS)
            .i32_const(object::SEALED | object::FROZEN)
            .i32_and();
        s.i32_or();
        s.if_(BlockType::Empty);
        s.local_get(A).struct_get(T_ARRAY, F_LENGTH).local_set(K);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(K).local_get(L).i32_le_u().br_if(1);
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(K)
            .i32_const(1)
            .i32_sub()
            .array_get(T_VALS)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(A)
            .local_get(K)
            .i32_const(1)
            .i32_sub()
            .call(attrs)
            .i32_const(object::FIXED)
            .i32_and();
        s.if_(BlockType::Empty);
        // Keep the elements up to this one, then throw.
        s.local_get(A).local_get(K).call(this);
        s.global_get(cannot);
        s.local_get(K)
            .i32_const(1)
            .i32_sub()
            .f64_convert_i32_u()
            .call(number_to_string);
        s.call(concat);
        s.global_get(of_array).call(concat);
        s.call(type_error);
        s.end();
        s.end();
        s.local_get(K).i32_const(1).i32_sub().local_set(K);
        s.br(0).end().end();
        s.end();
        s.local_get(A).struct_get(T_ARRAY, F_ELEMS);
        s.local_get(L);
        s.ref_null(HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Eq,
        });
        s.local_get(A)
            .struct_get(T_ARRAY, F_LENGTH)
            .local_get(L)
            .i32_sub();
        s.array_fill(T_VALS);
        // The dropped elements' attributes.
        s.local_get(A).struct_get(T_ARRAY, F_EATTRS).local_tee(B);
        s.ref_is_null().i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(L).local_get(B).ref_as_non_null().array_len().i32_lt_u();
        s.if_(BlockType::Empty);
        s.local_get(B).ref_as_non_null().local_get(L).i32_const(0);
        s.local_get(B).ref_as_non_null().array_len().local_get(L).i32_sub();
        s.array_fill(T_BYTES);
        s.end();
        s.end();
    }
    s.end();
    s.local_get(L)
        .local_get(A)
        .struct_get(T_ARRAY, F_ELEMS)
        .array_len()
        .i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(L).array_new_default(T_VALS).local_set(NEW);
    s.local_get(NEW).ref_as_non_null().i32_const(0);
    s.local_get(A).struct_get(T_ARRAY, F_ELEMS).i32_const(0);
    s.local_get(A).struct_get(T_ARRAY, F_LENGTH);
    s.array_copy(T_VALS, T_VALS);
    s.local_get(A)
        .local_get(NEW)
        .ref_as_non_null()
        .struct_set(T_ARRAY, F_ELEMS);
    s.end();
    s.local_get(A).local_get(L).struct_set(T_ARRAY, F_LENGTH);
    s.end();
    f
}

fn push(cx: &mut dyn Cx) -> Function {
    let set_index = ah(cx, Helper::SetIndex);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0);
    s.local_get(0).struct_get(T_ARRAY, F_LENGTH);
    s.local_get(1).call(set_index);
    s.end();
    f
}

/// `join`: `undefined`/`null`/holes are empty, others ToString.
fn join(cx: &mut dyn Cx) -> Function {
    let to_string = cx.helper(runtime::Helper::ToString);
    let value = cx.helper(runtime::Helper::Prop(crate::props::Helper::Value));
    let get = oh(cx, object::Helper::Get);
    let is_nullish = cx.helper(runtime::Helper::IsNullish);
    let empty = cx.str("");
    const A: u32 = 0;
    const SEP: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const TOTAL: u32 = 4;
    const POS: u32 = 5;
    const PARTS: u32 = 6;
    const E: u32 = 7;
    const P: u32 = 8;
    const R: u32 = 9;
    let mut f = Function::new([
        (4, ValType::I32),
        (1, rt(true, T_KEYS)),
        (1, jsval()),
        (2, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    // Pass 1: each element's string, in order (ToString may run user code, which may shrink the
    // array: an index past the current length reads as a hole), and the total length.
    s.local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .local_tee(N)
        .array_new_default(T_KEYS)
        .local_set(PARTS);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.global_get(empty).local_set(P);
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(A)
        .struct_get(T_ARRAY, F_ELEMS)
        .local_get(I)
        .array_get(T_VALS)
        .local_tee(E);
    s.ref_is_null();
    s.if_(BlockType::Result(jsval()));
    // A hole reads through the prototype chain (T33).
    s.local_get(A).local_get(I).ref_i31().call(get);
    s.else_();
    // An accessor element (T32): its getter's value.
    s.local_get(E).local_get(A).call(value);
    s.end();
    s.local_set(E);
    s.block(BlockType::Empty);
    s.local_get(E).call(is_nullish).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(E).call(to_string).local_set(P);
    s.end();
    s.end();
    s.end();
    s.local_get(PARTS)
        .local_get(I)
        .local_get(P)
        .array_set(T_KEYS);
    s.local_get(TOTAL)
        .local_get(P)
        .array_len()
        .i32_add()
        .local_set(TOTAL);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    // Pass 2: one string, the parts and separators copied in.
    s.local_get(N).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(empty).return_();
    s.end();
    s.local_get(N).i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(PARTS)
        .i32_const(0)
        .array_get(T_KEYS)
        .ref_as_non_null()
        .return_();
    s.end();
    s.local_get(TOTAL)
        .local_get(SEP)
        .array_len()
        .local_get(N)
        .i32_const(1)
        .i32_sub()
        .i32_mul()
        .i32_add()
        .array_new_default(T_STRING)
        .local_set(R);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(I).i32_const(0).i32_gt_u();
    s.if_(BlockType::Empty);
    // array.copy dst dst_off src src_off len
    s.local_get(R).local_get(POS).local_get(SEP).i32_const(0);
    s.local_get(SEP).array_len();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(POS)
        .local_get(SEP)
        .array_len()
        .i32_add()
        .local_set(POS);
    s.end();
    s.local_get(PARTS)
        .local_get(I)
        .array_get(T_KEYS)
        .local_set(P);
    s.local_get(R).local_get(POS).local_get(P).i32_const(0);
    s.local_get(P).array_len();
    s.array_copy(T_STRING, T_STRING);
    s.local_get(POS)
        .local_get(P)
        .array_len()
        .i32_add()
        .local_set(POS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

fn index_keys(cx: &mut dyn Cx) -> Function {
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let attrs = ah(cx, Helper::Attrs);
    const A: u32 = 0;
    const ALL: u32 = 1;
    const I: u32 = 2;
    const N: u32 = 3;
    const OUT: u32 = 4;
    const HID: u32 = 5;
    let mut f = Function::new([(2, ValType::I32), (1, rt(true, T_KEYS)), (1, ValType::I32)]);
    let mut s = f.instructions();
    // Non-enumerable elements (T32) are possible only with element attributes.
    s.local_get(ALL).i32_eqz();
    s.local_get(A).struct_get(T_ARRAY, F_EATTRS).ref_is_null().i32_eqz();
    s.i32_and().local_set(HID);
    let elem = |s: &mut InstructionSink| {
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(I)
            .array_get(T_VALS)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(HID);
        s.if_(BlockType::Result(ValType::I32));
        s.local_get(A)
            .local_get(I)
            .call(attrs)
            .i32_const(object::HIDDEN)
            .i32_and()
            .i32_eqz();
        s.else_();
        s.i32_const(1);
        s.end();
        s.else_();
        s.i32_const(0);
        s.end();
    };
    // Count the elements, then fill.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_ge_u()
        .br_if(1);
    elem(&mut s);
    s.local_get(N).i32_add().local_set(N);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(N).array_new_default(T_KEYS).local_set(OUT);
    s.i32_const(0).local_set(I);
    s.i32_const(0).local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_ge_u()
        .br_if(1);
    elem(&mut s);
    s.if_(BlockType::Empty);
    s.local_get(OUT).local_get(N);
    s.local_get(I).f64_convert_i32_u().call(number_to_string);
    s.array_set(T_KEYS);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}

/// `this` as an array, or a TypeError: the methods are only implemented for arrays so far.
fn this_array(cx: &mut dyn Cx, s: &mut InstructionSink, name: &str) {
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let msg = cx.str(&format!(
        "Array.prototype.{name} is only implemented for arrays (js2wasi3)"
    ));
    s.local_get(1).ref_test_non_null(array()).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(msg).call(throw);
    s.end();
}

fn args_len(s: &mut InstructionSink) {
    s.local_get(2).ref_cast_non_null(vals()).array_len();
}

fn proto_push(cx: &mut dyn Cx) -> Function {
    let push = ah(cx, Helper::Push);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let length_read_only =
        cx.str("Cannot assign to read only property 'length' of object '[object Array]'");
    const I: u32 = 3;
    const N: u32 = 4;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    this_array(cx, &mut s, "push");
    args_len(&mut s);
    s.local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(1).ref_cast_non_null(array());
    s.local_get(2)
        .ref_cast_non_null(vals())
        .local_get(I)
        .array_get(T_VALS);
    s.call(push);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    // Then `length` is set: a read-only one throws even with nothing pushed (T32).
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::LENGTH_READONLY | object::FROZEN)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(length_read_only).call(throw);
    s.end();
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, F_LENGTH)
        .ref_i31();
    s.end();
    f
}

fn proto_pop(cx: &mut dyn Cx) -> Function {
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let (cannot, of_array) = (
        cx.str("Cannot delete property '"),
        cx.str("' of [object Array]"),
    );
    let value = cx.helper(runtime::Helper::Prop(crate::props::Helper::Value));
    let get = oh(cx, object::Helper::Get);
    let attrs = ah(cx, Helper::Attrs);
    let set_attrs = ah(cx, Helper::SetAttrs);
    let length_read_only =
        cx.str("Cannot assign to read only property 'length' of object '[object Array]'");
    const L: u32 = 3;
    const V: u32 = 4;
    let mut f = Function::new([(1, ValType::I32), (1, jsval())]);
    let mut s = f.instructions();
    this_array(cx, &mut s, "pop");
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, F_LENGTH)
        .local_tee(L)
        .i32_eqz();
    s.if_(BlockType::Empty);
    // `length` is still set (to 0): a read-only one throws (T32).
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::LENGTH_READONLY | object::FROZEN)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(length_read_only).call(throw);
    s.end();
    s.global_get(G_UNDEFINED).return_();
    s.end();
    s.local_get(L).i32_const(1).i32_sub().local_set(L);
    // Get the last element (an accessor's getter, T32), then delete it: not when it isn't
    // configurable (sealed, frozen or by its attributes); then the new length.
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, F_ELEMS)
        .local_get(L)
        .array_get(T_VALS)
        .local_tee(V);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    // A hole reads through the prototype chain (T33).
    s.local_get(1).local_get(L).ref_i31().call(get).local_set(V);
    s.else_();
    s.local_get(V).local_get(1).call(value).local_set(V);
    s.local_get(1).ref_cast_non_null(array()).local_get(L).call(attrs);
    s.i32_const(object::FIXED).i32_and();
    s.if_(BlockType::Empty);
    s.global_get(cannot);
    s.local_get(L).f64_convert_i32_u().call(number_to_string);
    s.call(concat);
    s.global_get(of_array).call(concat);
    s.call(throw);
    s.end();
    s.end();
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::LENGTH_READONLY | object::FROZEN)
        .i32_and();
    s.if_(BlockType::Empty);
    s.global_get(length_read_only).call(throw);
    s.end();
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, F_ELEMS)
        .local_get(L);
    s.ref_null(HeapType::Abstract {
        shared: false,
        ty: wasm_encoder::AbstractHeapType::Eq,
    });
    s.array_set(T_VALS);
    s.local_get(1).ref_cast_non_null(array()).local_get(L).i32_const(0).call(set_attrs);
    s.local_get(1)
        .ref_cast_non_null(array())
        .local_get(L)
        .struct_set(T_ARRAY, F_LENGTH);
    // A hole reads as `undefined`.
    s.local_get(V).ref_is_null();
    s.if_(BlockType::Result(jsval()));
    s.global_get(G_UNDEFINED);
    s.else_();
    s.local_get(V);
    s.end();
    s.end();
    f
}

fn proto_join(cx: &mut dyn Cx) -> Function {
    let join = ah(cx, Helper::Join);
    let to_string = cx.helper(runtime::Helper::ToString);
    let comma = cx.str(",");
    let mut f = Function::new([]);
    let mut s = f.instructions();
    this_array(cx, &mut s, "join");
    s.local_get(1).ref_cast_non_null(array());
    // The separator: "," unless an argument other than `undefined` is given.
    args_len(&mut s);
    s.if_(BlockType::Result(string_ref()));
    s.local_get(2)
        .ref_cast_non_null(vals())
        .i32_const(0)
        .array_get(T_VALS)
        .global_get(G_UNDEFINED)
        .ref_eq();
    s.if_(BlockType::Result(string_ref()));
    s.global_get(comma);
    s.else_();
    s.local_get(2)
        .ref_cast_non_null(vals())
        .i32_const(0)
        .array_get(T_VALS)
        .call(to_string);
    s.end();
    s.else_();
    s.global_get(comma);
    s.end();
    s.call(join);
    s.end();
    f
}

fn proto_to_string(cx: &mut dyn Cx) -> Function {
    let join = ah(cx, Helper::Join);
    let comma = cx.str(",");
    let mut f = Function::new([]);
    let mut s = f.instructions();
    this_array(cx, &mut s, "toString");
    s.local_get(1).ref_cast_non_null(array());
    s.global_get(comma).call(join);
    s.end();
    f
}

fn from_args(cx: &mut dyn Cx) -> Function {
    let new = ah(cx, Helper::New);
    const ARGS: u32 = 0;
    const K: u32 = 1;
    const N: u32 = 2;
    const E: u32 = 3;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    // n = max(len - k, 0)
    s.i32_const(0);
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .array_len()
        .local_get(K)
        .i32_sub();
    s.local_get(ARGS)
        .ref_cast_non_null(vals())
        .array_len()
        .local_get(K)
        .i32_le_u();
    s.select().local_tee(N);
    s.array_new_default(T_VALS).local_set(E);
    s.local_get(N);
    s.if_(BlockType::Empty);
    s.local_get(E).ref_as_non_null().i32_const(0);
    s.local_get(ARGS).ref_cast_non_null(vals()).local_get(K);
    s.local_get(N);
    s.array_copy(T_VALS, T_VALS);
    s.end();
    s.local_get(E).ref_as_non_null().local_get(N).call(new);
    s.end();
    f
}
