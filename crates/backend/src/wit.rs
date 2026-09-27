//! Compound WIT types at the boundary of a user world (T20 slice 4b): the Canonical ABI's
//! flattening and memory layout, and the code that converts between their core values and JS
//! values (jco's shapes, see [`WitType`]).
//!
//! - [`lift_flat`]: a parameter's flat core values → a JS value.
//! - [`lower_mem`]: a JS value → the type's layout in memory (results go through a return area).
//! - [`flat_load`]: memory → flat core values (`task.return` of an async export).
//! - [`lift_mem`]: memory → a JS value (list elements, parameters past 16 flat values).
//!
//! A list's elements are in memory from `cabi_realloc` (results: the adapter's bump allocator,
//! reset by the next call). Numeric lists are typed arrays ([`WitType::typed_kind`]), copied
//! byte by byte; any other array-like (`length`, indices) lowers too.
//!
//! The generated code allocates locals as it goes ([`Gen`]), so functions using it are built
//! into a buffer first. A JS value that doesn't fit the type traps (an `enum` string that isn't
//! a case, a `result` object whose `tag` is neither `ok` nor `err`).

use js2wasi3_ir::{AbiType, TYPED_SIZES, TypedOp, WitType};
use wasm_encoder::{BlockType, InstructionSink, MemArg, ValType};

use crate::abi;
use crate::runtime::{self, Cx, G_FALSE, G_TRUE, G_UNDEFINED, T_BUFFER, T_BYTES, T_TYPED, T_VALS};
use crate::{array, object, typed};

/// Locals allocated while generating code: indices from `base`.
pub struct Gen {
    base: u32,
    pub locals: Vec<ValType>,
}

impl Gen {
    pub fn new(base: u32) -> Gen {
        Gen {
            base,
            locals: Vec::new(),
        }
    }

    pub fn local(&mut self, t: ValType) -> u32 {
        self.locals.push(t);
        self.base + self.locals.len() as u32 - 1
    }

    /// The local declarations, for `Function::new` (runs of one type together).
    pub fn decls(&self) -> Vec<(u32, ValType)> {
        let mut out: Vec<(u32, ValType)> = Vec::new();
        for t in &self.locals {
            match out.last_mut() {
                Some((n, u)) if u == t => *n += 1,
                _ => out.push((1, *t)),
            }
        }
        out
    }
}

/// The Canonical ABI flattening of `t`. The cases of a variant share their values, joined.
pub fn flatten(t: &WitType) -> Vec<ValType> {
    match t {
        WitType::Scalar(a) => abi::flat(*a),
        WitType::Option(t) => {
            let mut v = vec![ValType::I32];
            v.extend(flatten(t));
            v
        }
        WitType::Result { ok, err } => {
            let mut v = vec![ValType::I32];
            let ok = ok.as_deref().map_or(Vec::new(), flatten);
            let err = err.as_deref().map_or(Vec::new(), flatten);
            v.extend(join(&ok, &err));
            v
        }
        WitType::Tuple(ts) => ts.iter().flat_map(flatten).collect(),
        WitType::Record(fs) => fs.iter().flat_map(|(_, t)| flatten(t)).collect(),
        WitType::Enum(_) => vec![ValType::I32],
        WitType::Flags(fs) if fs.is_empty() => Vec::new(),
        WitType::Flags(_) => vec![ValType::I32],
        WitType::Variant(cs) => {
            let mut joined = Vec::new();
            for (_, t) in cs {
                if let Some(t) = t {
                    joined = join(&joined, &flatten(t));
                }
            }
            let mut v = vec![ValType::I32];
            v.extend(joined);
            v
        }
        WitType::List(_) => vec![ValType::I32, ValType::I32],
        WitType::Future(_) => vec![ValType::I32],
    }
}

/// The payload types of a variant's cases.
fn payloads(cs: &[(String, Option<WitType>)]) -> Vec<Option<&WitType>> {
    cs.iter().map(|(_, t)| t.as_ref()).collect()
}

fn join(a: &[ValType], b: &[ValType]) -> Vec<ValType> {
    (0..a.len().max(b.len()))
        .map(|i| match (a.get(i), b.get(i)) {
            (Some(x), None) | (None, Some(x)) => *x,
            (Some(x), Some(y)) if x == y => *x,
            (Some(ValType::I32 | ValType::F32), Some(ValType::I32 | ValType::F32)) => ValType::I32,
            _ => ValType::I64,
        })
        .collect()
}

