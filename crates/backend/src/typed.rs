//! Typed arrays and `ArrayBuffer`s (T20). The methods and constructors are in the prelude
//! (`crates/lower/builtins/typedarray.js`); this module has their representation, the
//! `Intrinsic::Typed` operations, element access and `console.log`.
//!
//! - `$buffer` (an `ArrayBuffer`): the object fields, then `data: (ref $bytes)` (fixed length).
//! - `$typed` (a typed array): the object fields, then `kind` (an index into
//!   `js2wasi3_ir::TYPED_NAMES`), `buffer: (ref $buffer)`, `offset` (bytes) and `length`
//!   (elements). Elements are little-endian in the buffer (like V8 on every platform it runs on).
//! - [`Helper::Get`] / [`Helper::Set`] are the integer-indexed element accesses `object::get` /
//!   `object::set` use for a typed array and an index: out of range reads `undefined`, writes are
//!   dropped. Writing converts like the spec: ToInt32 modulo the element width, clamping (round
//!   half to even) for `Uint8ClampedArray`, rounding to `f32` for `Float32Array`.
//! - T47, only in a module with `ir::Module::rab`: `$buffer`'s `data` is mutable (`resize`
//!   replaces it: the byte length is always `array.len(data)`; a detached buffer's is empty) and
//!   it has `max` (the `maxByteLength` as a JS number, null for a fixed-length buffer) and
//!   `detached`; `$typed` has `tracking` (1: a length-tracking view, `length` unused). Every
//!   native reader of a typed array's length then goes through [`Helper::Len`] ([`Length`]):
//!   0 when out of bounds, (byte length − offset) / size when tracking. Other modules keep the
//!   fixed layout and read the `length` field.

use js2wasi3_ir::{Builtin, TYPED_NAMES, TypedOp};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::inspect;
use crate::object;
use crate::runtime::{
    self, Cx, T_BUFFER, T_BYTES, T_I32S, T_KEYS, T_TYPED, T_VALS, jsval, rt, string_ref,
};

/// `$buffer` / `$typed` field indices after the object's.
pub const F_DATA: u32 = 7;
pub const F_KIND: u32 = 7;
pub const F_BUF: u32 = 8;
pub const F_OFFSET: u32 = 9;
pub const F_LENGTH: u32 = 10;
/// T47 (`Cx::rab`).
pub const F_MAX: u32 = 8;
pub const F_DETACHED: u32 = 9;
pub const F_TRACKING: u32 = 11;

const UINT8_CLAMPED: i32 = 2;
const FLOAT32: i32 = 7;
const FLOAT64: i32 = 8;

const EQ_HEAP: HeapType = HeapType::Abstract {
    shared: false,
    ty: wasm_encoder::AbstractHeapType::Eq,
};

pub fn typed() -> HeapType {
    HeapType::Concrete(T_TYPED)
}

pub fn buffer() -> HeapType {
    HeapType::Concrete(T_BUFFER)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `Intrinsic::Typed(op)`, with its signature.
    Op(TypedOp),
    /// `(t: (ref $typed), i: i32) -> jsval`: element `i` (in range).
    Get,
    /// `(t: (ref $typed), i: i32, x: f64)`: store `x` converted (in range).
    Set,
    /// `(t: (ref $typed), i: i32, b: jsval (a BigInt))`: store `b` modulo 2^64 (in range; T48).
    SetBig,
    /// `(t: (ref $typed), depth, indent) -> (ref $string)`: `Uint8Array(3) [ 1, 2, 3 ]`.
    Format,
    /// `(b: (ref $buffer), depth, indent) -> (ref $string)`: `ArrayBuffer { [Uint8Contents]:
    /// <01 02>, [byteLength]: 2 }`.
    FormatBuffer,
    /// `(kind: i32) -> (ref $string)`: the constructor name.
    Name,
    /// `(t: (ref $typed)) -> i32`: its current length (T47: TypedArrayLength, 0 when out of
    /// bounds). Only with `Cx::rab`; see [`Length`].
    Len,
}

/// How to read a typed array's length (T47): the field, or [`Helper::Len`] in a module with
/// resizable buffers.
#[derive(Clone, Copy)]
pub struct Length(Option<u32>);

impl Length {
    pub fn new(cx: &mut dyn Cx) -> Length {
        Length(if cx.rab() { Some(th(cx, Helper::Len)) } else { None })
    }

    /// `(ref $typed)` on the stack → its length.
    pub fn emit(self, s: &mut InstructionSink) {
        match self.0 {
            Some(f) => {
                s.call(f);
            }
            None => {
                s.struct_get(T_TYPED, F_LENGTH);
            }
        }
    }
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Op(op) => js2wasi3_ir::Intrinsic::Typed(op).name(),
            Helper::Get => "typed_get",
            Helper::Set => "typed_set",
            Helper::SetBig => "typed_set_bigint",
            Helper::Format => "inspect_typed_array",
            Helper::FormatBuffer => "inspect_array_buffer",
            Helper::Name => "typed_array_name",
            Helper::Len => "typed_current_length",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        let v = jsval();
        let t = rt(false, T_TYPED);
        match self {
            Helper::Op(op) => {
                let (params, result) = js2wasi3_ir::Intrinsic::Typed(op).signature();
                let ty = |t: &js2wasi3_ir::Type| match t {
                    js2wasi3_ir::Type::JsVal => v,
                    _ => I32,
                };
                (
                    params.iter().map(ty).collect(),
                    result.iter().map(ty).collect(),
                )
            }
            Helper::Get => (vec![t, I32], vec![v]),
            Helper::Set => (vec![t, I32, ValType::F64], vec![]),
            Helper::SetBig => (vec![t, I32, v], vec![]),
            Helper::Format => (vec![t, I32, I32], vec![string_ref()]),
            Helper::FormatBuffer => (vec![rt(false, T_BUFFER), I32, I32], vec![string_ref()]),
            Helper::Name => (vec![I32], vec![string_ref()]),
            Helper::Len => (vec![t], vec![I32]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Op(op) => op_body(cx, op),
            Helper::Get => get(cx),
            Helper::Set => set(),
            Helper::SetBig => set_big(cx),
            Helper::Format => format(cx),
            Helper::FormatBuffer => format_buffer(cx),
            Helper::Len => current_length(),
            Helper::Name => {
                let kinds = cx.typed_kinds() as usize;
                let names: Vec<u32> = TYPED_NAMES[..kinds].iter().map(|n| cx.str(n)).collect();
                let mut f = Function::new([]);
                let mut s = f.instructions();
                for (k, g) in names.iter().enumerate() {
                    s.local_get(0).i32_const(k as i32).i32_eq();
                    s.if_(BlockType::Empty);
                    s.global_get(*g).return_();
                    s.end();
                }
                s.global_get(names[0]);
                s.end();
                f
            }
        }
    }
}

