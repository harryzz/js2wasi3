//! `Map` and `Set` storage (T25, design D5): an ordered hash table keyed by SameValueZero.
//! The JS-visible methods are in the prelude (`crates/lower/builtins/map.js`) over
//! [`js2wasi3_ir::MapOp`] intrinsics, which are these helpers.
//!
//! # Layout
//! - `$map`: a final subtype of `$object` (class [`CLASS_MAP`] or [`CLASS_SET`]) with
//!   `table: mut (ref $mtable)`.
//! - `$mtable { keys: mut (ref $vals), vals: mut (ref $vals), index: mut (ref $i32s), used: mut i32,
//!   size: mut i32, next: mut (ref null $mtable), removed: mut (ref null $i32s) }`: entries
//!   `0..used` in insertion order (a deleted entry has a Wasm null key), `size` live ones. `index`
//!   is an open-addressing hash index (linear probing, a power of 2 ≥ twice the capacity) mapping
//!   slots to `entry + 1` (0: empty); a deleted entry keeps its slot until the next rehash. Sets
//!   store `undefined` values.
//! - `$mapiter`: a final subtype of `$object` with `table: mut (ref null $mtable)` (null: done
//!   for good), `pos: mut i32` (the next entry to look at) and `mode: i32` (`MAP_ITER_*` + 3 × the
//!   map's kind).
//!
//! # Iterators that survive changes (V8's OrderedHashTable transitions)
//! A rehash (growing, or compacting deleted entries) or `clear` never changes a table in place:
//! it makes a new one and marks the old one obsolete (`next` set). `removed` is the ascending list
//! of the entries a rehash dropped, or null after `clear`. An iterator on an obsolete table moves
//! to the newest one, each step mapping its position: after `clear` to 0, after a rehash to
//! `pos - (dropped entries before pos)`. So iteration sees entries added during it, skips deleted
//! ones, and never visits an entry twice.
//!
//! # Hashing (SameValueZero)
//! Numbers by value (`i31ref` and `$number` alike, `-0` as `+0`, every NaN the same), strings and
//! symbols by content (symbols then compare by identity), objects by an identity hash kept in the
//! high bits of `class` (assigned on first use from `G_ID_HASH`), anything else a constant.

use crate::runtime::JsTest;
use js2wasi3_ir::{Builtin, MAP_KIND_SET, MapOp};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

use crate::object::{self, F_CLASS};
use crate::runtime::{
    self, Cx, G_ID_HASH, G_UNDEFINED, T_BYTES, T_I32S, T_KEYS, T_MAP, T_MAPITER, T_MTABLE,
    T_NUMBER, T_OBJECT, T_VALS, jsval, rt,
};

/// `$object.class` of a `Map` / `Set`.
pub const CLASS_MAP: i32 = 2;
pub const CLASS_SET: i32 = 3;
/// The identity hash lives in `class` from this bit up (the low bits are the class and flags).
const ID_SHIFT: i32 = 13;

/// `$map` field after the object's.
pub const M_TABLE: u32 = 7;
/// `$mtable` fields.
pub const T_KEYS_F: u32 = 0;
pub const T_VALS_F: u32 = 1;
const T_INDEX: u32 = 2;
pub const T_USED: u32 = 3;
pub const T_SIZE: u32 = 4;
const T_NEXT: u32 = 5;
const T_REMOVED: u32 = 6;
/// `$mapiter` fields after the object's.
pub const I_TABLE: u32 = 7;
pub const I_POS: u32 = 8;
pub const I_MODE: u32 = 9;

/// The capacity of the first real table.
const MIN_CAP: i32 = 4;

pub fn map() -> HeapType {
    HeapType::Concrete(T_MAP)
}

fn mtable() -> HeapType {
    HeapType::Concrete(T_MTABLE)
}