fn align_to(x: u64, a: u64) -> u64 {
    x.div_ceil(a) * a
}

/// Size of a discriminant for `n` cases.
fn disc_size(n: usize) -> u64 {
    match n {
        0..=256 => 1,
        257..=65536 => 2,
        _ => 4,
    }
}

/// Size and alignment in memory.
pub fn size_align(t: &WitType) -> (u64, u64) {
    match t {
        WitType::Scalar(a) => abi::size_align(*a),
        WitType::Option(t) => variant_layout(2, &[None, Some(t)]).1,
        WitType::Result { ok, err } => variant_layout(2, &[ok.as_deref(), err.as_deref()]).1,
        WitType::Tuple(ts) => fields_layout(ts.iter()).1,
        WitType::Record(fs) => fields_layout(fs.iter().map(|(_, t)| t)).1,
        WitType::Enum(cs) => {
            let d = disc_size(cs.len());
            (d, d)
        }
        WitType::Flags(fs) => match fs.len() {
            0 => (0, 1),
            1..=8 => (1, 1),
            9..=16 => (2, 2),
            _ => (4, 4),
        },
        WitType::Variant(cs) => variant_layout(cs.len(), &payloads(cs)).1,
        WitType::List(_) => (8, 4),
        WitType::Future(_) => (4, 4),
    }
}

/// Field offsets, and the (size, alignment).
fn fields_layout<'t>(ts: impl Iterator<Item = &'t WitType>) -> (Vec<u64>, (u64, u64)) {
    let (mut off, mut align) = (0, 1);
    let mut offs = Vec::new();
    for t in ts {
        let (s, a) = size_align(t);
        off = align_to(off, a);
        offs.push(off);
        off += s;
        align = align.max(a);
    }
    (offs, (align_to(off, align), align))
}

/// A variant's payload offset, and its (size, alignment).
pub fn variant_layout(cases: usize, payloads: &[Option<&WitType>]) -> (u64, (u64, u64)) {
    let d = disc_size(cases);
    let (mut size, mut align) = (0, d);
    for t in payloads.iter().flatten() {
        let (s, a) = size_align(t);
        size = size.max(s);
        align = align.max(a);
    }
    let off = align_to(d, align);
    (off, (align_to(off + size, align), align))
}

fn mem(offset: u64, align: u32) -> MemArg {
    MemArg {
        offset,
        align,
        memory_index: 0,
    }
}

/// Convert the value on the stack from `from` (a case's flat type) to the joined type `to`.
fn coerce_up(s: &mut InstructionSink, from: ValType, to: ValType) {
    match (from, to) {
        _ if from == to => {}
        (ValType::F32, ValType::I32) => {
            s.i32_reinterpret_f32();
        }
        (ValType::I32, ValType::I64) => {
            s.i64_extend_i32_u();
        }
        (ValType::F32, ValType::I64) => {
            s.i32_reinterpret_f32().i64_extend_i32_u();
        }
        (ValType::F64, ValType::I64) => {
            s.i64_reinterpret_f64();
        }
        _ => unreachable!("no join from {from:?} to {to:?}"),
    }
}

/// The inverse of [`coerce_up`].
fn coerce_down(s: &mut InstructionSink, from: ValType, to: ValType) {
    match (from, to) {
        _ if from == to => {}
        (ValType::I32, ValType::F32) => {
            s.f32_reinterpret_i32();
        }
        (ValType::I64, ValType::I32) => {
            s.i32_wrap_i64();
        }
        (ValType::I64, ValType::F32) => {
            s.i32_wrap_i64().f32_reinterpret_i32();
        }
        (ValType::I64, ValType::F64) => {
            s.f64_reinterpret_i64();
        }
        _ => unreachable!("no join from {to:?} to {from:?}"),
    }
}

fn rh(cx: &mut dyn Cx, h: runtime::Helper) -> u32 {
    cx.helper(h)
}

