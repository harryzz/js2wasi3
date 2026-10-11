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
//! # Number mode (T107)
//! `nums: mut (ref null $nums)`: null, the array is generic, and then `hint: mut i32` set means every element
//! so far is a number or a hole (empty arrays and all-integer literals start so; an i32 rather than a shared
//! marker object, which the copying collector would forward for every array); a `$nums`, **number mode**: the elements are `nums[0..length]` as `f64`s (a hole is [`HOLE`]), `elems` is empty (so a
//! reader that tries `elems` below its size first, `object::get`, falls through). A hinted
//! array storing a number that isn't an i31 converts to number mode ([`Helper::ToNums`], which checks every
//! element: the hint is only a hint). Every reader of `elems` that doesn't know these states calls
//! [`Helper::Elems`], which leaves number mode (boxing) or drops the hint, for good; the hot paths know them.
//!
//! # Array.prototype
//! Built like the other prototypes (`crate::object`), with real `push`, `pop`, `join` and
//! `toString`; the other methods are sentinels (T21).

use js2wasi3_ir::{Builtin, ErrorKind};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::object;
use crate::runtime::{
    self, Cx, G_UNDEFINED, T_ARRAY, T_BYTES, T_I32S, T_KEYS, T_NUMS, T_STRING, T_VALS, jsval, rt,
    string_ref,
};

/// `$array` field indices after the object's.
pub const F_ELEMS: u32 = 7;
pub const F_LENGTH: u32 = 8;
pub const F_EATTRS: u32 = 9;
/// T107: `nums: mut (ref null $nums)`, the elements in number mode (see the module doc).
pub const F_NUMS: u32 = 10;
/// T107: `hint: mut i32` (see the module doc).
pub const F_HINT: u32 = 11;
/// The bits of the NaN that marks a hole in number mode (a stored NaN is canonicalized, so no value has them).
pub const HOLE: i64 = 0x7ff4_0000_0000_0001;

/// Indices from here on trap.
pub const MAX_DENSE: i32 = 1 << 26;

/// The throw of the RangeError that stands for sparse arrays (an index or length at or past
/// [`MAX_DENSE`]; not implemented, F50/F160) instead of a trap (T87): the helper and the message.
fn sparse_error(cx: &mut dyn Cx) -> (u32, u32) {
    let throw = oh(cx, object::Helper::Throw(ErrorKind::RangeError));
    let msg = cx.str(
        "Arrays with an index or length of 2^26 or more (sparse arrays) aren't supported by js2wasi3 yet",
    );
    (throw, msg)
}

fn sparse(s: &mut InstructionSink, (throw, msg): (u32, u32)) {
    s.global_get(msg).call(throw);
    s.unreachable();
}

pub fn array() -> HeapType {
    HeapType::Concrete(T_ARRAY)
}