pub fn mapiter() -> HeapType {
    HeapType::Concrete(T_MAPITER)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// A [`MapOp`] (signatures in `js2wasi3_ir::Intrinsic::signature`).
    Op(MapOp),
    /// `(v: jsval) -> i32`: the SameValueZero hash.
    Hash,
    /// `(a, b: jsval) -> i32`: SameValueZero.
    Same,
    /// `(t: (ref $mtable), key: jsval, hash: i32) -> i32`: the entry, or -1.
    Find,
    /// `(m: (ref $map))`: replace the table by one with room for twice the live entries.
    Rehash,
    /// `(cap: i32) -> (ref $mtable)`: an empty table.
    NewTable,
    /// `(o: (ref $object)) -> i32`: the object's identity hash.
    IdHash,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Op(op) => js2wasi3_ir::Intrinsic::Map(op).name(),
            Helper::Hash => "map_hash",
            Helper::Same => "same_value_zero",
            Helper::Find => "map_find",
            Helper::Rehash => "map_rehash",
            Helper::NewTable => "map_new_table",
            Helper::IdHash => "identity_hash",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::I32;
        let v = jsval();
        match self {
            Helper::Op(op) => {
                let (params, result) = js2wasi3_ir::Intrinsic::Map(op).signature();
                let ty = |t: &js2wasi3_ir::Type| match t {
                    js2wasi3_ir::Type::JsVal => v,
                    js2wasi3_ir::Type::F64 => ValType::F64,
                    _ => I32,
                };
                (
                    params.iter().map(ty).collect(),
                    result.iter().map(ty).collect(),
                )
            }
            Helper::Hash => (vec![v], vec![I32]),
            Helper::Same => (vec![v, v], vec![I32]),
            Helper::Find => (vec![rt(false, T_MTABLE), v, I32], vec![I32]),
            Helper::Rehash => (vec![rt(false, T_MAP)], vec![]),
            Helper::NewTable => (vec![I32], vec![rt(false, T_MTABLE)]),
            Helper::IdHash => (vec![rt(false, T_OBJECT)], vec![I32]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Op(op) => match op {
                MapOp::New(kind) => new(cx, kind),
                MapOp::Kind => kind(),
                MapOp::Get => get(cx, false),
                MapOp::Has => get(cx, true),
                MapOp::Set => set(cx),
                MapOp::Delete => delete(cx),
                MapOp::Clear => clear(cx),
                MapOp::Size => size(),
                MapOp::Iter(mode) => iter(cx, mode),
                MapOp::IterKind => iter_kind(),
                MapOp::Step => step(),
                MapOp::Key => entry(T_KEYS_F),
                MapOp::Value => entry(T_VALS_F),
            },
            Helper::Hash => hash(cx),
            Helper::Same => same(cx),
            Helper::Find => find(cx),
            Helper::Rehash => rehash(cx),
            Helper::NewTable => new_table(),
            Helper::IdHash => id_hash(),
        }
    }
}

const EQ: HeapType = HeapType::Abstract {
    shared: false,
    ty: wasm_encoder::AbstractHeapType::Eq,
};

fn mh(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Map(h))
}

/// Push local `m`'s table (`m` a jsval holding a `$map`).
fn table_of(s: &mut InstructionSink, m: u32) {
    s.local_get(m)
        .ref_cast_non_null(map())
        .struct_get(T_MAP, M_TABLE);
}

/// `$map_new_table(cap)`: `index` has the smallest power of 2 ≥ 2 × cap slots (at least 1).
fn new_table() -> Function {
    const CAP: u32 = 0;
    const N: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.i32_const(1).local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(N)
        .local_get(CAP)
        .i32_const(1)
        .i32_shl()
        .i32_ge_u()
        .br_if(1);
    s.local_get(N).i32_const(1).i32_shl().local_set(N);
    s.br(0).end().end();
    s.local_get(CAP).array_new_default(T_VALS);
    s.local_get(CAP).array_new_default(T_VALS);
    s.local_get(N).array_new_default(T_I32S);
    s.i32_const(0).i32_const(0);
    s.ref_null(mtable()).ref_null(HeapType::Concrete(T_I32S));
    s.struct_new(T_MTABLE);
    s.end();
    f
}