/// Push the JS value of type `t` whose flat values are in `slots` (local, its joined type).
pub fn lift_flat(
    cx: &mut dyn Cx,
    g: &mut Gen,
    s: &mut InstructionSink,
    t: &WitType,
    slots: &[(u32, ValType)],
) {
    let flat = flatten(t);
    assert_eq!(flat.len(), slots.len());
    // The i-th flat value, as its own type.
    let get = |s: &mut InstructionSink, i: usize| {
        s.local_get(slots[i].0);
        coerce_down(s, slots[i].1, flat[i]);
    };
    match t {
        WitType::Future(p) => {
            // The readable end: a promise of its value (T20 slice 6a).
            get(s, 0);
            let id = cx.payload(p);
            let lift = cx.helper(runtime::Helper::Abi(abi::Helper::FutureLift(id)));
            s.call(lift);
        }
        WitType::Scalar(a) => {
            get(s, 0);
            match a {
                AbiType::Bool => {
                    s.if_(BlockType::Result(runtime::jsval()));
                    s.global_get(G_TRUE);
                    s.else_();
                    s.global_get(G_FALSE);
                    s.end();
                    return;
                }
                AbiType::S8 => {
                    s.i32_extend8_s().f64_convert_i32_s();
                }
                AbiType::S16 => {
                    s.i32_extend16_s().f64_convert_i32_s();
                }
                AbiType::S32 => {
                    s.f64_convert_i32_s();
                }
                AbiType::U8 => {
                    s.i32_const(0xff).i32_and().f64_convert_i32_u();
                }
                AbiType::U16 => {
                    s.i32_const(0xffff).i32_and().f64_convert_i32_u();
                }
                AbiType::U32 => {
                    s.f64_convert_i32_u();
                }
                AbiType::S64 => {
                    s.f64_convert_i64_s();
                }
                AbiType::U64 => {
                    s.f64_convert_i64_u();
                }
                AbiType::F32 => {
                    s.f64_promote_f32();
                }
                AbiType::F64 => {}
                AbiType::Char => {
                    let from = cx.helper(runtime::Helper::Abi(abi::Helper::FromCodePoint));
                    s.call(from);
                    return;
                }
                AbiType::String => {
                    get(s, 1);
                    let to_string = cx.helper(runtime::Helper::Abi(abi::Helper::Utf8ToString));
                    s.call(to_string);
                    return;
                }
                AbiType::Compound(_) => unreachable!("nested types are trees"),
            }
            let boxed = rh(cx, runtime::Helper::Box);
            s.call(boxed);
        }
        WitType::Tuple(ts) => {
            let mut i = 0;
            for t in ts {
                let n = flatten(t).len();
                lift_flat(cx, g, s, t, &slots[i..i + n]);
                i += n;
            }
            new_array(cx, s, ts.len() as u32);
        }
        WitType::Record(fs) => {
            let define = cx.helper(runtime::Helper::Obj(object::Helper::DefineProp));
            let new = cx.helper(runtime::Helper::Obj(object::Helper::NewObject));
            let o = g.local(runtime::jsval());
            s.call(new).local_set(o);
            let mut i = 0;
            for (key, t) in fs {
                let n = flatten(t).len();
                let k = cx.str(key);
                s.local_get(o).global_get(k);
                lift_flat(cx, g, s, t, &slots[i..i + n]);
                s.call(define);
                i += n;
            }
            s.local_get(o);
        }
        WitType::Flags(fs) => {
            let define = cx.helper(runtime::Helper::Obj(object::Helper::DefineProp));
            let new = cx.helper(runtime::Helper::Obj(object::Helper::NewObject));
            let o = g.local(runtime::jsval());
            s.call(new).local_set(o);
            for (k, key) in fs.iter().enumerate() {
                let key = cx.str(key);
                s.local_get(o).global_get(key);
                get(s, 0);
                s.i32_const(1 << k).i32_and();
                s.if_(BlockType::Result(runtime::jsval()));
                s.global_get(G_TRUE);
                s.else_();
                s.global_get(G_FALSE);
                s.end();
                s.call(define);
            }
            s.local_get(o);
        }
        WitType::Enum(cs) => {
            let names: Vec<u32> = cs.iter().map(|c| cx.str(c)).collect();
            // A chain of `if`s on the case index; out of range traps.
            for (k, name) in names.iter().enumerate() {
                get(s, 0);
                s.i32_const(k as i32).i32_eq();
                s.if_(BlockType::Result(runtime::jsval()));
                s.global_get(*name);
                s.else_();
            }
            s.unreachable();
            for _ in &names {
                s.end();
            }
        }
        WitType::Option(t) => {
            get(s, 0);
            s.if_(BlockType::Result(runtime::jsval()));
            lift_flat(cx, g, s, t, &slots[1..]);
            s.else_();
            s.global_get(G_UNDEFINED);
            s.end();
        }
        WitType::Result { ok, err } => {
            // `{ tag: "ok" | "err", val }`, `val: undefined` without a payload.
            let cases = [("ok", ok.as_deref()), ("err", err.as_deref())];
            lift_cases(cx, g, s, &cases, true, slots);
        }
        WitType::Variant(cs) => {
            let cases: Vec<(&str, Option<&WitType>)> =
                cs.iter().map(|(n, t)| (n.as_str(), t.as_ref())).collect();
            lift_cases(cx, g, s, &cases, false, slots);
        }
        WitType::List(elem) => {
            let (ptr, len) = (slots[0].0, slots[1].0);
            let (size, _) = size_align(elem);
            let i = g.local(ValType::I32);
            if let Some(kind) = elem.typed_kind() {
                // A typed array: copy the bytes.
                let new = cx.helper(runtime::Helper::Typed(typed::Helper::Op(TypedOp::New)));
                let (a, data, n) = (
                    g.local(runtime::jsval()),
                    g.local(runtime::rt(false, T_BYTES)),
                    g.local(ValType::I32),
                );
                s.i32_const(kind as i32)
                    .local_get(len)
                    .call(new)
                    .local_tee(a);
                s.ref_cast_non_null(typed::typed())
                    .struct_get(T_TYPED, typed::F_BUF)
                    .struct_get(T_BUFFER, typed::F_DATA)
                    .local_set(data);
                s.local_get(len)
                    .i32_const(TYPED_SIZES[kind as usize] as i32)
                    .i32_mul()
                    .local_set(n);
                for_each(s, i, n, |s| {
                    s.local_get(data).local_get(i);
                    s.local_get(ptr)
                        .local_get(i)
                        .i32_add()
                        .i32_load8_u(mem(0, 0));
                    s.array_set(T_BYTES);
                });
                s.local_get(a);
                return;
            }
            let new = cx.helper(runtime::Helper::Arr(array::Helper::New));
            let (vals, ea) = (g.local(runtime::rt(false, T_VALS)), g.local(ValType::I32));
            s.local_get(len).array_new_default(T_VALS).local_set(vals);
            for_each(s, i, len, |s| {
                s.local_get(ptr)
                    .local_get(i)
                    .i32_const(size as i32)
                    .i32_mul()
                    .i32_add()
                    .local_set(ea);
                s.local_get(vals).local_get(i);
                lift_mem(cx, g, s, elem, ea, 0);
                s.array_set(T_VALS);
            });
            s.local_get(vals).local_get(len).call(new);
        }
    }
}