fn vals() -> HeapType {
    HeapType::Concrete(T_VALS)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(elems: (ref $vals), length: i32) -> (ref $array)`: a new array using `elems` as its store (T107: its
    /// number-mode state from its elements, checked).
    New,
    /// [`Helper::New`] for an array literal known at build time to hold only i31 constants (or nothing): hinted,
    /// unchecked (T107).
    NewHinted,
    /// [`Helper::New`] for an array literal with an element that isn't known to be a number: generic,
    /// unchecked (T107).
    NewGeneric,
    /// `(a: (ref $array), k: (ref $string)) -> eqref`: own index or `length`; Wasm null otherwise.
    OwnGet,
    /// `(a: (ref $array), k: (ref $string), v: jsval) -> i32`: an index or `length`; 0 if `k` is
    /// neither (the caller uses the dictionary part). `set`: [[Set]] ([`Helper::SetIndex`]), else
    /// CreateDataProperty ([`Helper::DefineIndex`]).
    Define { set: bool },
    /// `(a: (ref $array), i: i32, v: jsval)`: `a[i] = v` ([[Set]]) for an index `i` ≥ 0 (grows the
    /// array): accessor and read-only elements, and for an absent one the prototype chain (T34).
    SetIndex,
    /// `(a: (ref $array), i: i32, v: jsval)`: CreateDataProperty of index `i` (T34): a writable,
    /// enumerable, configurable data element, replacing a configurable one; never the prototype
    /// chain.
    DefineIndex,
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
    /// `(a: jsval (an array), k: i32) -> jsval`: a new array of `a`'s elements from `k` on (T63).
    Rest,
    /// `(a: (ref $array), i: i32) -> i32`: element `i`'s attribute bits, `SEALED`/`FROZEN` included.
    Attrs,
    /// `(a: (ref $array), i: i32, attrs: i32)`: set element `i`'s attribute bits.
    SetAttrs,
    /// `(a: (ref $array), len: i32)`: set a valid length (shrinking stops at an element that isn't
    /// configurable, with a TypeError).
    Resize,
    /// `(a: (ref null $array)) -> (ref $vals)`: `a`'s `elems`, after leaving number mode (T107): what
    /// every reader of `elems` that doesn't know number mode calls.
    Elems,
    /// `(a: (ref $array), len: i32)` (T107): number mode only: `length = len`, growing `nums` (doubling) and
    /// filling new slots with holes.
    NumsResize,
    /// `(a: (ref $array), i: i32) -> eqref` (T107): element `i < length` as stored (an accessor's slot is
    /// its `$accessor`), boxed in number mode; Wasm null for a hole.
    ElemAt,
    /// `(a: (ref $array))` (T107): a hinted array into number mode when every element is a number or a
    /// hole and it has no element attributes; else the hint goes.
    ToNums,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::New => "new_array",
            Helper::NewHinted => "new_array_hinted",
            Helper::NewGeneric => "new_array_generic",
            Helper::OwnGet => "array_own_get",
            Helper::Define { set: true } => "array_define",
            Helper::Define { set: false } => "array_define_own",
            Helper::SetIndex => "array_set_index",
            Helper::DefineIndex => "array_define_index",
            Helper::SetLength => "array_set_length",
            Helper::Push => "array_push",
            Helper::Join => "array_join",
            Helper::IndexKeys => "array_index_keys",
            Helper::ProtoPush => "Array.prototype.push",
            Helper::ProtoPop => "Array.prototype.pop",
            Helper::ProtoJoin => "Array.prototype.join",
            Helper::ProtoToString => "Array.prototype.toString",
            Helper::FromArgs => "array_from_args",
            Helper::Rest => "array_rest",
            Helper::Attrs => "array_attrs",
            Helper::SetAttrs => "array_set_attrs",
            Helper::Resize => "array_resize",
            Helper::Elems => "array_elems",
            Helper::NumsResize => "array_nums_resize",
            Helper::ElemAt => "array_elem_at",
            Helper::ToNums => "array_to_nums",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let v = jsval();
        let a = rt(false, T_ARRAY);
        let s = string_ref();
        match self {
            Helper::New | Helper::NewHinted | Helper::NewGeneric => {
                (vec![rt(false, T_VALS), ValType::I32], vec![a])
            }
            Helper::OwnGet => (vec![a, s], vec![v]),
            Helper::Define { .. } => (vec![a, s, v], vec![ValType::I32]),
            Helper::SetIndex | Helper::DefineIndex => (vec![a, ValType::I32, v], vec![]),
            Helper::SetLength | Helper::Push => (vec![a, v], vec![]),
            Helper::Join => (vec![a, s], vec![s]),
            Helper::IndexKeys => (vec![a, ValType::I32], vec![rt(false, T_KEYS)]),
            Helper::ProtoPush | Helper::ProtoPop | Helper::ProtoJoin | Helper::ProtoToString => {
                (vec![v, v, v], vec![v])
            }
            Helper::FromArgs | Helper::Rest => (vec![v, ValType::I32], vec![v]),
            Helper::Attrs => (vec![a, ValType::I32], vec![ValType::I32]),
            Helper::SetAttrs => (vec![a, ValType::I32, ValType::I32], vec![]),
            Helper::Resize => (vec![a, ValType::I32], vec![]),
            Helper::Elems => (vec![rt(true, T_ARRAY)], vec![rt(false, T_VALS)]),
            Helper::NumsResize => (vec![a, ValType::I32], vec![]),
            Helper::ElemAt => (vec![a, ValType::I32], vec![v]),
            Helper::ToNums => (vec![a], vec![]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::New => new(cx),
            Helper::NewHinted => new_unchecked(cx, true),
            Helper::NewGeneric => new_unchecked(cx, false),
            Helper::OwnGet => own_get(cx),
            Helper::Define { set } => define(cx, set),
            Helper::SetIndex => set_index(cx, true),
            Helper::DefineIndex => set_index(cx, false),
            Helper::SetLength => set_length(cx),
            Helper::Push => push(cx),
            Helper::Join => join(cx),
            Helper::IndexKeys => index_keys(cx),
            Helper::ProtoPush => proto_push(cx),
            Helper::ProtoPop => proto_pop(cx),
            Helper::ProtoJoin => proto_join(cx),
            Helper::ProtoToString => proto_to_string(cx),
            Helper::FromArgs => from_args(cx),
            Helper::Rest => rest(cx),
            Helper::Attrs => attrs(),
            Helper::SetAttrs => set_attrs(cx),
            Helper::Resize => resize(cx),
            Helper::Elems => elems(cx),
            Helper::NumsResize => nums_resize(),
            Helper::ElemAt => elem_at(cx),
            Helper::ToNums => to_nums(),
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
    const ELEMS: u32 = 0;
    const LEN: u32 = 1;
    const I: u32 = 2;
    const V: u32 = 3;
    const X: u32 = 4;
    const N: u32 = 5;
    const DBL: u32 = 6;
    const HINTED: u32 = 7;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, jsval()),
        (1, ValType::F64),
        (1, rt(true, T_NUMS)),
        (2, ValType::I32),
    ]);
    let mut s = f.instructions();
    // T107: all numbers (none, for an empty one): hinted when they're all i31s, in number mode (the boxes
    // in `elems` dropped) when one isn't; anything else generic.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    s.local_get(ELEMS).local_get(I).array_get(T_VALS).local_tee(V);
    s.ref_test_non_null(HeapType::I31);
    s.if_(BlockType::Empty);
    s.else_();
    s.local_get(V).ref_test_non_null(HeapType::Concrete(runtime::T_NUMBER)).i32_eqz().br_if(2);
    s.i32_const(1).local_set(DBL);
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0);
    s.end().end();
    s.local_get(I).local_get(LEN).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(DBL).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(1).local_set(HINTED);
    s.else_();
    s.local_get(LEN).array_new_default(T_NUMS).local_set(N);
    s.i32_const(0).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(LEN).i32_ge_u().br_if(1);
    s.local_get(ELEMS).local_get(I).array_get(T_VALS).local_set(V);
    num_value(&mut s, V, X);
    s.drop();
    s.local_get(N).local_get(I).local_get(X).array_set(T_NUMS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0);
    s.end().end();
    s.i32_const(0).array_new_default(T_VALS).local_set(ELEMS);
    s.end();
    s.end();
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(ELEMS).local_get(LEN);
    s.ref_null(HeapType::Concrete(T_BYTES));
    s.local_get(N);
    s.local_get(HINTED);
    s.struct_new(T_ARRAY);
    s.end();
    f
}

/// [`Helper::NewHinted`] / [`Helper::NewGeneric`].
fn new_unchecked(cx: &mut dyn Cx, hinted: bool) -> Function {
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
    s.ref_null(HeapType::Concrete(T_NUMS));
    s.i32_const(hinted as i32);
    s.struct_new(T_ARRAY);
    s.end();
    f
}

/// Push 1 if local `n` (`nums`) means number mode: not null (T107).
pub fn is_num_mode(s: &mut InstructionSink, n: u32) {
    s.local_get(n).ref_is_null().i32_eqz();
}

/// `array_to_nums(a)` (see [`Helper::ToNums`]): `a` is hinted.
fn to_nums() -> Function {
    const A: u32 = 0;
    const E: u32 = 1;
    const N: u32 = 2;
    const I: u32 = 3;
    const V: u32 = 4;
    const X: u32 = 5;
    let mut f = Function::new([
        (1, rt(true, T_VALS)),
        (1, rt(true, T_NUMS)),
        (1, ValType::I32),
        (1, jsval()),
        (1, ValType::F64),
    ]);
    let mut s = f.instructions();
    s.block(BlockType::Empty);
    s.local_get(A).struct_get(T_ARRAY, F_EATTRS).ref_is_null().i32_eqz().br_if(0);
    s.local_get(A).struct_get(T_ARRAY, F_ELEMS).local_tee(E);
    s.array_len().array_new_default(T_NUMS).local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(A).struct_get(T_ARRAY, F_LENGTH).i32_ge_u().br_if(1);
    s.local_get(E).local_get(I).array_get(T_VALS).local_tee(V).ref_is_null();
    s.if_(BlockType::Empty);
    s.i64_const(HOLE).f64_reinterpret_i64().local_set(X);
    s.else_();
    num_value(&mut s, V, X);
    s.i32_eqz().br_if(3);
    s.end();
    s.local_get(N).local_get(I).local_get(X).array_set(T_NUMS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0);
    s.end().end();
    s.local_get(A).local_get(N).struct_set(T_ARRAY, F_NUMS);
    s.local_get(A).i32_const(0).struct_set(T_ARRAY, F_HINT);
    // Number mode keeps an empty `elems` (readers test its size first, `object::get`).
    s.local_get(A).i32_const(0).array_new_default(T_VALS).struct_set(T_ARRAY, F_ELEMS);
    s.return_();
    s.end();
    // Not all numbers (or attributes): generic from now on.
    s.local_get(A).i32_const(0).struct_set(T_ARRAY, F_HINT);
    s.end();
    f
}

/// `array_elems(a)` (see [`Helper::Elems`]): in number mode, box every element into a new `$vals` of
/// `length` slots (holes: null) and leave number mode for good.
fn elems(cx: &mut dyn Cx) -> Function {
    let box_ = cx.helper(runtime::Helper::Box);
    const A: u32 = 0;
    const N: u32 = 1;
    const NEW: u32 = 2;
    const I: u32 = 3;
    const X: u32 = 4;
    let mut f = Function::new([
        (1, rt(true, T_NUMS)),
        (1, rt(true, T_VALS)),
        (1, ValType::I32),
        (1, ValType::F64),
    ]);
    let mut s = f.instructions();
    s.local_get(A).struct_get(T_ARRAY, F_NUMS).local_tee(N).ref_is_null();
    s.if_(BlockType::Empty);
    // Generic (a hinted one loses its hint: the caller may store anything).
    s.local_get(A).i32_const(0).struct_set(T_ARRAY, F_HINT);
    s.local_get(A).struct_get(T_ARRAY, F_ELEMS).return_();
    s.end();

    // Exactly `length` slots: some readers take the length of `elems` as the array's (arrays built at
    // their final size, e.g. key lists); writes grow it as usual (`ensure`).
    s.local_get(A).struct_get(T_ARRAY, F_LENGTH).array_new_default(T_VALS).local_set(NEW);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(A).struct_get(T_ARRAY, F_LENGTH).i32_ge_u().br_if(1);
    s.local_get(N).local_get(I).array_get(T_NUMS).local_tee(X);
    s.i64_reinterpret_f64().i64_const(HOLE).i64_ne();
    s.if_(BlockType::Empty);
    s.local_get(NEW).local_get(I).local_get(X).call(box_).array_set(T_VALS);
    s.end();
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0);
    s.end().end();
    s.local_get(A).local_get(NEW).ref_as_non_null().struct_set(T_ARRAY, F_ELEMS);
    s.local_get(A).ref_null(HeapType::Concrete(T_NUMS)).struct_set(T_ARRAY, F_NUMS);
    s.local_get(NEW).ref_as_non_null();
    s.end();
    f
}

/// `array_nums_resize(a, len)` (see [`Helper::NumsResize`]).
fn nums_resize() -> Function {
    const A: u32 = 0;
    const L: u32 = 1;
    const N: u32 = 2;
    const CAP: u32 = 3;
    const NEW: u32 = 4;
    let mut f = Function::new([(1, rt(false, T_NUMS)), (1, ValType::I32), (1, rt(true, T_NUMS))]);
    let mut s = f.instructions();
    s.local_get(A).struct_get(T_ARRAY, F_NUMS).ref_as_non_null().local_tee(N);
    s.array_len().local_get(L).i32_lt_u();
    s.if_(BlockType::Empty);
    {
        // cap = max(len, 2 * old, 4)
        s.local_get(N).array_len().i32_const(1).i32_shl().local_tee(CAP);
        s.local_get(L);
        s.local_get(CAP).local_get(L).i32_gt_u();
        s.select().local_tee(CAP);
        s.i32_const(4);
        s.local_get(CAP).i32_const(4).i32_gt_u();
        s.select();
        s.array_new_default(T_NUMS).local_tee(NEW);
        s.i32_const(0).local_get(N).i32_const(0);
        s.local_get(A).struct_get(T_ARRAY, F_LENGTH);
        s.array_copy(T_NUMS, T_NUMS);
        s.local_get(A).local_get(NEW).struct_set(T_ARRAY, F_NUMS);
        s.local_get(NEW).ref_as_non_null().local_set(N);
    }
    s.end();
    s.local_get(L).local_get(A).struct_get(T_ARRAY, F_LENGTH).i32_gt_u();
    s.if_(BlockType::Empty);
    s.local_get(N);
    s.local_get(A).struct_get(T_ARRAY, F_LENGTH);
    s.i64_const(HOLE).f64_reinterpret_i64();
    s.local_get(L).local_get(A).struct_get(T_ARRAY, F_LENGTH).i32_sub();
    s.array_fill(T_NUMS);
    s.end();
    s.local_get(A).local_get(L).struct_set(T_ARRAY, F_LENGTH);
    s.end();
    f
}

/// `array_elem_at(a, i)` (see [`Helper::ElemAt`]).
fn elem_at(cx: &mut dyn Cx) -> Function {
    let box_ = cx.helper(runtime::Helper::Box);
    const A: u32 = 0;
    const I: u32 = 1;
    const N: u32 = 2;
    const X: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_NUMS)), (1, ValType::F64)]);
    let mut s = f.instructions();
    s.local_get(A).struct_get(T_ARRAY, F_NUMS).local_set(N);
    is_num_mode(&mut s, N);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(A).struct_get(T_ARRAY, F_ELEMS).local_get(I).array_get(T_VALS).return_();
    s.end();
    s.local_get(N).local_get(I).array_get(T_NUMS).local_tee(X);
    s.i64_reinterpret_f64().i64_const(HOLE).i64_eq();
    s.if_(BlockType::Empty);
    s.ref_null(HeapType::Abstract {
        shared: false,
        ty: wasm_encoder::AbstractHeapType::Eq,
    });
    s.return_();
    s.end();
    s.local_get(X).call(box_);
    s.end();
    f
}

/// Push 1 if local `v` holds a number (an i31 or a `$number`), its value (a NaN canonicalized, so it never
/// has [`HOLE`]'s bits) in local `x`; else 0 (T107).
pub fn num_value(s: &mut InstructionSink, v: u32, x: u32) {
    s.local_get(v).ref_test_non_null(HeapType::I31);
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(v)
        .ref_cast_non_null(HeapType::I31)
        .i31_get_s()
        .f64_convert_i32_s()
        .local_set(x);
    s.i32_const(1);
    s.else_();
    s.local_get(v).ref_test_non_null(HeapType::Concrete(runtime::T_NUMBER));
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(v)
        .ref_cast_non_null(HeapType::Concrete(runtime::T_NUMBER))
        .struct_get(runtime::T_NUMBER, 0)
        .local_tee(x)
        .local_get(x)
        .f64_ne();
    s.if_(BlockType::Empty);
    s.f64_const(f64::NAN.into()).local_set(x);
    s.end();
    s.i32_const(1);
    s.else_();
    s.i32_const(0);
    s.end();
    s.end();
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
fn set_attrs(cx: &mut dyn Cx) -> Function {
    let elems_h = ah(cx, Helper::Elems);
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
    s.local_get(A).call(elems_h).array_len();
    s.local_get(I).i32_const(1).i32_add();
    s.local_get(A).call(elems_h).array_len();
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
    let elem_at = ah(cx, Helper::ElemAt);
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
        .local_get(I)
        .i32_wrap_i64()
        .call(elem_at)
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

fn define(cx: &mut dyn Cx, set: bool) -> Function {
    let str_eq = cx.helper(runtime::Helper::StrEq);
    let set_index = ah(
        cx,
        if set {
            Helper::SetIndex
        } else {
            Helper::DefineIndex
        },
    );
    let set_length = ah(cx, Helper::SetLength);
    let length = cx.str("length");
    let sparse_ids = sparse_error(cx);
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
    sparse(&mut s, sparse_ids); // sparse arrays aren't implemented (F50/F160)
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

/// [`Helper::SetIndex`] (`set`) or [`Helper::DefineIndex`].
fn set_index(cx: &mut dyn Cx, set: bool) -> Function {
    let elems_h = ah(cx, Helper::Elems);
    let nums_resize = ah(cx, Helper::NumsResize);
    let to_nums = ah(cx, Helper::ToNums);
    let sparse_ids = sparse_error(cx);
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
    let set_attrs = ah(cx, Helper::SetAttrs);
    let redefine = cx.str("Cannot redefine property: ");
    let own_get = oh(cx, object::Helper::OwnGet);
    let find = oh(cx, object::Helper::FindOwn);
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
    const KEY: u32 = 7;
    const P: u32 = 8;
    const NUMS: u32 = 9;
    const XF: u32 = 10;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, rt(true, T_VALS)),
        (1, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_STRING)),
        (1, rt(true, runtime::T_OBJECT)),
        (1, rt(true, T_NUMS)),
        (1, ValType::F64),
    ]);
    let mut s = f.instructions();
    // T107: the common case first: overwriting an element that's there, in a generic array (a hinted one:
    // with an i31), without element attributes or `seal`/`freeze`.
    s.block(BlockType::Empty);
    {
        s.local_get(I).local_get(A).struct_get(T_ARRAY, F_LENGTH).i32_ge_u().br_if(0);
        s.local_get(A).struct_get(T_ARRAY, F_NUMS).ref_is_null().i32_eqz().br_if(0);
        s.local_get(A).struct_get(T_ARRAY, F_HINT);
        s.local_get(V).ref_test_non_null(HeapType::I31).i32_eqz();
        s.i32_and().br_if(0);
        s.local_get(A).struct_get(T_ARRAY, F_EATTRS).ref_is_null().i32_eqz().br_if(0);
        s.local_get(A)
            .struct_get(T_ARRAY, object::F_CLASS)
            .i32_const(object::SEALED | object::FROZEN)
            .i32_and()
            .br_if(0);
        s.local_get(A).struct_get(T_ARRAY, F_ELEMS).local_tee(NEW);
        s.local_get(I).array_get(T_VALS).ref_is_null().br_if(0);
        s.local_get(NEW).local_get(I).local_get(V).array_set(T_VALS);
        s.return_();
    }
    s.end();
    s.local_get(I).i32_const(MAX_DENSE).i32_ge_u();
    s.if_(BlockType::Empty);
    sparse(&mut s, sparse_ids);
    s.end();
    // T107: a generic array without the hint (the usual case for arrays of objects) skips all of this.
    s.local_get(A).struct_get(T_ARRAY, F_NUMS).ref_is_null().i32_eqz();
    s.local_get(A).struct_get(T_ARRAY, F_HINT);
    s.i32_or();
    s.if_(BlockType::Empty);
    // A hinted array keeps its hint for an i31, enters number mode for another number, loses the hint
    // for anything else.
    s.local_get(A).struct_get(T_ARRAY, F_HINT);
    s.if_(BlockType::Empty);
    s.local_get(V).ref_test_non_null(HeapType::I31).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(V).ref_test_non_null(HeapType::Concrete(runtime::T_NUMBER));
    s.if_(BlockType::Empty);
    s.local_get(A).call(to_nums);
    s.else_();
    s.local_get(A).i32_const(0).struct_set(T_ARRAY, F_HINT);
    s.end();
    s.end();
    s.end();
    // A number into an array in number mode with no attributes in the way stays unboxed; anything else
    // takes the general path below, which leaves number mode.
    s.block(BlockType::Empty);
    {
        s.local_get(A).struct_get(T_ARRAY, F_NUMS).local_set(NUMS);
        is_num_mode(&mut s, NUMS);
        s.i32_eqz().br_if(0);
        s.local_get(A)
            .struct_get(T_ARRAY, object::F_CLASS)
            .i32_const(object::NO_EXTEND | object::SEALED | object::FROZEN | object::LENGTH_READONLY)
            .i32_and()
            .br_if(0);
        num_value(&mut s, V, XF);
        s.i32_eqz().br_if(0);
        s.local_get(I).local_get(A).struct_get(T_ARRAY, F_LENGTH).i32_lt_u();
        s.if_(BlockType::Empty);
        if set {
            // A hole while the prototype chain may have index keys: the general path (T34).
            s.local_get(NUMS)
                .local_get(I)
                .array_get(T_NUMS)
                .i64_reinterpret_f64()
                .i64_const(HOLE)
                .i64_eq()
                .global_get(runtime::G_INDEX_PROTO)
                .i32_eqz()
                .i32_eqz()
                .i32_and()
                .br_if(1);
        }
        s.local_get(NUMS).local_get(I).local_get(XF).array_set(T_NUMS);
        s.return_();
        s.end();
        if set {
            s.global_get(runtime::G_INDEX_PROTO).br_if(0);
        }
        s.local_get(A).local_get(I).i32_const(1).i32_add().call(nums_resize);
        s.local_get(A)
            .struct_get(T_ARRAY, F_NUMS)
            .local_get(I)
            .local_get(XF)
            .array_set(T_NUMS);
        s.return_();
    }
    s.end();
    // Anything else leaves number mode here, once (a hinted array stays hinted: an i31 is stored); below,
    // `elems` is read directly.
    s.local_get(A).struct_get(T_ARRAY, F_NUMS).local_set(NUMS);
    is_num_mode(&mut s, NUMS);
    s.if_(BlockType::Empty);
    s.local_get(A).call(elems_h).drop();
    s.end();
    s.end();
    // An absent element (a hole, or past the end) while the prototype chain may have index keys
    // (T34): OrdinarySet goes up the chain; a setter there is called, a read-only property there
    // forbids the write.
    if set {
        s.global_get(runtime::G_INDEX_PROTO);
        s.if_(BlockType::Empty);
        s.local_get(I)
            .local_get(A)
            .struct_get(T_ARRAY, F_LENGTH)
            .i32_ge_u();
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(1);
        s.else_();
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(I)
            .array_get(T_VALS)
            .ref_is_null();
        s.end();
        s.if_(BlockType::Empty);
        {
            s.local_get(I)
                .f64_convert_i32_u()
                .call(number_to_string)
                .local_set(KEY);
            s.local_get(A)
                .struct_get(T_ARRAY, object::F_PROTO)
                .local_set(P);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(P).ref_is_null().br_if(1);
            s.local_get(P)
                .ref_as_non_null()
                .local_get(KEY)
                .ref_as_non_null()
                .call(own_get)
                .local_tee(X);
            s.ref_is_null().i32_eqz();
            s.if_(BlockType::Empty);
            {
                s.local_get(X)
                    .ref_test_non_null(HeapType::Concrete(runtime::T_ACCESSOR));
                s.if_(BlockType::Empty);
                s.local_get(X)
                    .ref_cast_non_null(HeapType::Concrete(runtime::T_ACCESSOR))
                    .struct_get(runtime::T_ACCESSOR, 1)
                    .local_tee(X);
                s.global_get(G_UNDEFINED).ref_eq();
                s.if_(BlockType::Empty);
                s.global_get(set_prop);
                s.local_get(KEY).ref_as_non_null().call(concat);
                s.global_get(only_getter).call(concat);
                s.call(throw);
                s.end();
                s.local_get(X).local_get(A);
                s.local_get(V).array_new_fixed(T_VALS, 1);
                s.global_get(setter).call(call_value).drop();
                s.return_();
                s.end();
                // A data property: read-only forbids the write, else the array gets its own element.
                s.local_get(P).ref_test_non_null(array());
                s.if_(BlockType::Result(ValType::I32));
                s.local_get(P)
                    .ref_cast_non_null(array())
                    .local_get(I)
                    .call(attrs);
                s.else_();
                s.local_get(P)
                    .ref_as_non_null()
                    .local_get(KEY)
                    .ref_as_non_null()
                    .call(find)
                    .local_tee(T);
                s.i32_const(0).i32_ge_s();
                s.if_(BlockType::Result(ValType::I32));
                s.local_get(P)
                    .ref_as_non_null()
                    .struct_get(runtime::T_OBJECT, object::F_ATTRS)
                    .local_get(T)
                    .array_get_u(T_BYTES);
                s.else_();
                s.i32_const(0);
                s.end();
                s.end();
                s.i32_const(object::READONLY).i32_and();
                s.if_(BlockType::Empty);
                s.global_get(read_only);
                s.local_get(KEY).ref_as_non_null().call(concat);
                s.global_get(of_array).call(concat);
                s.call(throw);
                s.end();
                s.br(2);
            }
            s.end();
            s.local_get(P)
                .ref_as_non_null()
                .struct_get(runtime::T_OBJECT, object::F_PROTO)
                .local_set(P);
            s.br(0).end().end();
        }
        s.end();
        s.end();
    }
    // Element attributes (T32): an accessor's setter, a read-only element; past the end, a
    // read-only `length`. Defining: an existing element that isn't configurable can only keep
    // being a writable, enumerable data element; a configurable one loses its attributes.
    s.local_get(I)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_lt_u();
    s.if_(BlockType::Empty);
    if !set {
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(I)
            .array_get(T_VALS)
            .ref_is_null()
            .i32_eqz();
        s.if_(BlockType::Empty);
        s.local_get(A).local_get(I).call(attrs).local_tee(T);
        s.i32_const(object::FIXED).i32_and();
        s.if_(BlockType::Empty);
        s.local_get(T)
            .i32_const(object::ACCESSOR | object::READONLY | object::HIDDEN)
            .i32_and();
        s.if_(BlockType::Empty);
        s.global_get(redefine);
        s.local_get(I).f64_convert_i32_u().call(number_to_string);
        s.call(concat);
        s.call(throw);
        s.end();
        s.else_();
        s.local_get(A).local_get(I).i32_const(0).call(set_attrs);
        s.end();
        s.local_get(A)
            .struct_get(T_ARRAY, F_ELEMS)
            .local_get(I)
            .local_get(V)
            .array_set(T_VALS);
        s.return_();
        s.end();
    }
    if set {
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
    let elems_h = ah(cx, Helper::Elems);
    let nums_resize = ah(cx, Helper::NumsResize);
    let sparse_ids = sparse_error(cx);
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
    const NUMS: u32 = 5;
    let mut f = Function::new([
        (1, rt(true, T_VALS)),
        (1, ValType::I32),
        (1, rt(true, T_BYTES)),
        (1, rt(true, T_NUMS)),
    ]);
    let mut s = f.instructions();
    s.local_get(L).i32_const(MAX_DENSE).i32_gt_u();
    s.if_(BlockType::Empty);
    sparse(&mut s, sparse_ids); // sparse arrays aren't implemented (F50/F160)
    s.end();
    // T107: number mode (every element a configurable data element): just the length. A hinted array
    // stays hinted (holes are fine).
    s.local_get(A).struct_get(T_ARRAY, F_NUMS).local_set(NUMS);
    is_num_mode(&mut s, NUMS);
    s.local_get(A)
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::SEALED | object::FROZEN)
        .i32_and()
        .i32_eqz()
        .i32_and();
    s.if_(BlockType::Empty);
    s.local_get(A).local_get(L).call(nums_resize);
    s.return_();
    s.end();
    is_num_mode(&mut s, NUMS);
    s.if_(BlockType::Empty);
    s.local_get(A).call(elems_h).drop();
    s.end();
    s.local_get(L)
        .local_get(A)
        .struct_get(T_ARRAY, F_LENGTH)
        .i32_lt_u();
    s.if_(BlockType::Empty);
    {
        // Elements that aren't configurable stop the deletion (T32).
        s.local_get(A)
            .struct_get(T_ARRAY, F_EATTRS)
            .ref_is_null()
            .i32_eqz();
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
        s.local_get(L)
            .local_get(B)
            .ref_as_non_null()
            .array_len()
            .i32_lt_u();
        s.if_(BlockType::Empty);
        s.local_get(B).ref_as_non_null().local_get(L).i32_const(0);
        s.local_get(B)
            .ref_as_non_null()
            .array_len()
            .local_get(L)
            .i32_sub();
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

/// Literals, spread, rest: definitions (T34), never the prototype chain.
fn push(cx: &mut dyn Cx) -> Function {
    let set_index = ah(cx, Helper::DefineIndex);
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
    let elems_h = ah(cx, Helper::Elems);
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
        .call(elems_h)
        .local_get(I)
        .array_get(T_VALS)
        .local_set(E);
    // T76: a string element is its own part.
    runtime::is_string(&mut s, E);
    s.if_(BlockType::Empty);
    s.local_get(E)
        .ref_cast_non_null(runtime::string())
        .local_set(P);
    s.else_();
    s.local_get(E).ref_is_null();
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
    let elems_h = ah(cx, Helper::Elems);
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
    s.local_get(A)
        .struct_get(T_ARRAY, F_EATTRS)
        .ref_is_null()
        .i32_eqz();
    s.i32_and().local_set(HID);
    let elem = |s: &mut InstructionSink| {
        s.local_get(A)
            .call(elems_h)
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

/// `this` must be an array for the native code: any other `this` goes to the prelude's generic
/// method (T35), linked when the code names it.
fn this_array(cx: &mut dyn Cx, s: &mut InstructionSink, name: &str) {
    s.local_get(1).ref_test_non_null(array()).i32_eqz();
    s.if_(BlockType::Empty);
    match cx.array_generic(name) {
        Some(generic) => {
            s.local_get(0).local_get(1).local_get(2).call(generic).return_();
        }
        None => {
            // Only reachable without the name in the code (e.g. `[].push` taken by a
            // computed key built at run time, F109).
            let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
            let msg = cx.str(&format!(
                "Array.prototype.{name} is only implemented for arrays (js2wasi3)"
            ));
            s.global_get(msg).call(throw);
        }
    }
    s.end();
}

fn args_len(s: &mut InstructionSink) {
    s.local_get(2).ref_cast_non_null(vals()).array_len();
}

fn proto_push(cx: &mut dyn Cx) -> Function {
    let set_index = ah(cx, Helper::SetIndex);
    let resize = ah(cx, Helper::Resize);
    let throw = oh(cx, object::Helper::Throw(ErrorKind::TypeError));
    let length_read_only =
        cx.str("Cannot assign to read only property 'length' of object '[object Array]'");
    const I: u32 = 3;
    const N: u32 = 4;
    const L: u32 = 5;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut s = f.instructions();
    this_array(cx, &mut s, "push");
    args_len(&mut s);
    s.local_set(N);
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, F_LENGTH)
        .local_set(L);
    // `a[len + i] = args[i]` ([[Set]]: a setter on the prototype chain may take it, T34).
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(1).ref_cast_non_null(array());
    s.local_get(L).local_get(I).i32_add();
    s.local_get(2)
        .ref_cast_non_null(vals())
        .local_get(I)
        .array_get(T_VALS);
    s.call(set_index);
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
    // `length = len + argc`, even for elements a setter took.
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, F_LENGTH)
        .local_get(L)
        .local_get(N)
        .i32_add()
        .i32_ne();
    s.if_(BlockType::Empty);
    s.local_get(1).ref_cast_non_null(array());
    s.local_get(L).local_get(N).i32_add().call(resize);
    s.end();
    s.local_get(L).local_get(N).i32_add().ref_i31();
    s.end();
    f
}

fn proto_pop(cx: &mut dyn Cx) -> Function {
    let elems_h = ah(cx, Helper::Elems);
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
    const N: u32 = 5;
    const X: u32 = 6;
    const T: u32 = 7;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, jsval()),
        (1, rt(true, T_NUMS)),
        (1, ValType::F64),
        (1, ValType::I32),
    ]);
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
    // T107: number mode (not sealed/frozen): the last element boxed and a shorter `length`.
    s.local_get(1).ref_cast_non_null(array()).struct_get(T_ARRAY, F_NUMS).local_set(N);
    s.block(BlockType::Empty);
    is_num_mode(&mut s, N);
    s.i32_eqz().br_if(0);
    s.local_get(1)
        .ref_cast_non_null(array())
        .struct_get(T_ARRAY, object::F_CLASS)
        .i32_const(object::SEALED | object::FROZEN)
        .i32_and()
        .br_if(0);
    s.local_get(N).local_get(L).array_get(T_NUMS).local_tee(X);
    s.i64_reinterpret_f64().i64_const(HOLE).i64_eq().br_if(0);
    s.local_get(1).ref_cast_non_null(array()).local_get(L).struct_set(T_ARRAY, F_LENGTH);
    runtime::box_inline(&mut s, X, T);
    s.return_();
    s.end();
    // Get the last element (an accessor's getter, T32), then delete it: not when it isn't
    // configurable (sealed, frozen or by its attributes); then the new length. (`elems` directly
    // when generic.)
    s.local_get(N).ref_is_null();
    s.if_(BlockType::Result(rt(false, T_VALS)));
    s.local_get(1).ref_cast_non_null(array()).struct_get(T_ARRAY, F_ELEMS);
    s.else_();
    s.local_get(1).ref_cast_non_null(array()).call(elems_h);
    s.end();
    s.local_get(L)
        .array_get(T_VALS)
        .local_tee(V);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    // A hole reads through the prototype chain (T33).
    s.local_get(1).local_get(L).ref_i31().call(get).local_set(V);
    s.else_();
    s.local_get(V).local_get(1).call(value).local_set(V);
    s.local_get(1)
        .ref_cast_non_null(array())
        .local_get(L)
        .call(attrs);
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
    s.local_get(1)
        .ref_cast_non_null(array())
        .local_get(L)
        .i32_const(0)
        .call(set_attrs);
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

/// `$array_rest(a, k)` (T63), see [`Helper::Rest`]: `a`'s store up to its length.
fn rest(cx: &mut dyn Cx) -> Function {
    let elems_h = ah(cx, Helper::Elems);
    let new = ah(cx, Helper::NewGeneric);
    const A: u32 = 0;
    const K: u32 = 1;
    const N: u32 = 2;
    const E: u32 = 3;
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_VALS))]);
    let mut s = f.instructions();
    // n = max(length - k, 0)
    s.i32_const(0);
    s.local_get(A).ref_cast_non_null(array()).struct_get(T_ARRAY, F_LENGTH);
    s.local_get(K).i32_sub();
    s.local_get(A).ref_cast_non_null(array()).struct_get(T_ARRAY, F_LENGTH);
    s.local_get(K).i32_le_u();
    s.select().local_tee(N);
    s.array_new_default(T_VALS).local_set(E);
    s.local_get(N);
    s.if_(BlockType::Empty);
    s.local_get(E).ref_as_non_null().i32_const(0);
    s.local_get(A).ref_cast_non_null(array()).call(elems_h);
    s.local_get(K).local_get(N);
    s.array_copy(T_VALS, T_VALS);
    s.end();
    s.local_get(E).ref_as_non_null().local_get(N).call(new);
    s.end();
    f
}

fn from_args(cx: &mut dyn Cx) -> Function {
    let new = ah(cx, Helper::NewGeneric);
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