/// `$map_new()`: an empty map of `kind` (no room yet: the first `set` rehashes).
fn new(cx: &mut dyn Cx, kind: u8) -> Function {
    let b = if kind == MAP_KIND_SET {
        Builtin::Set
    } else {
        Builtin::Map
    };
    let proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(b)));
    let new_table = mh(cx, Helper::NewTable);
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.i32_const(CLASS_MAP + kind as i32);
    s.call(proto);
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    s.i32_const(0).call(new_table);
    s.struct_new(T_MAP);
    s.end();
    f
}

/// `(v) -> i32`: 0 for a `Map`, 1 for a `Set`, -1 otherwise.
fn kind() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).ref_test_non_null(map());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(0)
        .ref_cast_non_null(map())
        .struct_get(T_MAP, F_CLASS)
        .i32_const(object::CLASS_MASK)
        .i32_and()
        .i32_const(CLASS_MAP)
        .i32_sub();
    s.else_();
    s.i32_const(-1);
    s.end();
    s.end();
    f
}

fn size() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    table_of(&mut s, 0);
    s.struct_get(T_MTABLE, T_SIZE);
    s.end();
    f
}

/// `$identity_hash(o)`: assigned on first use (never 0).
fn id_hash() -> Function {
    const O: u32 = 0;
    const H: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(O)
        .struct_get(T_OBJECT, F_CLASS)
        .i32_const(ID_SHIFT)
        .i32_shr_u()
        .local_tee(H);
    s.if_(BlockType::Empty);
    s.local_get(H).return_();
    s.end();
    s.global_get(G_ID_HASH)
        .i32_const(1)
        .i32_add()
        .global_set(G_ID_HASH);
    s.global_get(G_ID_HASH)
        .i32_const(0x9E37_79B1_u32 as i32)
        .i32_mul()
        .i32_const(ID_SHIFT)
        .i32_shr_u()
        .local_tee(H);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(1).local_set(H);
    s.end();
    s.local_get(O);
    s.local_get(O).struct_get(T_OBJECT, F_CLASS);
    s.local_get(H).i32_const(ID_SHIFT).i32_shl().i32_or();
    s.struct_set(T_OBJECT, F_CLASS);
    s.local_get(H);
    s.end();
    f
}

/// Push the hash of the `f64` on the stack (`-0` as `+0`, one NaN); uses local `bits` (i64).
fn hash_f64(s: &mut InstructionSink, bits: u32) {
    // x + 0 turns -0 into +0.
    s.f64_const(0.0.into()).f64_add();
    s.i64_reinterpret_f64().local_tee(bits);
    // NaN: exponent all ones and a non-zero fraction.
    s.i64_const(0x7fff_ffff_ffff_ffff).i64_and();
    s.i64_const(0x7ff0_0000_0000_0000).i64_gt_u();
    s.if_(BlockType::Empty);
    s.i64_const(0x7ff8_0000_0000_0000).local_set(bits);
    s.end();
    s.local_get(bits)
        .local_get(bits)
        .i64_const(32)
        .i64_shr_u()
        .i64_xor();
    s.i32_wrap_i64();
    s.i32_const(0x9E37_79B1_u32 as i32).i32_mul();
}

fn hash(cx: &mut dyn Cx) -> Function {
    let string_hash = cx.helper(runtime::Helper::Obj(object::Helper::Hash));
    let id = mh(cx, Helper::IdHash);
    const V: u32 = 0;
    const BITS: u32 = 1;
    let mut f = Function::new([(1, ValType::I64)]);
    let mut s = f.instructions();
    s.local_get(V).ref_test_non_null(HeapType::I31);
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(HeapType::I31).i31_get_s();
    s.f64_convert_i32_s();
    hash_f64(&mut s, BITS);
    s.return_();
    s.end();
    s.local_get(V)
        .ref_test_non_null(HeapType::Concrete(T_NUMBER));
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(HeapType::Concrete(T_NUMBER))
        .struct_get(T_NUMBER, 0);
    hash_f64(&mut s, BITS);
    s.return_();
    s.end();
    s.test_string(V);
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(runtime::string())
        .call(string_hash);
    s.return_();
    s.end();
    s.test_object(V);
    s.if_(BlockType::Empty);
    s.local_get(V).ref_cast_non_null(runtime::object()).call(id);
    s.return_();
    s.end();
    if cx.bigint() {
        // T40: by value.
        let big_hash = cx.helper(runtime::Helper::Big(crate::bigint::Helper::Hash));
        crate::bigint::is_bigint(&mut s, V);
        s.if_(BlockType::Empty);
        s.local_get(V).ref_cast_non_null(crate::bigint::bigint());
        s.call(big_hash).return_();
        s.end();
    }
    s.i32_const(0x2545_F491);
    s.end();
    f
}