/// `for (i = 0; i < n; i++) body` (`i`, `n`: i32 locals).
fn for_each(s: &mut InstructionSink, i: u32, n: u32, mut body: impl FnMut(&mut InstructionSink)) {
    s.i32_const(0).local_set(i);
    s.block(BlockType::Empty);
    s.loop_(BlockType::Empty);
    s.local_get(i).local_get(n).i32_ge_u().br_if(1);
    body(s);
    s.local_get(i).i32_const(1).i32_add().local_set(i);
    s.br(0);
    s.end();
    s.end();
}

/// Push the `{ tag, val }` object of a variant (a nested `result` too) whose discriminant and
/// joined payload values are in `slots`. A case without a payload has `val: undefined` if
/// `always_val` (jco's `result`), else no `val` (jco's `variant`).
fn lift_cases(
    cx: &mut dyn Cx,
    g: &mut Gen,
    s: &mut InstructionSink,
    cases: &[(&str, Option<&WitType>)],
    always_val: bool,
    slots: &[(u32, ValType)],
) {
    let define = cx.helper(runtime::Helper::Obj(object::Helper::DefineProp));
    let new = cx.helper(runtime::Helper::Obj(object::Helper::NewObject));
    let (tag, val) = (cx.str("tag"), cx.str("val"));
    let o = g.local(runtime::jsval());
    s.call(new).local_set(o);
    // A chain of `if`s on the case index; out of range traps.
    for (k, (name, t)) in cases.iter().enumerate() {
        s.local_get(slots[0].0).i32_const(k as i32).i32_eq();
        s.if_(BlockType::Empty);
        let name = cx.str(name);
        s.local_get(o).global_get(tag).global_get(name).call(define);
        match t {
            Some(t) => {
                let n = flatten(t).len();
                s.local_get(o).global_get(val);
                lift_flat(cx, g, s, t, &slots[1..1 + n]);
                s.call(define);
            }
            None if always_val => {
                s.local_get(o)
                    .global_get(val)
                    .global_get(G_UNDEFINED)
                    .call(define);
            }
            None => {}
        }
        s.else_();
    }
    s.unreachable();
    for _ in cases {
        s.end();
    }
    s.local_get(o);
}