fn th(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Typed(h))
}

/// Push the element size of the kind in local `kind` (1, 2, 4 or 8).
fn size_of(s: &mut InstructionSink, kind: u32) {
    // kind < 3: 1; < 5: 2; < 8: 4; else 8.
    s.i32_const(1).i32_const(2);
    s.local_get(kind).i32_const(3).i32_lt_u();
    s.select();
    s.i32_const(4);
    s.local_get(kind).i32_const(5).i32_lt_u();
    s.select();
    s.i32_const(8);
    s.local_get(kind).i32_const(8).i32_lt_u();
    s.select();
}

/// Push the byte of `data` (local) at `base + k` (local `base`), zero-extended.
fn byte(s: &mut InstructionSink, data: u32, base: u32, k: i32) {
    s.local_get(data).local_get(base);
    if k != 0 {
        s.i32_const(k).i32_add();
    }
    s.array_get_u(T_BYTES);
}

/// Push the little-endian unsigned value of `n` (≤ 4) bytes at `base`.
fn load(s: &mut InstructionSink, data: u32, base: u32, n: i32) {
    byte(s, data, base, 0);
    for k in 1..n {
        byte(s, data, base, k);
        s.i32_const(8 * k).i32_shl().i32_or();
    }
}

/// Store the low `n` bytes of the `i32` in local `v` at `base`.
fn store(s: &mut InstructionSink, data: u32, base: u32, v: u32, n: i32) {
    for k in 0..n {
        s.local_get(data).local_get(base);
        if k != 0 {
            s.i32_const(k).i32_add();
        }
        s.local_get(v);
        if k != 0 {
            s.i32_const(8 * k).i32_shr_u();
        }
        s.array_set(T_BYTES);
    }
}

fn get(cx: &mut dyn Cx) -> Function {
    let box_ = cx.helper(runtime::Helper::Box);
    const T: u32 = 0;
    const I: u32 = 1;
    const KIND: u32 = 2;
    const DATA: u32 = 3;
    const BASE: u32 = 4;
    const W: u32 = 5;
    let mut locals = vec![(1, ValType::I32), (1, rt(true, T_BYTES)), (1, ValType::I32)];
    if cx.big_typed() {
        locals.push((1, ValType::I64));
    }
    let mut f = Function::new(locals);
    let mut s = f.instructions();
    s.local_get(T).struct_get(T_TYPED, F_KIND).local_set(KIND);
    s.local_get(T)
        .struct_get(T_TYPED, F_BUF)
        .struct_get(T_BUFFER, F_DATA)
        .local_set(DATA);
    s.local_get(T).struct_get(T_TYPED, F_OFFSET);
    s.local_get(I);
    size_of(&mut s, KIND);
    s.i32_mul().i32_add().local_set(BASE);
    if cx.big_typed() {
        // T48: BigInt64Array / BigUint64Array: a `$bigint` of the 8 bytes (sign, magnitude).
        let norm = cx.helper(runtime::Helper::Big(crate::bigint::Helper::Norm));
        s.local_get(KIND).i32_const(js2wasi3_ir::TYPED_BIGINT64 as i32).i32_ge_u();
        s.if_(BlockType::Empty);
        load(&mut s, DATA, BASE, 4);
        s.i64_extend_i32_u();
        s.local_get(BASE).i32_const(4).i32_add().local_set(BASE);
        load(&mut s, DATA, BASE, 4);
        s.i64_extend_i32_u().i64_const(32).i64_shl().i64_or().local_set(W);
        // Negative (BigInt64Array only): sign 1, magnitude -w.
        s.local_get(KIND).i32_const(js2wasi3_ir::TYPED_BIGINT64 as i32).i32_eq();
        s.local_get(W).i64_const(0).i64_lt_s().i32_and().local_tee(BASE);
        s.if_(BlockType::Empty);
        s.i64_const(0).local_get(W).i64_sub().local_set(W);
        s.end();
        s.local_get(BASE);
        s.local_get(W).i32_wrap_i64();
        s.local_get(W).i64_const(32).i64_shr_u().i32_wrap_i64();
        s.array_new_fixed(runtime::T_BIGINT, 3);
        s.call(norm).return_();
        s.end();
    }
    s.block(BlockType::Result(ValType::F64));
    // Int8, Uint8, Uint8Clamped, Int16, Uint16, Int32, Uint32, Float32, Float64.
    for kind in 0..9 {
        s.local_get(KIND).i32_const(kind).i32_eq();
        s.if_(BlockType::Empty);
        match kind {
            0 => {
                s.local_get(DATA).local_get(BASE).array_get_s(T_BYTES);
                s.f64_convert_i32_s();
            }
            1 | 2 => {
                byte(&mut s, DATA, BASE, 0);
                s.f64_convert_i32_u();
            }
            3 => {
                load(&mut s, DATA, BASE, 2);
                s.i32_extend16_s().f64_convert_i32_s();
            }
            4 => {
                load(&mut s, DATA, BASE, 2);
                s.f64_convert_i32_u();
            }
            5 => {
                load(&mut s, DATA, BASE, 4);
                s.f64_convert_i32_s();
            }
            6 => {
                load(&mut s, DATA, BASE, 4);
                s.f64_convert_i32_u();
            }
            7 => {
                load(&mut s, DATA, BASE, 4);
                s.f32_reinterpret_i32().f64_promote_f32();
            }
            _ => {
                load(&mut s, DATA, BASE, 4);
                s.i64_extend_i32_u();
                s.local_get(BASE).i32_const(4).i32_add().local_set(BASE);
                load(&mut s, DATA, BASE, 4);
                s.i64_extend_i32_u().i64_const(32).i64_shl().i64_or();
                s.f64_reinterpret_i64();
            }
        }
        s.br(1);
        s.end();
    }
    s.unreachable();
    s.end();
    s.call(box_);
    s.end();
    f
}

/// Push ToInt32 of the `f64` in local `x`.
fn to_int32(s: &mut InstructionSink, x: u32) {
    // NaN and ±Infinity: 0; else trunc(x) modulo 2^32 (exact in f64 for every finite x).
    s.local_get(x)
        .local_get(x)
        .f64_sub()
        .local_get(x)
        .local_get(x)
        .f64_sub()
        .f64_eq();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(x).f64_trunc();
    s.local_get(x)
        .f64_trunc()
        .f64_const(4294967296.0.into())
        .f64_div()
        .f64_floor();
    s.f64_const(4294967296.0.into()).f64_mul();
    s.f64_sub();
    s.i64_trunc_f64_u().i32_wrap_i64();
    s.else_();
    s.i32_const(0);
    s.end();
}