/// SameValueZero (after `ref.eq` failed: numbers by value, strings by content).
fn same(cx: &mut dyn Cx) -> Function {
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let str_eq = cx.helper(runtime::Helper::StrEq);
    const A: u32 = 0;
    const B: u32 = 1;
    const X: u32 = 2;
    const Y: u32 = 3;
    let mut f = Function::new([(2, ValType::F64)]);
    let mut s = f.instructions();
    s.local_get(A).local_get(B).ref_eq();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.local_get(A).call(is_number);
    s.local_get(B).call(is_number);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(A).call(to_number).local_tee(X);
    s.local_get(B).call(to_number).local_tee(Y);
    s.f64_eq();
    s.local_get(X).local_get(X).f64_ne();
    s.local_get(Y).local_get(Y).f64_ne();
    s.i32_and().i32_or();
    s.return_();
    s.end();
    runtime::is_string(&mut s, A);
    runtime::is_string(&mut s, B);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(A).ref_cast_non_null(runtime::string());
    s.local_get(B).ref_cast_non_null(runtime::string());
    s.call(str_eq).return_();
    s.end();
    if cx.bigint() {
        // T40: BigInts by value.
        let cmp = cx.helper(runtime::Helper::Big(crate::bigint::Helper::Cmp));
        crate::bigint::is_bigint(&mut s, A);
        crate::bigint::is_bigint(&mut s, B);
        s.i32_and();
        s.if_(BlockType::Empty);
        s.local_get(A).ref_cast_non_null(crate::bigint::bigint());
        s.local_get(B).ref_cast_non_null(crate::bigint::bigint());
        s.call(cmp).i32_eqz().return_();
        s.end();
    }
    s.i32_const(0);
    s.end();
    f
}

/// `$map_find(t, key, hash)`.
fn find(cx: &mut dyn Cx) -> Function {
    let same = mh(cx, Helper::Same);
    const T: u32 = 0;
    const K: u32 = 1;
    const H: u32 = 2;
    const IDX: u32 = 3;
    const MASK: u32 = 4;
    const I: u32 = 5;
    const E: u32 = 6;
    const KEY: u32 = 7;
    let mut f = Function::new([(1, rt(false, T_I32S)), (3, ValType::I32), (1, jsval())]);
    let mut s = f.instructions();
    s.local_get(T).struct_get(T_MTABLE, T_INDEX).local_tee(IDX);
    s.array_len().i32_const(1).i32_sub().local_tee(MASK);
    s.local_get(H).i32_and().local_set(I);
    s.loop_(BlockType::Empty);
    s.local_get(IDX).local_get(I).array_get(T_I32S).local_tee(E);
    s.i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(-1).return_();
    s.end();
    s.local_get(T)
        .struct_get(T_MTABLE, T_KEYS_F)
        .local_get(E)
        .i32_const(1)
        .i32_sub()
        .local_tee(E)
        .array_get(T_VALS)
        .local_tee(KEY);
    s.ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(KEY).local_get(K).ref_eq();
    s.if_(BlockType::Empty);
    s.local_get(E).return_();
    s.end();
    s.local_get(KEY).local_get(K).call(same);
    s.if_(BlockType::Empty);
    s.local_get(E).return_();
    s.end();
    s.end();
    s.local_get(I)
        .i32_const(1)
        .i32_add()
        .local_get(MASK)
        .i32_and()
        .local_set(I);
    s.br(0);
    s.end();
    s.unreachable();
    s.end();
    f
}