/// Push the JS value of the `t` stored at `addr + off`.
pub fn lift_mem(
    cx: &mut dyn Cx,
    g: &mut Gen,
    s: &mut InstructionSink,
    t: &WitType,
    addr: u32,
    off: u64,
) {
    let flat = flatten(t);
    flat_load(g, s, t, addr, off);
    let slots: Vec<(u32, ValType)> = flat.iter().map(|v| (g.local(*v), *v)).collect();
    for (l, _) in slots.iter().rev() {
        s.local_set(*l);
    }
    lift_flat(cx, g, s, t, &slots);
}

/// A new array of the `n` values on the stack.
fn new_array(cx: &mut dyn Cx, s: &mut InstructionSink, n: u32) {
    let new = cx.helper(runtime::Helper::Arr(array::Helper::New));
    s.array_new_fixed(T_VALS, n).i32_const(n as i32).call(new);
}

/// Store the JS value in local `v` as type `t` at `addr + off`.
pub fn lower_mem(
    cx: &mut dyn Cx,
    g: &mut Gen,
    s: &mut InstructionSink,
    t: &WitType,
    v: u32,
    addr: u32,
    off: u64,
) {
    let get = cx.helper(runtime::Helper::Obj(object::Helper::Get));
    match t {
        WitType::Future(p) => {
            // A new future, written when the value settles (T20 slice 6a); its readable end.
            let id = cx.payload(p);
            let lower = cx.helper(runtime::Helper::Abi(abi::Helper::FutureLower(id)));
            s.local_get(addr).local_get(v).call(lower);
            s.i32_store(mem(off, 2));
        }
        WitType::Scalar(a) => {
            s.local_get(addr);
            let to_number = |cx: &mut dyn Cx, s: &mut InstructionSink| {
                let n = rh(cx, runtime::Helper::ToNumber);
                s.local_get(v).call(n);
            };
            let to_int32 = |cx: &mut dyn Cx, s: &mut InstructionSink| {
                to_number(cx, s);
                let i = cx.to_int32();
                s.call(i);
            };
            match a {
                AbiType::Bool => {
                    let b = rh(cx, runtime::Helper::ToBoolean);
                    s.local_get(v).call(b).i32_store8(mem(off, 0));
                }
                AbiType::S8 | AbiType::U8 => {
                    to_int32(cx, s);
                    s.i32_store8(mem(off, 0));
                }
                AbiType::S16 | AbiType::U16 => {
                    to_int32(cx, s);
                    s.i32_store16(mem(off, 1));
                }
                AbiType::S32 | AbiType::U32 => {
                    to_int32(cx, s);
                    s.i32_store(mem(off, 2));
                }
                AbiType::S64 => {
                    to_number(cx, s);
                    s.i64_trunc_sat_f64_s().i64_store(mem(off, 3));
                }
                AbiType::U64 => {
                    to_number(cx, s);
                    s.i64_trunc_sat_f64_u().i64_store(mem(off, 3));
                }
                AbiType::F32 => {
                    to_number(cx, s);
                    s.f32_demote_f64().f32_store(mem(off, 2));
                }
                AbiType::F64 => {
                    to_number(cx, s);
                    s.f64_store(mem(off, 3));
                }
                AbiType::Char => {
                    let to_string = rh(cx, runtime::Helper::ToString);
                    let cp = cx.helper(runtime::Helper::Abi(abi::Helper::CodePointAt0));
                    s.local_get(v).call(to_string).call(cp);
                    s.i32_store(mem(off, 2));
                }
                AbiType::String => {
                    s.drop();
                    let to_string = rh(cx, runtime::Helper::ToString);
                    let to_utf8 = cx.helper(runtime::Helper::Abi(abi::Helper::StringToUtf8));
                    let (ptr, len) = (g.local(ValType::I32), g.local(ValType::I32));
                    s.local_get(v).call(to_string).call(to_utf8);
                    s.local_set(len).local_set(ptr);
                    s.local_get(addr).local_get(ptr).i32_store(mem(off, 2));
                    s.local_get(addr).local_get(len).i32_store(mem(off + 4, 2));
                }
                AbiType::Compound(_) => unreachable!("nested types are trees"),
            }
        }
        WitType::Tuple(ts) => {
            let (offs, _) = fields_layout(ts.iter());
            let boxed = rh(cx, runtime::Helper::Box);
            for (i, (t, o)) in ts.iter().zip(offs).enumerate() {
                let x = g.local(runtime::jsval());
                s.local_get(v).f64_const((i as f64).into()).call(boxed);
                s.call(get).local_set(x);
                lower_mem(cx, g, s, t, x, addr, off + o);
            }
        }
        WitType::Record(fs) => {
            let (offs, _) = fields_layout(fs.iter().map(|(_, t)| t));
            for ((key, t), o) in fs.iter().zip(offs) {
                let x = g.local(runtime::jsval());
                let k = cx.str(key);
                s.local_get(v).global_get(k).call(get).local_set(x);
                lower_mem(cx, g, s, t, x, addr, off + o);
            }
        }
        WitType::Flags(fs) => {
            if fs.is_empty() {
                return;
            }
            let b = rh(cx, runtime::Helper::ToBoolean);
            s.local_get(addr);
            s.i32_const(0);
            for (k, key) in fs.iter().enumerate() {
                let key = cx.str(key);
                s.local_get(v).global_get(key).call(get).call(b);
                s.i32_const(k as i32).i32_shl().i32_or();
            }
            store_disc(s, size_align(t).0, off);
        }
        WitType::Enum(cs) => {
            let eq = rh(cx, runtime::Helper::StrictEq);
            let size = disc_size(cs.len());
            s.block(BlockType::Empty);
            for (k, c) in cs.iter().enumerate() {
                let c = cx.str(c);
                s.local_get(v).global_get(c).call(eq);
                s.if_(BlockType::Empty);
                s.local_get(addr).i32_const(k as i32);
                store_disc(s, size, off);
                s.br(1);
                s.end();
            }
            s.unreachable();
            s.end();
        }
        WitType::Option(t) => {
            let nullish = rh(cx, runtime::Helper::IsNullish);
            let (payload, _) = variant_layout(2, &[None, Some(t)]);
            s.local_get(v).call(nullish);
            s.if_(BlockType::Empty);
            s.local_get(addr).i32_const(0).i32_store8(mem(off, 0));
            s.else_();
            s.local_get(addr).i32_const(1).i32_store8(mem(off, 0));
            lower_mem(cx, g, s, t, v, addr, off + payload);
            s.end();
        }
        WitType::Result { ok, err } => {
            let cases = [("ok", ok.as_deref()), ("err", err.as_deref())];
            lower_cases(cx, g, s, &cases, v, addr, off);
        }
        WitType::Variant(cs) => {
            let cases: Vec<(&str, Option<&WitType>)> =
                cs.iter().map(|(n, t)| (n.as_str(), t.as_ref())).collect();
            lower_cases(cx, g, s, &cases, v, addr, off);
        }
        WitType::List(elem) => {
            let realloc = cx.helper(runtime::Helper::Abi(abi::Helper::Realloc));
            let (size, align) = size_align(elem);
            let (p, len, i) = (
                g.local(ValType::I32),
                g.local(ValType::I32),
                g.local(ValType::I32),
            );
            let alloc = |s: &mut InstructionSink, bytes: &dyn Fn(&mut InstructionSink)| {
                s.i32_const(0).i32_const(0).i32_const(align as i32);
                bytes(s);
                s.call(realloc).local_set(p);
            };
            s.block(BlockType::Empty);
            if let Some(kind) = elem.typed_kind() {
                // A typed array of this kind: copy its bytes.
                let (tv, data, base, n) = (
                    g.local(runtime::rt(false, T_TYPED)),
                    g.local(runtime::rt(false, T_BYTES)),
                    g.local(ValType::I32),
                    g.local(ValType::I32),
                );
                s.local_get(v).ref_test_non_null(typed::typed());
                s.if_(BlockType::Empty);
                s.local_get(v)
                    .ref_cast_non_null(typed::typed())
                    .local_tee(tv);
                s.struct_get(T_TYPED, typed::F_KIND)
                    .i32_const(kind as i32)
                    .i32_eq();
                s.if_(BlockType::Empty);
                s.local_get(tv)
                    .struct_get(T_TYPED, typed::F_LENGTH)
                    .local_set(len);
                s.local_get(tv)
                    .struct_get(T_TYPED, typed::F_BUF)
                    .struct_get(T_BUFFER, typed::F_DATA)
                    .local_set(data);
                s.local_get(tv)
                    .struct_get(T_TYPED, typed::F_OFFSET)
                    .local_set(base);
                s.local_get(len)
                    .i32_const(size as i32)
                    .i32_mul()
                    .local_set(n);
                alloc(s, &|s| {
                    s.local_get(n);
                });
                for_each(s, i, n, |s| {
                    s.local_get(p).local_get(i).i32_add();
                    s.local_get(data)
                        .local_get(base)
                        .local_get(i)
                        .i32_add()
                        .array_get_u(T_BYTES);
                    s.i32_store8(mem(0, 0));
                });
                s.br(2);
                s.end();
                s.end();
            }
            // Any array-like: `length`, then each element.
            let boxed = rh(cx, runtime::Helper::Box);
            let to_number = rh(cx, runtime::Helper::ToNumber);
            let length = cx.str("length");
            let (x, ea) = (g.local(runtime::jsval()), g.local(ValType::I32));
            s.local_get(v).global_get(length).call(get).call(to_number);
            s.i32_trunc_sat_f64_u().local_set(len);
            alloc(s, &|s| {
                s.local_get(len).i32_const(size as i32).i32_mul();
            });
            for_each(s, i, len, |s| {
                s.local_get(v).local_get(i).f64_convert_i32_u().call(boxed);
                s.call(get).local_set(x);
                s.local_get(p)
                    .local_get(i)
                    .i32_const(size as i32)
                    .i32_mul()
                    .i32_add()
                    .local_set(ea);
                lower_mem(cx, g, s, elem, x, ea, 0);
            });
            s.end();
            s.local_get(addr).local_get(p).i32_store(mem(off, 2));
            s.local_get(addr).local_get(len).i32_store(mem(off + 4, 2));
        }
    }
}