fn set() -> Function {
    const T: u32 = 0;
    const I: u32 = 1;
    const X: u32 = 2;
    const KIND: u32 = 3;
    const DATA: u32 = 4;
    const BASE: u32 = 5;
    const V: u32 = 6;
    const W: u32 = 7;
    let mut f = Function::new([
        (1, ValType::I32),
        (1, rt(true, T_BYTES)),
        (2, ValType::I32),
        (1, ValType::I64),
    ]);
    let mut s = f.instructions();
    s.local_get(T).struct_get(T_TYPED, F_KIND).local_set(KIND);
    s.local_get(T)
        .struct_get(T_TYPED, F_BUF)
        .struct_get(T_BUFFER, F_DATA)
        .local_set(DATA);
    s.local_get(T).struct_get(T_TYPED, F_OFFSET);
    s.local_get(I);
    size_of(&mut s, KIND);
    s.i32_mul().i32_add().local_set(BASE);
    // Float64.
    s.local_get(KIND).i32_const(FLOAT64).i32_eq();
    s.if_(BlockType::Empty);
    {
        s.local_get(X).i64_reinterpret_f64().local_set(W);
        s.local_get(W).i32_wrap_i64().local_set(V);
        store(&mut s, DATA, BASE, V, 4);
        s.local_get(W)
            .i64_const(32)
            .i64_shr_u()
            .i32_wrap_i64()
            .local_set(V);
        s.local_get(BASE).i32_const(4).i32_add().local_set(BASE);
        store(&mut s, DATA, BASE, V, 4);
        s.return_();
    }
    s.end();
    s.local_get(KIND).i32_const(FLOAT32).i32_eq();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(X).f32_demote_f64().i32_reinterpret_f32();
    s.else_();
    s.local_get(KIND).i32_const(UINT8_CLAMPED).i32_eq();
    s.if_(BlockType::Result(ValType::I32));
    {
        // NaN and x ≤ 0: 0; x ≥ 255: 255; else rounded half to even.
        s.local_get(X).f64_const(0.0.into()).f64_gt();
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(255);
        s.local_get(X).f64_nearest().i32_trunc_sat_f64_u();
        s.local_get(X).f64_const(255.0.into()).f64_ge();
        s.select();
        s.else_();
        s.i32_const(0);
        s.end();
    }
    s.else_();
    to_int32(&mut s, X);
    s.end();
    s.end();
    s.local_set(V);
    // Width: 1, 2 or 4 bytes.
    s.local_get(KIND).i32_const(3).i32_lt_u();
    s.if_(BlockType::Empty);
    store(&mut s, DATA, BASE, V, 1);
    s.return_();
    s.end();
    s.local_get(KIND).i32_const(5).i32_lt_u();
    s.if_(BlockType::Empty);
    store(&mut s, DATA, BASE, V, 2);
    s.return_();
    s.end();
    store(&mut s, DATA, BASE, V, 4);
    s.end();
    f
}

/// [`Helper::SetBig`]: the low 64 bits of the two's complement.
fn set_big(cx: &mut dyn Cx) -> Function {
    let to_tc = cx.helper(runtime::Helper::Big(crate::bigint::Helper::ToTc));
    const T: u32 = 0;
    const I: u32 = 1;
    const B: u32 = 2;
    const DATA: u32 = 3;
    const BASE: u32 = 4;
    const TC: u32 = 5;
    const V: u32 = 6;
    let mut f = Function::new([
        (1, rt(true, T_BYTES)),
        (1, ValType::I32),
        (1, rt(true, T_I32S)),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    s.local_get(T)
        .struct_get(T_TYPED, F_BUF)
        .struct_get(T_BUFFER, F_DATA)
        .local_set(DATA);
    s.local_get(T).struct_get(T_TYPED, F_OFFSET);
    s.local_get(I).i32_const(8).i32_mul().i32_add().local_set(BASE);
    s.local_get(B).ref_cast_non_null(crate::bigint::bigint());
    s.i32_const(2).call(to_tc).local_set(TC);
    for half in 0..2 {
        s.local_get(TC).ref_as_non_null().i32_const(half).array_get(T_I32S).local_set(V);
        store(&mut s, DATA, BASE, V, 4);
        if half == 0 {
            s.local_get(BASE).i32_const(4).i32_add().local_set(BASE);
        }
    }
    s.end();
    f
}

/// Push a new `$buffer` of `len` (local) zeroed bytes.
fn new_buffer(cx: &mut dyn Cx, s: &mut InstructionSink, len: u32) {
    let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
        Builtin::ArrayBuffer,
    )));
    object_fields(s, proto);
    s.local_get(len).array_new_default(T_BYTES);
    if cx.rab() {
        s.ref_null(EQ_HEAP).i32_const(0);
    }
    s.struct_new(T_BUFFER);
}

/// Push the byte length of the `(ref $buffer)` on the stack.
fn buffer_length(s: &mut InstructionSink) {
    s.struct_get(T_BUFFER, F_DATA).array_len();
}

/// Push the element size of the `$typed` in local `t` (local `kind` is set to its kind).
fn elem_size(s: &mut InstructionSink, t: u32, kind: u32) {
    s.local_get(t).struct_get(T_TYPED, F_KIND).local_set(kind);
    size_of(s, kind);
}

/// Push IsTypedArrayOutOfBounds of the `$typed` in local `t` (T47), without the detached check;
/// locals `kind` and `blen` are scratch.
fn out_of_bounds(s: &mut InstructionSink, t: u32, kind: u32, blen: u32) {
    s.local_get(t).struct_get(T_TYPED, F_BUF);
    buffer_length(s);
    s.local_set(blen);
    s.local_get(t).struct_get(T_TYPED, F_TRACKING);
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(t).struct_get(T_TYPED, F_OFFSET).local_get(blen).i32_gt_u();
    s.else_();
    // offset + length × size > byte length (both terms < 2^31: no unsigned overflow).
    s.local_get(t).struct_get(T_TYPED, F_OFFSET);
    s.local_get(t).struct_get(T_TYPED, F_LENGTH);
    elem_size(s, t, kind);
    s.i32_mul().i32_add();
    s.local_get(blen).i32_gt_u();
    s.end();
}