/// Put entry `e` (local) with hash `h` (local) into index `idx` (local, mask in `mask`), using
/// local `i`.
fn insert_index(s: &mut InstructionSink, idx: u32, mask: u32, h: u32, e: u32, i: u32) {
    s.local_get(h).local_get(mask).i32_and().local_set(i);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(idx)
        .local_get(i)
        .array_get(T_I32S)
        .i32_eqz()
        .br_if(1);
    s.local_get(i)
        .i32_const(1)
        .i32_add()
        .local_get(mask)
        .i32_and()
        .local_set(i);
    s.br(0).end().end();
    s.local_get(idx).local_get(i);
    s.local_get(e).i32_const(1).i32_add();
    s.array_set(T_I32S);
}

/// `$map_rehash(m)`: a new table with the live entries in order and room for as many again; the
/// old one becomes obsolete (`removed`: the entries dropped).
fn rehash(cx: &mut dyn Cx) -> Function {
    let new_table = mh(cx, Helper::NewTable);
    let hash = mh(cx, Helper::Hash);
    const M: u32 = 0;
    const OLD: u32 = 1;
    const NEW: u32 = 2;
    const REMOVED: u32 = 3;
    const IDX: u32 = 4;
    const E: u32 = 5;
    const USED: u32 = 6;
    const N: u32 = 7;
    const R: u32 = 8;
    const MASK: u32 = 9;
    const H: u32 = 10;
    const I: u32 = 11;
    const K: u32 = 12;
    let mut f = Function::new([
        (2, rt(true, T_MTABLE)),
        (1, rt(true, T_I32S)),
        (1, rt(true, T_I32S)),
        (7, ValType::I32),
        (1, jsval()),
    ]);
    let mut s = f.instructions();
    s.local_get(M).struct_get(T_MAP, M_TABLE).local_tee(OLD);
    s.struct_get(T_MTABLE, T_USED).local_set(USED);
    // Capacity: twice the live entries, at least MIN_CAP.
    s.local_get(OLD)
        .struct_get(T_MTABLE, T_SIZE)
        .i32_const(1)
        .i32_shl();
    s.i32_const(MIN_CAP);
    s.local_get(OLD)
        .struct_get(T_MTABLE, T_SIZE)
        .i32_const(1)
        .i32_shl();
    s.i32_const(MIN_CAP);
    s.i32_gt_s();
    s.select();
    s.call(new_table).local_tee(NEW);
    s.struct_get(T_MTABLE, T_INDEX).local_tee(IDX);
    s.array_len().i32_const(1).i32_sub().local_set(MASK);
    s.local_get(USED)
        .local_get(OLD)
        .struct_get(T_MTABLE, T_SIZE)
        .i32_sub()
        .array_new_default(T_I32S)
        .local_set(REMOVED);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(E).local_get(USED).i32_ge_u().br_if(1);
    s.local_get(OLD)
        .struct_get(T_MTABLE, T_KEYS_F)
        .local_get(E)
        .array_get(T_VALS)
        .local_tee(K);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    s.local_get(REMOVED)
        .local_get(R)
        .local_get(E)
        .array_set(T_I32S);
    s.local_get(R).i32_const(1).i32_add().local_set(R);
    s.else_();
    s.local_get(NEW).struct_get(T_MTABLE, T_KEYS_F);
    s.local_get(N).local_get(K).array_set(T_VALS);
    s.local_get(NEW).struct_get(T_MTABLE, T_VALS_F).local_get(N);
    s.local_get(OLD)
        .struct_get(T_MTABLE, T_VALS_F)
        .local_get(E)
        .array_get(T_VALS);
    s.array_set(T_VALS);
    s.local_get(K).call(hash).local_set(H);
    insert_index(&mut s, IDX, MASK, H, N, I);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.end();
    s.local_get(E).i32_const(1).i32_add().local_set(E);
    s.br(0).end().end();
    s.local_get(NEW).local_get(N).struct_set(T_MTABLE, T_USED);
    s.local_get(NEW).local_get(N).struct_set(T_MTABLE, T_SIZE);
    s.local_get(OLD).local_get(NEW).struct_set(T_MTABLE, T_NEXT);
    s.local_get(OLD)
        .local_get(REMOVED)
        .struct_set(T_MTABLE, T_REMOVED);
    obsolete_drop(&mut s, OLD);
    s.local_get(M)
        .local_get(NEW)
        .ref_as_non_null()
        .struct_set(T_MAP, M_TABLE);
    s.end();
    f
}