/// [`lower_mem`] of a variant (a nested `result` too) from its `{ tag, val }` object: a `tag`
/// that isn't a case name traps.
fn lower_cases(
    cx: &mut dyn Cx,
    g: &mut Gen,
    s: &mut InstructionSink,
    cases: &[(&str, Option<&WitType>)],
    v: u32,
    addr: u32,
    off: u64,
) {
    let get = cx.helper(runtime::Helper::Obj(object::Helper::Get));
    let eq = rh(cx, runtime::Helper::StrictEq);
    let (tag, val) = (cx.str("tag"), cx.str("val"));
    let ts: Vec<Option<&WitType>> = cases.iter().map(|(_, t)| *t).collect();
    let (payload, _) = variant_layout(cases.len(), &ts);
    let size = disc_size(cases.len());
    let (tg, x) = (g.local(runtime::jsval()), g.local(runtime::jsval()));
    s.local_get(v).global_get(tag).call(get).local_set(tg);
    s.local_get(v).global_get(val).call(get).local_set(x);
    for (k, (name, t)) in cases.iter().enumerate() {
        let name = cx.str(name);
        s.local_get(tg).global_get(name).call(eq);
        s.if_(BlockType::Empty);
        s.local_get(addr).i32_const(k as i32);
        store_disc(s, size, off);
        if let Some(t) = t {
            lower_mem(cx, g, s, t, x, addr, off + payload);
        }
        s.else_();
    }
    s.unreachable();
    for _ in cases {
        s.end();
    }
}