/// [`Helper::Len`]. A detached buffer's bytes are empty, so it needs no check of its own.
fn current_length() -> Function {
    const T: u32 = 0;
    const KIND: u32 = 1;
    const BLEN: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    out_of_bounds(&mut s, T, KIND, BLEN);
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.local_get(T).struct_get(T_TYPED, F_TRACKING);
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(BLEN).local_get(T).struct_get(T_TYPED, F_OFFSET).i32_sub();
    elem_size(&mut s, T, KIND);
    s.i32_div_u();
    s.else_();
    s.local_get(T).struct_get(T_TYPED, F_LENGTH);
    s.end();
    s.end();
    f
}

/// Push the object fields of a new plain-class object with the prototype `proto` gives.
fn object_fields(s: &mut InstructionSink, proto: u32) {
    s.i32_const(object::CLASS_PLAIN).call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
}

/// Push a new `$typed` of the kind in local `kind` on the `$buffer` in local `buf`; `len: None`
/// makes a length-tracking one (T47, `Cx::rab` only).
fn new_typed(
    cx: &mut dyn Cx,
    s: &mut InstructionSink,
    kind: u32,
    buf: u32,
    off: u32,
    len: Option<u32>,
) {
    let protos: Vec<u32> = (0..cx.typed_kinds())
        .map(|k| {
            cx.helper(runtime::Helper::Obj(object::Helper::Proto(Builtin::Typed(
                k,
            ))))
        })
        .collect();
    s.i32_const(object::CLASS_PLAIN);
    s.block(BlockType::Result(rt(false, runtime::T_OBJECT)));
    for (k, p) in protos.iter().enumerate() {
        s.local_get(kind).i32_const(k as i32).i32_eq();
        s.if_(BlockType::Empty);
        s.call(*p).br(1);
        s.end();
    }
    s.unreachable();
    s.end();
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.local_get(kind);
    s.local_get(buf).ref_cast_non_null(buffer());
    match len {
        Some(len) => {
            s.local_get(off).local_get(len);
            if cx.rab() {
                s.i32_const(0);
            }
        }
        None => {
            // T47: length-tracking.
            s.local_get(off).i32_const(0).i32_const(1);
        }
    }
    s.struct_new(T_TYPED);
}