/// An obsolete table keeps only what iterators need: drop its entries (local `t`).
fn obsolete_drop(s: &mut InstructionSink, t: u32) {
    s.local_get(t).i32_const(0).array_new_default(T_VALS);
    s.struct_set(T_MTABLE, T_KEYS_F);
    s.local_get(t).i32_const(0).array_new_default(T_VALS);
    s.struct_set(T_MTABLE, T_VALS_F);
    s.local_get(t).i32_const(1).array_new_default(T_I32S);
    s.struct_set(T_MTABLE, T_INDEX);
}

/// `get(m, key)` (`has`: whether it's there).
fn get(cx: &mut dyn Cx, has: bool) -> Function {
    let find = mh(cx, Helper::Find);
    let hash = mh(cx, Helper::Hash);
    const M: u32 = 0;
    const K: u32 = 1;
    const T: u32 = 2;
    const E: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_MTABLE)), (1, ValType::I32)]);
    let mut s = f.instructions();
    table_of(&mut s, M);
    s.local_tee(T).ref_as_non_null();
    s.local_get(K)
        .local_get(K)
        .call(hash)
        .call(find)
        .local_set(E);
    if has {
        s.local_get(E).i32_const(0).i32_ge_s();
    } else {
        s.local_get(E).i32_const(0).i32_lt_s();
        s.if_(BlockType::Result(jsval()));
        s.global_get(G_UNDEFINED);
        s.else_();
        s.local_get(T)
            .struct_get(T_MTABLE, T_VALS_F)
            .local_get(E)
            .array_get(T_VALS);
        s.end();
    }
    s.end();
    f
}

