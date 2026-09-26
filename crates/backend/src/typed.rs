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

const UINT8_CLAMPED: i32 = 2;
const FLOAT32: i32 = 7;
const FLOAT64: i32 = 8;

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
    /// `(t: (ref $typed), depth, indent) -> (ref $string)`: `Uint8Array(3) [ 1, 2, 3 ]`.
    Format,
    /// `(b: (ref $buffer), depth, indent) -> (ref $string)`: `ArrayBuffer { [Uint8Contents]:
    /// <01 02>, [byteLength]: 2 }`.
    FormatBuffer,
    /// `(kind: i32) -> (ref $string)`: the constructor name.
    Name,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Op(op) => js2wasi3_ir::Intrinsic::Typed(op).name(),
            Helper::Get => "typed_get",
            Helper::Set => "typed_set",
            Helper::Format => "inspect_typed_array",
            Helper::FormatBuffer => "inspect_array_buffer",
            Helper::Name => "typed_array_name",
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
            Helper::Format => (vec![t, I32, I32], vec![string_ref()]),
            Helper::FormatBuffer => (vec![rt(false, T_BUFFER), I32, I32], vec![string_ref()]),
            Helper::Name => (vec![I32], vec![string_ref()]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Op(op) => op_body(cx, op),
            Helper::Get => get(cx),
            Helper::Set => set(),
            Helper::Format => format(cx),
            Helper::FormatBuffer => format_buffer(cx),
            Helper::Name => {
                let names: Vec<u32> = TYPED_NAMES.iter().map(|n| cx.str(n)).collect();
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
    let mut f = Function::new([(1, ValType::I32), (1, rt(true, T_BYTES)), (1, ValType::I32)]);
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

/// Push a new `$buffer` of `len` (local) zeroed bytes.
fn new_buffer(cx: &mut dyn Cx, s: &mut InstructionSink, len: u32) {
    let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
        Builtin::ArrayBuffer,
    )));
    object_fields(s, proto);
    s.local_get(len).array_new_default(T_BYTES);
    s.struct_new(T_BUFFER);
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

/// Push a new `$typed` of the kind in local `kind` on the `$buffer` in local `buf`.
fn new_typed(cx: &mut dyn Cx, s: &mut InstructionSink, kind: u32, buf: u32, off: u32, len: u32) {
    let protos: Vec<u32> = (0..9)
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
    s.local_get(off).local_get(len);
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
            new_typed(cx, &mut s, KIND, BUF, ZERO, LEN);
            s.end();
            f
        }
        TypedOp::View => {
            // (kind, b, offset, length)
            let mut f = Function::new([]);
            let mut s = f.instructions();
            new_typed(cx, &mut s, 0, 1, 2, 3);
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
        TypedOp::Length | TypedOp::ByteOffset | TypedOp::Buffer => {
            let field = match op {
                TypedOp::Length => F_LENGTH,
                TypedOp::ByteOffset => F_OFFSET,
                _ => F_BUF,
            };
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
    let mut f = Function::new([(3, ValType::I32), (1, rt(true, T_KEYS))]);
    let mut s = f.instructions();
    s.local_get(T).struct_get(T_TYPED, F_LENGTH).local_set(LEN);
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