fn op_body(cx: &mut dyn Cx, op: TypedOp) -> Function {
    let t = |s: &mut InstructionSink, l: u32| {
        s.local_get(l).ref_cast_non_null(typed());
    };
    let b = |s: &mut InstructionSink, l: u32| {
        s.local_get(l).ref_cast_non_null(buffer());
    };
    match op {
        TypedOp::New => {
            // (kind, length)
            const KIND: u32 = 0;
            const LEN: u32 = 1;
            const BYTES: u32 = 2;
            const BUF: u32 = 3;
            const ZERO: u32 = 4;
            let mut f = Function::new([(1, ValType::I32), (1, jsval()), (1, ValType::I32)]);
            let mut s = f.instructions();
            s.local_get(LEN);
            size_of(&mut s, KIND);
            s.i32_mul().local_set(BYTES);
            new_buffer(cx, &mut s, BYTES);
            s.local_set(BUF);
            new_typed(cx, &mut s, KIND, BUF, ZERO, Some(LEN));
            s.end();
            f
        }
        TypedOp::View => {
            // (kind, b, offset, length)
            let mut f = Function::new([]);
            let mut s = f.instructions();
            new_typed(cx, &mut s, 0, 1, 2, Some(3));
            s.end();
            f
        }
        TypedOp::Kind => {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            s.local_get(0).ref_test_non_null(typed());
            s.if_(BlockType::Result(ValType::I32));
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_KIND);
            s.else_();
            s.i32_const(-1);
            s.end();
            s.end();
            f
        }
        TypedOp::Length => {
            let length = Length::new(cx);
            let mut f = Function::new([]);
            let mut s = f.instructions();
            t(&mut s, 0);
            length.emit(&mut s);
            s.end();
            f
        }
        TypedOp::ByteOffset | TypedOp::Buffer => {
            let field = if op == TypedOp::ByteOffset { F_OFFSET } else { F_BUF };
            let mut f = Function::new([]);
            let mut s = f.instructions();
            t(&mut s, 0);
            s.struct_get(T_TYPED, field);
            s.end();
            f
        }
        TypedOp::IsBuffer => {
            let mut f = Function::new([]);
            f.instructions()
                .local_get(0)
                .ref_test_non_null(buffer())
                .end();
            f
        }
        TypedOp::BufferNew => {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            new_buffer(cx, &mut s, 0);
            s.end();
            f
        }
        TypedOp::BufferLength => {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            b(&mut s, 0);
            s.struct_get(T_BUFFER, F_DATA).array_len();
            s.end();
            f
        }
        TypedOp::BufferSlice => {
            // (b, start, end)
            const LEN: u32 = 3;
            const NEW: u32 = 4;
            let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_BUFFER))]);
            let mut s = f.instructions();
            s.local_get(2).local_get(1).i32_sub().local_set(LEN);
            new_buffer(cx, &mut s, LEN);
            s.local_set(NEW);
            s.local_get(NEW)
                .ref_as_non_null()
                .struct_get(T_BUFFER, F_DATA);
            s.i32_const(0);
            b(&mut s, 0);
            s.struct_get(T_BUFFER, F_DATA);
            s.local_get(1).local_get(LEN);
            s.array_copy(T_BYTES, T_BYTES);
            s.local_get(NEW);
            s.end();
            f
        }
        TypedOp::ViewTracking => {
            // (kind, b, offset)
            let mut f = Function::new([]);
            let mut s = f.instructions();
            new_typed(cx, &mut s, 0, 1, 2, None);
            s.end();
            f
        }
        TypedOp::OutOfBounds => {
            const T: u32 = 1;
            const KIND: u32 = 2;
            const BLEN: u32 = 3;
            let mut f = Function::new([(1, rt(false, T_TYPED)), (2, ValType::I32)]);
            let mut s = f.instructions();
            t(&mut s, 0);
            s.local_set(T);
            s.local_get(T)
                .struct_get(T_TYPED, F_BUF)
                .struct_get(T_BUFFER, F_DETACHED);
            s.if_(BlockType::Result(ValType::I32));
            s.i32_const(1);
            s.else_();
            out_of_bounds(&mut s, T, KIND, BLEN);
            s.end();
            s.end();
            f
        }
        TypedOp::BufferNewResizable => {
            // (len, max)
            let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
                Builtin::ArrayBuffer,
            )));
            let mut f = Function::new([]);
            let mut s = f.instructions();
            object_fields(&mut s, proto);
            s.local_get(0).array_new_default(T_BYTES);
            s.local_get(1).i32_const(0);
            s.struct_new(T_BUFFER);
            s.end();
            f
        }
        TypedOp::BufferMax => {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            s.block(BlockType::Result(jsval()));
            b(&mut s, 0);
            s.struct_get(T_BUFFER, F_MAX).br_on_non_null(0);
            s.global_get(runtime::G_UNDEFINED);
            s.end();
            s.end();
            f
        }
        TypedOp::Tracking => {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_TRACKING);
            s.end();
            f
        }
        TypedOp::BufferDetached => {
            let mut f = Function::new([]);
            let mut s = f.instructions();
            b(&mut s, 0);
            s.struct_get(T_BUFFER, F_DETACHED);
            s.end();
            f
        }
        TypedOp::BufferResize => {
            // (b, len): new bytes, the old ones copied (as many as fit).
            const NEW: u32 = 2;
            let mut f = Function::new([(1, rt(false, T_BYTES))]);
            let mut s = f.instructions();
            s.local_get(1).array_new_default(T_BYTES).local_set(NEW);
            copy_prefix(&mut s, NEW, 0, 1);
            b(&mut s, 0);
            s.local_get(NEW).struct_set(T_BUFFER, F_DATA);
            s.end();
            f
        }
        TypedOp::Latin1 => {
            // (t, start, end, mask): one unit per byte, from the buffer's data at the view's offset.
            const T: u32 = 0;
            const START: u32 = 1;
            const END: u32 = 2;
            const MASK: u32 = 3;
            const R: u32 = 4;
            const DATA: u32 = 5;
            const AT: u32 = 6;
            const I: u32 = 7;
            let mut f = Function::new([
                (1, rt(true, runtime::T_STRING)),
                (1, rt(true, T_BYTES)),
                (2, ValType::I32),
            ]);
            let mut s = f.instructions();
            s.local_get(END).local_get(START).i32_sub();
            s.array_new_default(runtime::T_STRING).local_set(R);
            t(&mut s, T);
            s.struct_get(T_TYPED, F_BUF).struct_get(T_BUFFER, F_DATA).local_set(DATA);
            t(&mut s, T);
            s.struct_get(T_TYPED, F_OFFSET).local_get(START).i32_add().local_set(AT);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(I).local_get(END).local_get(START).i32_sub().i32_ge_s().br_if(1);
            s.local_get(R).local_get(I);
            s.local_get(DATA).local_get(AT).local_get(I).i32_add().array_get_u(T_BYTES);
            s.local_get(MASK).i32_and();
            s.array_set(runtime::T_STRING);
            s.local_get(I).i32_const(1).i32_add().local_set(I);
            s.br(0);
            s.end().end();
            s.local_get(R).ref_as_non_null();
            s.end();
            f
        }
        TypedOp::Utf8Length => {
            // (s): a pass over the units, 1/2/3 bytes each, 4 per surrogate pair.
            const S: u32 = 1;
            const N: u32 = 2;
            const I: u32 = 3;
            const R: u32 = 4;
            const C: u32 = 5;
            const SZ: u32 = 6;
            let mut f = Function::new([(1, rt(true, runtime::T_STRING)), (5, ValType::I32)]);
            let mut s = f.instructions();
            s.local_get(0).ref_cast_non_null(runtime::string()).local_tee(S);
            s.ref_as_non_null().array_len().local_set(N);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(I).local_get(N).i32_ge_u().br_if(1);
            utf8_code_point(&mut s, S, N, I, C);
            utf8_size(&mut s, C, SZ);
            s.local_get(R).local_get(SZ).i32_add().local_set(R);
            s.local_get(I).i32_const(1).i32_add().local_set(I);
            s.br(0);
            s.end().end();
            s.local_get(R);
            s.end();
            f
        }
        TypedOp::Utf8Write => {
            // (t, s, offset, limit): whole characters while they fit.
            const LIMIT: u32 = 3;
            const S: u32 = 4;
            const DATA: u32 = 5;
            const N: u32 = 6;
            const I: u32 = 7;
            const AT: u32 = 8;
            const P: u32 = 9;
            const END: u32 = 10;
            const C: u32 = 11;
            const SZ: u32 = 12;
            let mut f = Function::new([
                (1, rt(true, runtime::T_STRING)),
                (1, rt(true, T_BYTES)),
                (7, ValType::I32),
            ]);
            let mut s = f.instructions();
            s.local_get(1).ref_cast_non_null(runtime::string()).local_tee(S);
            s.ref_as_non_null().array_len().local_set(N);
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_BUF).struct_get(T_BUFFER, F_DATA).local_set(DATA);
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_OFFSET).local_get(2).i32_add().local_tee(AT).local_tee(P);
            s.local_get(LIMIT).i32_add().local_set(END);
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(I).local_get(N).i32_ge_u().br_if(1);
            utf8_code_point(&mut s, S, N, I, C);
            utf8_size(&mut s, C, SZ);
            s.local_get(P).local_get(SZ).i32_add().local_get(END).i32_gt_u().br_if(1);
            // The lead byte: C itself, or the marker bits and C's top bits.
            let byte = |s: &mut InstructionSink, k: i32, shift: i32, bits: i32| {
                s.local_get(DATA).local_get(P);
                if k > 0 {
                    s.i32_const(k).i32_add();
                }
                s.local_get(C);
                if shift > 0 {
                    s.i32_const(shift).i32_shr_u();
                }
                s.i32_const(0x3F).i32_and().i32_const(bits).i32_or();
                s.array_set(T_BYTES);
            };
            s.local_get(SZ).i32_const(1).i32_eq();
            s.if_(BlockType::Empty);
            s.local_get(DATA).local_get(P).local_get(C).array_set(T_BYTES);
            s.else_();
            s.local_get(SZ).i32_const(2).i32_eq();
            s.if_(BlockType::Empty);
            byte(&mut s, 0, 6, 0xC0);
            byte(&mut s, 1, 0, 0x80);
            s.else_();
            s.local_get(SZ).i32_const(3).i32_eq();
            s.if_(BlockType::Empty);
            // 0xE0 | C >> 12 (C < 0x10000: no bits above the low 4 are set).
            byte(&mut s, 0, 12, 0xE0);
            byte(&mut s, 1, 6, 0x80);
            byte(&mut s, 2, 0, 0x80);
            s.else_();
            byte(&mut s, 0, 18, 0xF0);
            byte(&mut s, 1, 12, 0x80);
            byte(&mut s, 2, 6, 0x80);
            byte(&mut s, 3, 0, 0x80);
            s.end().end().end();
            s.local_get(P).local_get(SZ).i32_add().local_set(P);
            s.local_get(I).i32_const(1).i32_add().local_set(I);
            s.br(0);
            s.end().end();
            s.local_get(P).local_get(AT).i32_sub();
            s.end();
            f
        }
        TypedOp::Utf8Slice => {
            // (t, start, end): the WHATWG decoder into units (never more units than bytes), then
            // cut to the units written.
            const R: u32 = 3;
            const DATA: u32 = 4;
            const P: u32 = 5;
            const E: u32 = 6;
            const K: u32 = 7;
            const B: u32 = 8;
            const NEED: u32 = 9;
            const CP: u32 = 10;
            const LO: u32 = 11;
            const HI: u32 = 12;
            const J: u32 = 13;
            const OK: u32 = 14;
            const NEW: u32 = 15;
            let mut f = Function::new([
                (1, rt(true, runtime::T_STRING)),
                (1, rt(true, T_BYTES)),
                (10, ValType::I32),
                (1, rt(true, runtime::T_STRING)),
            ]);
            let mut s = f.instructions();
            s.local_get(2).local_get(1).i32_sub();
            s.array_new_default(runtime::T_STRING).local_set(R);
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_BUF).struct_get(T_BUFFER, F_DATA).local_set(DATA);
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_OFFSET).local_tee(E).local_get(1).i32_add().local_set(P);
            s.local_get(E).local_get(2).i32_add().local_set(E);
            let push = |s: &mut InstructionSink, unit: &dyn Fn(&mut InstructionSink)| {
                s.local_get(R).local_get(K);
                unit(s);
                s.array_set(runtime::T_STRING);
                s.local_get(K).i32_const(1).i32_add().local_set(K);
            };
            let in_range = |s: &mut InstructionSink, lo: i32, hi: i32| {
                s.local_get(B).i32_const(lo).i32_sub().i32_const(hi - lo + 1).i32_lt_u();
            };
            // LO = B == at ? lo : 0x80 (HI likewise).
            let bound = |s: &mut InstructionSink, local: u32, at: i32, v: i32, default: i32| {
                s.i32_const(v).i32_const(default);
                s.local_get(B).i32_const(at).i32_eq().select().local_set(local);
            };
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(P).local_get(E).i32_ge_u().br_if(1);
            s.local_get(DATA).local_get(P).array_get_u(T_BYTES).local_tee(B);
            s.i32_const(0x80).i32_lt_u();
            s.if_(BlockType::Empty);
            push(&mut s, &|s| {
                s.local_get(B);
            });
            s.local_get(P).i32_const(1).i32_add().local_set(P);
            s.else_();
            s.i32_const(1).local_set(OK);
            s.i32_const(0x80).local_set(LO);
            s.i32_const(0xBF).local_set(HI);
            in_range(&mut s, 0xC2, 0xDF);
            s.if_(BlockType::Empty);
            s.i32_const(1).local_set(NEED);
            s.local_get(B).i32_const(0x1F).i32_and().local_set(CP);
            s.else_();
            in_range(&mut s, 0xE0, 0xEF);
            s.if_(BlockType::Empty);
            s.i32_const(2).local_set(NEED);
            s.local_get(B).i32_const(0xF).i32_and().local_set(CP);
            bound(&mut s, LO, 0xE0, 0xA0, 0x80);
            bound(&mut s, HI, 0xED, 0x9F, 0xBF);
            s.else_();
            in_range(&mut s, 0xF0, 0xF4);
            s.if_(BlockType::Empty);
            s.i32_const(3).local_set(NEED);
            s.local_get(B).i32_const(0x7).i32_and().local_set(CP);
            bound(&mut s, LO, 0xF0, 0x90, 0x80);
            bound(&mut s, HI, 0xF4, 0x8F, 0xBF);
            s.else_();
            s.i32_const(0).local_set(NEED);
            s.i32_const(0).local_set(OK);
            s.end().end().end();
            s.local_get(P).i32_const(1).i32_add().local_set(J);
            // The continuation bytes: stop at the first one out of [LO, HI] (not consumed).
            s.block(BlockType::Empty).loop_(BlockType::Empty);
            s.local_get(NEED).i32_eqz().br_if(1);
            s.local_get(J).local_get(E).i32_ge_u();
            s.if_(BlockType::Empty);
            s.i32_const(0).local_set(OK).br(2);
            s.end();
            s.local_get(DATA).local_get(J).array_get_u(T_BYTES).local_tee(B);
            s.local_get(LO).i32_lt_u();
            s.local_get(B).local_get(HI).i32_gt_u().i32_or();
            s.if_(BlockType::Empty);
            s.i32_const(0).local_set(OK).br(2);
            s.end();
            s.i32_const(0x80).local_set(LO);
            s.i32_const(0xBF).local_set(HI);
            s.local_get(CP).i32_const(6).i32_shl();
            s.local_get(B).i32_const(0x3F).i32_and().i32_or().local_set(CP);
            s.local_get(NEED).i32_const(1).i32_sub().local_set(NEED);
            s.local_get(J).i32_const(1).i32_add().local_set(J);
            s.br(0);
            s.end().end();
            s.local_get(OK);
            s.if_(BlockType::Empty);
            s.local_get(CP).i32_const(0xFFFF).i32_gt_u();
            s.if_(BlockType::Empty);
            s.local_get(CP).i32_const(0x10000).i32_sub().local_set(CP);
            push(&mut s, &|s| {
                s.local_get(CP).i32_const(10).i32_shr_u().i32_const(0xD800).i32_or();
            });
            s.local_get(CP).i32_const(0x3FF).i32_and().i32_const(0xDC00).i32_or().local_set(CP);
            s.end();
            push(&mut s, &|s| {
                s.local_get(CP);
            });
            s.else_();
            push(&mut s, &|s| {
                s.i32_const(0xFFFD);
            });
            s.end();
            s.local_get(J).local_set(P);
            s.end();
            s.br(0);
            s.end().end();
            // Cut to K units.
            s.local_get(K).local_get(R).ref_as_non_null().array_len().i32_eq();
            s.if_(BlockType::Empty);
            s.local_get(R).ref_as_non_null().return_();
            s.end();
            s.local_get(K).array_new_default(runtime::T_STRING).local_tee(NEW);
            s.i32_const(0).local_get(R).i32_const(0).local_get(K);
            s.array_copy(runtime::T_STRING, runtime::T_STRING);
            s.local_get(NEW).ref_as_non_null();
            s.end();
            f
        }
        TypedOp::BufferTransfer => {
            // (b, len, resizable): ArrayBufferCopyAndDetach.
            const NEW: u32 = 3;
            let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
                Builtin::ArrayBuffer,
            )));
            let mut f = Function::new([(1, rt(false, T_BYTES))]);
            let mut s = f.instructions();
            s.local_get(1).array_new_default(T_BYTES).local_set(NEW);
            copy_prefix(&mut s, NEW, 0, 1);
            object_fields(&mut s, proto);
            s.local_get(NEW);
            s.local_get(2);
            s.if_(BlockType::Result(jsval()));
            b(&mut s, 0);
            s.struct_get(T_BUFFER, F_MAX);
            s.else_();
            s.ref_null(EQ_HEAP);
            s.end();
            s.i32_const(0);
            s.struct_new(T_BUFFER);
            b(&mut s, 0);
            s.i32_const(0).array_new_default(T_BYTES);
            s.struct_set(T_BUFFER, F_DATA);
            b(&mut s, 0);
            s.i32_const(1).struct_set(T_BUFFER, F_DETACHED);
            s.end();
            f
        }
        TypedOp::Copy => {
            // (dst, dst_index, src, src_index, count)
            const KIND: u32 = 5;
            let mut f = Function::new([(1, ValType::I32)]);
            let mut s = f.instructions();
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_KIND).local_set(KIND);
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_BUF).struct_get(T_BUFFER, F_DATA);
            t(&mut s, 0);
            s.struct_get(T_TYPED, F_OFFSET);
            s.local_get(1);
            size_of(&mut s, KIND);
            s.i32_mul().i32_add();
            t(&mut s, 2);
            s.struct_get(T_TYPED, F_BUF).struct_get(T_BUFFER, F_DATA);
            t(&mut s, 2);
            s.struct_get(T_TYPED, F_OFFSET);
            s.local_get(3);
            size_of(&mut s, KIND);
            s.i32_mul().i32_add();
            s.local_get(4);
            size_of(&mut s, KIND);
            s.i32_mul();
            s.array_copy(T_BYTES, T_BYTES);
            s.end();
            f
        }
    }
}