/// `set(m, key, value)`.
fn set(cx: &mut dyn Cx) -> Function {
    let find = mh(cx, Helper::Find);
    let hash = mh(cx, Helper::Hash);
    let rehash = mh(cx, Helper::Rehash);
    const M: u32 = 0;
    const K: u32 = 1;
    const V: u32 = 2;
    const T: u32 = 3;
    const H: u32 = 4;
    const E: u32 = 5;
    const MASK: u32 = 6;
    const I: u32 = 7;
    const IDX: u32 = 8;
    let mut f = Function::new([
        (1, rt(true, T_MTABLE)),
        (4, ValType::I32),
        (1, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    table_of(&mut s, M);
    s.local_tee(T).ref_as_non_null();
    s.local_get(K).local_get(K).call(hash).local_tee(H);
    s.call(find).local_tee(E);
    s.i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    s.local_get(T).struct_get(T_MTABLE, T_VALS_F);
    s.local_get(E).local_get(V).array_set(T_VALS);
    s.return_();
    s.end();
    // A new key; -0 is stored as +0.
    s.local_get(K)
        .ref_test_non_null(HeapType::Concrete(T_NUMBER));
    s.if_(BlockType::Empty);
    s.local_get(K)
        .ref_cast_non_null(HeapType::Concrete(T_NUMBER))
        .struct_get(T_NUMBER, 0)
        .f64_const(0.0.into())
        .f64_eq();
    s.if_(BlockType::Empty);
    s.i32_const(0).ref_i31().local_set(K);
    s.end();
    s.end();
    s.local_get(T).struct_get(T_MTABLE, T_USED);
    s.local_get(T).struct_get(T_MTABLE, T_KEYS_F).array_len();
    s.i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(M).ref_cast_non_null(map()).call(rehash);
    table_of(&mut s, M);
    s.local_set(T);
    s.end();
    s.local_get(T).struct_get(T_MTABLE, T_USED).local_set(E);
    s.local_get(T).struct_get(T_MTABLE, T_KEYS_F);
    s.local_get(E).local_get(K).array_set(T_VALS);
    s.local_get(T).struct_get(T_MTABLE, T_VALS_F);
    s.local_get(E).local_get(V).array_set(T_VALS);
    s.local_get(T).local_get(E).i32_const(1).i32_add();
    s.struct_set(T_MTABLE, T_USED);
    s.local_get(T);
    s.local_get(T)
        .struct_get(T_MTABLE, T_SIZE)
        .i32_const(1)
        .i32_add();
    s.struct_set(T_MTABLE, T_SIZE);
    s.local_get(T).struct_get(T_MTABLE, T_INDEX).local_tee(IDX);
    s.array_len().i32_const(1).i32_sub().local_set(MASK);
    insert_index(&mut s, IDX, MASK, H, E, I);
    s.end();
    f
}

/// `delete(m, key) -> i32`.
fn delete(cx: &mut dyn Cx) -> Function {
    let find = mh(cx, Helper::Find);
    let hash = mh(cx, Helper::Hash);
    const M: u32 = 0;
    const K: u32 = 1;
    const T: u32 = 2;
    const E: u32 = 3;
    let mut f = Function::new([(1, rt(true, T_MTABLE)), (1, ValType::I32)]);
    let mut s = f.instructions();
    table_of(&mut s, M);
    s.local_tee(T).ref_as_non_null();
    s.local_get(K)
        .local_get(K)
        .call(hash)
        .call(find)
        .local_tee(E);
    s.i32_const(0).i32_lt_s();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.local_get(T).struct_get(T_MTABLE, T_KEYS_F);
    s.local_get(E).ref_null(EQ).array_set(T_VALS);
    s.local_get(T).struct_get(T_MTABLE, T_VALS_F);
    s.local_get(E).ref_null(EQ).array_set(T_VALS);
    s.local_get(T);
    s.local_get(T)
        .struct_get(T_MTABLE, T_SIZE)
        .i32_const(1)
        .i32_sub();
    s.struct_set(T_MTABLE, T_SIZE);
    s.i32_const(1);
    s.end();
    f
}

/// `clear(m)`: a new empty table; the old one is obsolete with `removed` null.
fn clear(cx: &mut dyn Cx) -> Function {
    let new_table = mh(cx, Helper::NewTable);
    const M: u32 = 0;
    const T: u32 = 1;
    let mut f = Function::new([(1, rt(true, T_MTABLE))]);
    let mut s = f.instructions();
    table_of(&mut s, M);
    s.local_tee(T).struct_get(T_MTABLE, T_USED).i32_eqz();
    s.if_(BlockType::Empty);
    s.return_();
    s.end();
    s.local_get(M).ref_cast_non_null(map());
    s.i32_const(0).call(new_table);
    s.struct_set(T_MAP, M_TABLE);
    s.local_get(T);
    table_of(&mut s, M);
    s.struct_set(T_MTABLE, T_NEXT);
    obsolete_drop(&mut s, T);
    s.end();
    f
}

/// `iter(m)`: a new iterator on `m`'s current table.
fn iter(cx: &mut dyn Cx, mode: u8) -> Function {
    let kind = mh(cx, Helper::Op(MapOp::Kind));
    let map_proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
        Builtin::MapIterator,
    )));
    let set_proto = cx.helper(runtime::Helper::Obj(object::Helper::Proto(
        Builtin::SetIterator,
    )));
    const M: u32 = 0;
    const K: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(M).call(kind).local_set(K);
    s.i32_const(object::CLASS_PLAIN);
    s.local_get(K);
    s.if_(BlockType::Result(rt(false, T_OBJECT)));
    s.call(set_proto);
    s.else_();
    s.call(map_proto);
    s.end();
    s.i32_const(0).array_new_default(T_KEYS);
    s.i32_const(0).array_new_default(T_VALS);
    s.i32_const(0).array_new_default(T_BYTES);
    s.i32_const(0);
    s.ref_null(HeapType::Concrete(T_I32S));
    table_of(&mut s, M);
    s.i32_const(0);
    s.local_get(K)
        .i32_const(3)
        .i32_mul()
        .i32_const(mode as i32)
        .i32_add();
    s.struct_new(T_MAPITER);
    s.end();
    f
}