/// Store the i32 on the stack (after the address) as a `size`-byte value.
fn store_disc(s: &mut InstructionSink, size: u64, off: u64) {
    match size {
        1 => s.i32_store8(mem(off, 0)),
        2 => s.i32_store16(mem(off, 1)),
        _ => s.i32_store(mem(off, 2)),
    };
}

/// Load a `size`-byte unsigned value from `addr + off`.
fn load_disc(s: &mut InstructionSink, addr: u32, size: u64, off: u64) {
    s.local_get(addr);
    match size {
        1 => s.i32_load8_u(mem(off, 0)),
        2 => s.i32_load16_u(mem(off, 1)),
        _ => s.i32_load(mem(off, 2)),
    };
}

/// Push the flat values of the `t` stored at `addr + off`.
pub fn flat_load(g: &mut Gen, s: &mut InstructionSink, t: &WitType, addr: u32, off: u64) {
    match t {
        WitType::Scalar(a) => {
            s.local_get(addr);
            match a {
                AbiType::Bool | AbiType::U8 => {
                    s.i32_load8_u(mem(off, 0));
                }
                AbiType::S8 => {
                    s.i32_load8_s(mem(off, 0));
                }
                AbiType::U16 => {
                    s.i32_load16_u(mem(off, 1));
                }
                AbiType::S16 => {
                    s.i32_load16_s(mem(off, 1));
                }
                AbiType::S32 | AbiType::U32 | AbiType::Char => {
                    s.i32_load(mem(off, 2));
                }
                AbiType::S64 | AbiType::U64 => {
                    s.i64_load(mem(off, 3));
                }
                AbiType::F32 => {
                    s.f32_load(mem(off, 2));
                }
                AbiType::F64 => {
                    s.f64_load(mem(off, 3));
                }
                AbiType::String => {
                    s.i32_load(mem(off, 2));
                    s.local_get(addr).i32_load(mem(off + 4, 2));
                }
                AbiType::Compound(_) => unreachable!("nested types are trees"),
            }
        }
        WitType::List(_) => {
            s.local_get(addr).i32_load(mem(off, 2));
            s.local_get(addr).i32_load(mem(off + 4, 2));
        }
        WitType::Future(_) => {
            s.local_get(addr).i32_load(mem(off, 2));
        }
        WitType::Variant(cs) => variant_load(g, s, &payloads(cs), addr, off),
        WitType::Tuple(ts) => {
            let (offs, _) = fields_layout(ts.iter());
            for (t, o) in ts.iter().zip(offs) {
                flat_load(g, s, t, addr, off + o);
            }
        }
        WitType::Record(fs) => {
            let (offs, _) = fields_layout(fs.iter().map(|(_, t)| t));
            for ((_, t), o) in fs.iter().zip(offs) {
                flat_load(g, s, t, addr, off + o);
            }
        }
        WitType::Flags(fs) => {
            if !fs.is_empty() {
                load_disc(s, addr, size_align(t).0, off);
            }
        }
        WitType::Enum(cs) => load_disc(s, addr, disc_size(cs.len()), off),
        WitType::Option(p) => variant_load(g, s, &[None, Some(p)], addr, off),
        WitType::Result { ok, err } => {
            variant_load(g, s, &[ok.as_deref(), err.as_deref()], addr, off)
        }
    }
}

/// [`flat_load`] of a variant with `cases`: the discriminant, then the joined values, set by
/// the case that's there (the others stay 0).
fn variant_load(
    g: &mut Gen,
    s: &mut InstructionSink,
    cases: &[Option<&WitType>],
    addr: u32,
    off: u64,
) {
    let (payload, _) = variant_layout(cases.len(), cases);
    let mut joined = Vec::new();
    for t in cases.iter().flatten() {
        joined = join(&joined, &flatten(t));
    }
    let disc = g.local(ValType::I32);
    let slots: Vec<u32> = joined.iter().map(|t| g.local(*t)).collect();
    load_disc(s, addr, disc_size(cases.len()), off);
    s.local_set(disc);
    for (k, t) in cases.iter().enumerate() {
        let Some(t) = t else { continue };
        let flat = flatten(t);
        s.local_get(disc).i32_const(k as i32).i32_eq();
        s.if_(BlockType::Empty);
        flat_load(g, s, t, addr, off + payload);
        for j in (0..flat.len()).rev() {
            coerce_up(s, flat[j], joined[j]);
            s.local_set(slots[j]);
        }
        s.end();
    }
    s.local_get(disc);
    for l in slots {
        s.local_get(l);
    }
}