/// T80: the code point at unit `i` of string `str` (`n` units) into local `c`: a surrogate pair's
/// (then `i` is at its second unit), U+FFFD for an unpaired surrogate.
fn utf8_code_point(s: &mut InstructionSink, str: u32, n: u32, i: u32, c: u32) {
    s.local_get(str).local_get(i).array_get_u(runtime::T_STRING).local_tee(c);
    s.i32_const(0xF800).i32_and().i32_const(0xD800).i32_eq();
    s.if_(BlockType::Empty);
    // A lead followed by a trail: a pair.
    s.local_get(c).i32_const(0xDC00).i32_lt_u();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(i).i32_const(1).i32_add().local_get(n).i32_lt_u();
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(str).local_get(i).i32_const(1).i32_add().array_get_u(runtime::T_STRING);
    s.i32_const(0xFC00).i32_and().i32_const(0xDC00).i32_eq();
    s.else_();
    s.i32_const(0);
    s.end();
    s.else_();
    s.i32_const(0);
    s.end();
    s.if_(BlockType::Empty);
    s.local_get(i).i32_const(1).i32_add().local_set(i);
    s.local_get(c).i32_const(0xD800).i32_sub().i32_const(10).i32_shl();
    s.local_get(str).local_get(i).array_get_u(runtime::T_STRING);
    s.i32_const(0xDC00).i32_sub().i32_or();
    s.i32_const(0x10000).i32_add().local_set(c);
    s.else_();
    s.i32_const(0xFFFD).local_set(c);
    s.end();
    s.end();
}