fn iter_kind() -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0).ref_test_non_null(mapiter());
    s.if_(BlockType::Result(ValType::I32));
    s.local_get(0)
        .ref_cast_non_null(mapiter())
        .struct_get(T_MAPITER, I_MODE);
    s.else_();
    s.i32_const(-1);
    s.end();
    s.end();
    f
}

/// `step(it) -> i32`: move to the newest table (see the module docs), then to the next live
/// entry.
fn step() -> Function {
    const V: u32 = 0;
    const IT: u32 = 1;
    const T: u32 = 2;
    const POS: u32 = 3;
    const R: u32 = 4;
    const J: u32 = 5;
    const N: u32 = 6;
    let mut f = Function::new([
        (1, rt(true, T_MAPITER)),
        (1, rt(true, T_MTABLE)),
        (4, ValType::I32),
    ]);
    let mut s = f.instructions();
    s.local_get(V).ref_cast_non_null(mapiter()).local_tee(IT);
    s.struct_get(T_MAPITER, I_TABLE).local_tee(T);
    s.ref_is_null();
    s.if_(BlockType::Empty);
    s.i32_const(-1).return_();
    s.end();
    s.local_get(IT).struct_get(T_MAPITER, I_POS).local_set(POS);
    // Transitions.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(T)
        .struct_get(T_MTABLE, T_NEXT)
        .ref_is_null()
        .br_if(1);
    s.local_get(T).struct_get(T_MTABLE, T_REMOVED).ref_is_null();
    s.if_(BlockType::Empty);
    s.i32_const(0).local_set(POS);
    s.else_();
    // pos -= the dropped entries before pos (the list is ascending).
    s.i32_const(0).local_set(J);
    s.local_get(T)
        .struct_get(T_MTABLE, T_REMOVED)
        .ref_as_non_null()
        .array_len()
        .local_set(N);
    s.i32_const(0).local_set(R);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(J).local_get(N).i32_ge_u().br_if(1);
    s.local_get(T)
        .struct_get(T_MTABLE, T_REMOVED)
        .ref_as_non_null()
        .local_get(J)
        .array_get(T_I32S)
        .local_get(POS)
        .i32_ge_u()
        .br_if(1);
    s.local_get(R).i32_const(1).i32_add().local_set(R);
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(POS).local_get(R).i32_sub().local_set(POS);
    s.end();
    s.local_get(T).struct_get(T_MTABLE, T_NEXT).local_set(T);
    s.br(0).end().end();
    s.local_get(IT).local_get(T).struct_set(T_MAPITER, I_TABLE);
    // The next live entry.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(POS)
        .local_get(T)
        .struct_get(T_MTABLE, T_USED)
        .i32_ge_u()
        .br_if(1);
    s.local_get(T)
        .struct_get(T_MTABLE, T_KEYS_F)
        .local_get(POS)
        .array_get(T_VALS)
        .ref_is_null()
        .i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(IT)
        .local_get(POS)
        .i32_const(1)
        .i32_add()
        .struct_set(T_MAPITER, I_POS);
    s.local_get(POS).return_();
    s.end();
    s.local_get(POS).i32_const(1).i32_add().local_set(POS);
    s.br(0).end().end();
    // Done for good.
    s.local_get(IT)
        .ref_null(mtable())
        .struct_set(T_MAPITER, I_TABLE);
    s.i32_const(-1);
    s.end();
    f
}

/// `key(it, e)` / `value(it, e)`.
fn entry(field: u32) -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(0)
        .ref_cast_non_null(mapiter())
        .struct_get(T_MAPITER, I_TABLE)
        .ref_as_non_null()
        .struct_get(T_MTABLE, field)
        .local_get(1)
        .array_get(T_VALS);
    s.end();
    f
}