/// T80: local `sz` = the UTF-8 length of code point `c` (1 to 4).
fn utf8_size(s: &mut InstructionSink, c: u32, sz: u32) {
    s.i32_const(1);
    s.local_get(c).i32_const(0x80).i32_ge_u().i32_add();
    s.local_get(c).i32_const(0x800).i32_ge_u().i32_add();
    s.local_get(c).i32_const(0x10000).i32_ge_u().i32_add();
    s.local_set(sz);
}

/// Copy the first min(len, old length) bytes of the `$buffer` in local `b` (a jsval) into the
/// `$bytes` in local `new` (`len`: a local, the new length).
fn copy_prefix(s: &mut InstructionSink, new: u32, b: u32, len: u32) {
    s.local_get(new).i32_const(0);
    s.local_get(b).ref_cast_non_null(buffer()).struct_get(T_BUFFER, F_DATA);
    s.i32_const(0);
    s.local_get(len);
    s.local_get(b).ref_cast_non_null(buffer());
    buffer_length(s);
    s.local_get(len);
    s.local_get(b).ref_cast_non_null(buffer());
    buffer_length(s);
    s.i32_lt_u();
    s.select();
    s.array_copy(T_BYTES, T_BYTES);
}

/// node's `Uint8Array(n) [ … ]`: at most 100 elements, then `... n more items`; grouped in
/// columns like an array of numbers.
fn format(cx: &mut dyn Cx) -> Function {
    let name = th(cx, Helper::Name);
    let elem = th(cx, Helper::Get);
    let fmt = cx.helper(runtime::Helper::Fmt(inspect::Helper::Format));
    let layout = cx.helper(runtime::Helper::Fmt(inspect::Helper::Layout));
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let (lparen, rparen, lbracket, rbracket) =
        (cx.str("("), cx.str(") ["), cx.str("["), cx.str("]"));
    let (more, more_item, more_items) =
        (cx.str("... "), cx.str(" more item"), cx.str(" more items"));
    const T: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const LEN: u32 = 3;
    const N: u32 = 4;
    const I: u32 = 5;
    const OUT: u32 = 6;
    let length = Length::new(cx);
    let mut f = Function::new([(3, ValType::I32), (1, rt(true, T_KEYS))]);
    let mut s = f.instructions();
    s.local_get(T);
    length.emit(&mut s);
    s.local_set(LEN);
    // Too deep: `[Uint8Array]` (node: only when there is something to show).
    s.local_get(DEPTH).i32_const(2).i32_gt_s();
    s.local_get(LEN).i32_const(0).i32_ne();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.global_get(lbracket);
    s.local_get(T)
        .struct_get(T_TYPED, F_KIND)
        .call(name)
        .call(concat);
    s.global_get(rbracket).call(concat).return_();
    s.end();
    s.local_get(LEN)
        .i32_const(100)
        .local_get(LEN)
        .i32_const(100)
        .i32_lt_u()
        .select()
        .local_set(N);
    s.local_get(N)
        .local_get(LEN)
        .local_get(N)
        .i32_gt_u()
        .i32_add()
        .array_new_default(T_KEYS)
        .local_set(OUT);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT).ref_as_non_null().local_get(I);
    s.local_get(T).local_get(I).call(elem);
    s.local_get(DEPTH).i32_const(1).i32_add();
    s.local_get(INDENT).i32_const(2).i32_add();
    s.i32_const(1).call(fmt);
    s.array_set(T_KEYS);
    s.local_get(I).i32_const(1).i32_add().local_set(I);
    s.br(0).end().end();
    s.local_get(LEN).local_get(N).i32_gt_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(OUT).ref_as_non_null().local_get(N);
        s.global_get(more);
        s.local_get(LEN)
            .local_get(N)
            .i32_sub()
            .f64_convert_i32_u()
            .call(number_to_string);
        s.call(concat);
        s.global_get(more_items).global_get(more_item);
        s.local_get(LEN)
            .local_get(N)
            .i32_sub()
            .i32_const(1)
            .i32_gt_u();
        s.typed_select(string_ref());
        s.call(concat);
        s.array_set(T_KEYS);
        s.local_get(N).i32_const(1).i32_add().local_set(N);
    }
    s.end();
    s.local_get(OUT)
        .ref_as_non_null()
        .local_get(N)
        .local_get(INDENT);
    s.local_get(T).struct_get(T_TYPED, F_KIND).call(name);
    s.global_get(lparen).call(concat);
    s.local_get(LEN)
        .f64_convert_i32_u()
        .call(number_to_string)
        .call(concat);
    s.global_get(rparen).call(concat);
    s.global_get(rbracket);
    s.i32_const(1).i32_const(1).call(layout);
    s.end();
    f
}

/// node's `ArrayBuffer { [Uint8Contents]: <01 02>, [byteLength]: 2 }` (at most 100 bytes, then
/// ` ... n more bytes`).
fn format_buffer(cx: &mut dyn Cx) -> Function {
    let layout = cx.helper(runtime::Helper::Fmt(inspect::Helper::Layout));
    let concat = cx.helper(runtime::Helper::Concat);
    let number_to_string = cx.helper(runtime::Helper::Num(crate::numstr::Helper::NumberToString));
    let (contents, gt, byte_length, open, close, deep) = (
        cx.str("[Uint8Contents]: <"),
        cx.str(">"),
        cx.str("[byteLength]: "),
        cx.str("ArrayBuffer {"),
        cx.str("}"),
        cx.str("[ArrayBuffer]"),
    );
    let (more, more_byte, more_bytes) =
        (cx.str(" ... "), cx.str(" more byte"), cx.str(" more bytes"));
    const B: u32 = 0;
    const DEPTH: u32 = 1;
    const INDENT: u32 = 2;
    const LEN: u32 = 3;
    const N: u32 = 4;
    const I: u32 = 5;
    const HEX: u32 = 6;
    const D: u32 = 7;
    let mut f = Function::new([
        (3, ValType::I32),
        (1, rt(true, runtime::T_STRING)),
        (1, ValType::I32),
    ]);
    let mut s = f.instructions();
    s.local_get(DEPTH).i32_const(2).i32_gt_s();
    s.if_(BlockType::Empty);
    s.global_get(deep).return_();
    s.end();
    if cx.rab() {
        // T47.
        let detached = cx.str("ArrayBuffer { (detached), [byteLength]: 0 }");
        s.local_get(B).struct_get(T_BUFFER, F_DETACHED);
        s.if_(BlockType::Empty);
        s.global_get(detached).return_();
        s.end();
    }
    s.local_get(B)
        .struct_get(T_BUFFER, F_DATA)
        .array_len()
        .local_set(LEN);
    s.local_get(LEN)
        .i32_const(100)
        .local_get(LEN)
        .i32_const(100)
        .i32_lt_u()
        .select()
        .local_set(N);
    // "hh hh hh": 3 units per byte, less the last space.
    s.i32_const(0);
    s.local_get(N).i32_const(3).i32_mul().i32_const(1).i32_sub();
    s.local_get(N).i32_eqz().select();
    s.array_new_default(runtime::T_STRING).local_set(HEX);
    let digit = |s: &mut InstructionSink| {
        // A nibble in local D → its lowercase hex digit.
        s.local_get(D).i32_const(48).i32_add();
        s.local_get(D).i32_const(87).i32_add();
        s.local_get(D).i32_const(10).i32_lt_u();
        s.select();
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        s.local_get(I).local_get(N).i32_ge_u().br_if(1);
        s.local_get(HEX).ref_as_non_null();
        s.local_get(I).i32_const(3).i32_mul();
        s.local_get(B)
            .struct_get(T_BUFFER, F_DATA)
            .local_get(I)
            .array_get_u(T_BYTES)
            .i32_const(4)
            .i32_shr_u()
            .local_set(D);
        digit(&mut s);
        s.array_set(runtime::T_STRING);
        s.local_get(HEX).ref_as_non_null();
        s.local_get(I).i32_const(3).i32_mul().i32_const(1).i32_add();
        s.local_get(B)
            .struct_get(T_BUFFER, F_DATA)
            .local_get(I)
            .array_get_u(T_BYTES)
            .i32_const(15)
            .i32_and()
            .local_set(D);
        digit(&mut s);
        s.array_set(runtime::T_STRING);
        s.local_get(I)
            .i32_const(1)
            .i32_add()
            .local_get(N)
            .i32_lt_u();
        s.if_(BlockType::Empty);
        s.local_get(HEX).ref_as_non_null();
        s.local_get(I).i32_const(3).i32_mul().i32_const(2).i32_add();
        s.i32_const(32);
        s.array_set(runtime::T_STRING);
        s.end();
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0);
    }
    s.end().end();
    s.global_get(contents);
    s.local_get(HEX)
        .ref_as_non_null()
        .call(concat)
        .local_set(HEX);
    s.local_get(LEN).local_get(N).i32_gt_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(HEX).ref_as_non_null();
        s.global_get(more).call(concat);
        s.local_get(LEN)
            .local_get(N)
            .i32_sub()
            .f64_convert_i32_u()
            .call(number_to_string);
        s.call(concat);
        s.global_get(more_bytes).global_get(more_byte);
        s.local_get(LEN)
            .local_get(N)
            .i32_sub()
            .i32_const(1)
            .i32_gt_u();
        s.typed_select(string_ref());
        s.call(concat).local_set(HEX);
    }
    s.end();
    s.local_get(HEX).ref_as_non_null();
    s.global_get(gt).call(concat);
    s.global_get(byte_length);
    s.local_get(LEN)
        .f64_convert_i32_u()
        .call(number_to_string)
        .call(concat);
    s.array_new_fixed(T_KEYS, 2);
    s.i32_const(2).local_get(INDENT);
    s.global_get(open).global_get(close);
    s.i32_const(0).i32_const(0).call(layout);
    s.end();
    f
}
