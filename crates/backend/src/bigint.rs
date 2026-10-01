//! BigInts (T40), emitted only in a module that can make one ([`Cx::bigint`]).
//!
//! A BigInt is a `$bigint`, a final `(array (mut i32))`: element 0 is the sign (0, or 1 for a
//! negative value), then the magnitude in 32-bit limbs, least significant first, normalized (no
//! zero top limb; `0n` is `[0]`). A BigInt is never changed once made, so `===` compares contents.
//! Constants are immutable globals ([`Cx::bigint_const`]).
//!
//! The arithmetic works on magnitudes with `i64` intermediates: schoolbook multiplication, Knuth's
//! algorithm D for division (Hacker's Delight's `divmnu`), and `& | ^` on two's-complement copies.
//! Results over 2^30 bits throw V8's RangeError.

use js2wasi3_ir::{BinOp, ErrorKind, NumericOp};
use wasm_encoder::{
    BlockType, ConstExpr, Function, GlobalSection, GlobalType, HeapType, InstructionSink, ValType,
};

use crate::object;
use crate::runtime::{self, Cx, T_BIGINT, T_I32S, T_STRING, jsval, rt};

/// Results larger than this many limbs (2^30 bits, V8's limit) throw.
const MAX_LIMBS: i32 = 1 << 25;

pub fn bigint() -> HeapType {
    HeapType::Concrete(T_BIGINT)
}

fn big() -> ValType {
    rt(false, T_BIGINT)
}

fn nbig() -> ValType {
    rt(true, T_BIGINT)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    /// `(r) -> big`: `r` without zero top limbs (a copy if any), sign 0 if it's zero.
    Norm,
    /// `(a, b) -> i32`: compare the magnitudes: -1, 0 or 1.
    MagCmp,
    /// `(a, b) -> i32`: compare: -1, 0 or 1.
    Cmp,
    /// `(a, b, sign) -> big`: |a| + |b| with that sign.
    MagAdd,
    /// `(a, b, sign) -> big`: |a| - |b| (|a| ≥ |b|) with that sign.
    MagSub,
    /// `(a, b, sb) -> big`: a + (|b| with sign `sb`).
    AddS,
    Neg,
    Mul,
    /// `(a, b) -> big` (`b` not zero): the truncated quotient, or (`true`) the remainder.
    DivMod(bool),
    /// `(a, b) -> big` (`b` ≥ 0).
    Pow,
    /// `(a, k: i32 ≥ 0) -> big`: a · 2^k.
    Shl,
    /// `(a, k: i32 ≥ 0) -> big`: floor(a / 2^k).
    Shr,
    /// `(a, b, right: i32) -> big`: `a << b` or `a >> b`.
    Shift,
    /// `(a, b) -> big`: `&` (0), `|` (1), `^` (2).
    Bitwise(u8),
    /// `(a, len) -> (ref $i32s)`: `len` limbs of a's two's complement.
    ToTc,
    /// `(x: (ref $i32s)) -> big`: the value of a two's complement.
    FromTc,
    /// `(x: f64) -> big`: an integral number.
    FromF64,
    /// `(a) -> f64`: correctly rounded.
    ToF64,
    /// `(a, x: f64) -> i32`: compare with a number: -1, 0, 1, or 2 if `x` is NaN.
    CmpNum,
    /// `(a, radix: i32) -> (ref $string)`.
    ToString,
    /// `(a) -> i32`: a hash of the value (Map/Set keys).
    Hash,
    /// `(a, v: jsval) -> i32`: `a == v` for a primitive `v` other than a symbol or nullish.
    LooseEq,
    /// `(x, y: jsval) -> i32`: compare two primitives, one a BigInt, as IsLessThan does: -1, 0,
    /// 1, or 2 if they're unordered (NaN, a string that isn't an integer).
    CmpAny,
    /// `(v: jsval) -> jsval`: StringToBigInt of a string (the prelude's): a BigInt or `undefined`.
    Parse,
    /// `(v) -> jsval`: ToNumeric.
    ToNumeric,
    /// `(a, b) -> jsval` for a binary operator, `(v) -> jsval` otherwise: see
    /// [`js2wasi3_ir::Intrinsic::Numeric`].
    Numeric(NumericOp),
    /// `(a, b) -> jsval`: the BigInt operation on two ToNumeric results, one of them a BigInt
    /// (the TypeError for mixing if the other isn't).
    Binary(BinOp),
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::Norm => "bigint_norm",
            Helper::MagCmp => "bigint_mag_cmp",
            Helper::Cmp => "bigint_cmp",
            Helper::MagAdd => "bigint_mag_add",
            Helper::MagSub => "bigint_mag_sub",
            Helper::AddS => "bigint_add",
            Helper::Neg => "bigint_neg",
            Helper::Mul => "bigint_mul",
            Helper::DivMod(false) => "bigint_div",
            Helper::DivMod(true) => "bigint_rem",
            Helper::Pow => "bigint_pow",
            Helper::Shl => "bigint_shl",
            Helper::Shr => "bigint_shr",
            Helper::Shift => "bigint_shift",
            Helper::Bitwise(_) => "bigint_bitwise",
            Helper::ToTc => "bigint_to_tc",
            Helper::FromTc => "bigint_from_tc",
            Helper::FromF64 => "bigint_from_f64",
            Helper::ToF64 => "bigint_to_f64",
            Helper::CmpNum => "bigint_cmp_number",
            Helper::ToString => "bigint_to_string",
            Helper::Hash => "bigint_hash",
            Helper::LooseEq => "bigint_loose_eq",
            Helper::CmpAny => "bigint_compare",
            Helper::Parse => "bigint_parse",
            Helper::ToNumeric => "to_numeric",
            Helper::Numeric(_) => "numeric",
            Helper::Binary(_) => "bigint_binary",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        let (b, v, i, f) = (big(), jsval(), ValType::I32, ValType::F64);
        match self {
            Helper::Norm | Helper::Neg => (vec![b], vec![b]),
            Helper::MagCmp | Helper::Cmp => (vec![b, b], vec![i]),
            Helper::MagAdd | Helper::MagSub | Helper::AddS | Helper::Shift => {
                (vec![b, b, i], vec![b])
            }
            Helper::Mul | Helper::DivMod(_) | Helper::Pow | Helper::Bitwise(_) => {
                (vec![b, b], vec![b])
            }
            Helper::Shl | Helper::Shr => (vec![b, i], vec![b]),
            Helper::ToTc => (vec![b, i], vec![rt(false, T_I32S)]),
            Helper::FromTc => (vec![rt(false, T_I32S)], vec![b]),
            Helper::FromF64 => (vec![f], vec![b]),
            Helper::ToF64 => (vec![b], vec![f]),
            Helper::CmpNum => (vec![b, f], vec![i]),
            Helper::ToString => (vec![b, i], vec![runtime::string_ref()]),
            Helper::Hash => (vec![b], vec![i]),
            Helper::LooseEq => (vec![b, v], vec![i]),
            Helper::CmpAny => (vec![v, v], vec![i]),
            Helper::Parse => (vec![v], vec![v]),
            Helper::ToNumeric => (vec![v], vec![v]),
            Helper::Numeric(NumericOp::Binary(_)) | Helper::Binary(_) => (vec![v, v], vec![v]),
            Helper::Numeric(_) => (vec![v], vec![v]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::Norm => norm(),
            Helper::MagCmp => mag_cmp(),
            Helper::Cmp => cmp(cx),
            Helper::MagAdd => mag_add(cx),
            Helper::MagSub => mag_sub(cx),
            Helper::AddS => add_s(cx),
            Helper::Neg => neg(),
            Helper::Mul => mul(cx),
            Helper::DivMod(rem) => div_mod(cx, rem),
            Helper::Pow => pow(cx),
            Helper::Shl => shl(cx),
            Helper::Shr => shr(cx),
            Helper::Shift => shift(cx),
            Helper::Bitwise(op) => bitwise(cx, op),
            Helper::ToTc => to_tc(),
            Helper::FromTc => from_tc(cx),
            Helper::FromF64 => from_f64(cx),
            Helper::ToF64 => to_f64(),
            Helper::CmpNum => cmp_num(cx),
            Helper::ToString => to_string(cx),
            Helper::Hash => hash(),
            Helper::LooseEq => loose_eq(cx),
            Helper::CmpAny => cmp_any(cx),
            Helper::Parse => parse(cx),
            Helper::ToNumeric => to_numeric(cx),
            Helper::Numeric(op) => numeric(cx, op),
            Helper::Binary(op) => binary(cx, op),
        }
    }
}

/// A constant's words (sign, then limbs) from its decimal digits (a literal's value), with a
/// leading `-` for a negated literal.
pub fn from_decimal(digits: &str) -> Vec<i32> {
    let mut limbs: Vec<u32> = Vec::new();
    for d in digits.bytes().filter(u8::is_ascii_digit) {
        let mut carry = (d - b'0') as u64;
        for l in &mut limbs {
            let t = *l as u64 * 10 + carry;
            *l = t as u32;
            carry = t >> 32;
        }
        if carry != 0 {
            limbs.push(carry as u32);
        }
    }
    let negative = digits.starts_with('-') && !limbs.is_empty();
    std::iter::once(negative as i32)
        .chain(limbs.into_iter().map(|l| l as i32))
        .collect()
}

/// A constant's global (T57): `(global (mut (ref $bigint)) (global.get placeholder))`, set by
/// the start function.
pub fn const_global(globals: &mut GlobalSection, placeholder: u32) {
    globals.global(
        GlobalType {
            val_type: big(),
            mutable: true,
            shared: false,
        },
        &ConstExpr::global_get(placeholder),
    );
}

fn bh(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Big(h))
}

/// Push "`v` is a BigInt".
pub fn is_bigint(s: &mut InstructionSink, v: u32) {
    s.local_get(v).ref_test_non_null(bigint());
}

/// Push limb `idx` (a local) of BigInt `arr` (a local), as an `i32`.
fn limb(s: &mut InstructionSink, arr: u32, idx: u32) {
    s.local_get(arr)
        .local_get(idx)
        .i32_const(1)
        .i32_add()
        .array_get(T_BIGINT);
}

/// Push the number of limbs of BigInt `arr` (a local).
fn limbs(s: &mut InstructionSink, arr: u32) {
    s.local_get(arr).array_len().i32_const(1).i32_sub();
}

/// Push `0n` / `1n` / `-1n` (fresh).
fn push_small(s: &mut InstructionSink, v: i32) {
    match v {
        0 => s.i32_const(0).array_new_fixed(T_BIGINT, 1),
        1 => s.i32_const(0).i32_const(1).array_new_fixed(T_BIGINT, 2),
        _ => s.i32_const(1).i32_const(1).array_new_fixed(T_BIGINT, 2),
    };
}

/// Throw a RangeError (then `unreachable`, so it types as anything).
fn throw_range(cx: &mut dyn Cx, s: &mut InstructionSink, msg: &str) {
    let throw = cx.helper(runtime::Helper::Obj(object::Helper::Throw(
        ErrorKind::RangeError,
    )));
    let g = cx.str(msg);
    s.global_get(g).call(throw).unreachable();
}

fn throw_type(cx: &mut dyn Cx, s: &mut InstructionSink, msg: &str) {
    let throw = cx.helper(runtime::Helper::Obj(object::Helper::Throw(
        ErrorKind::TypeError,
    )));
    let g = cx.str(msg);
    s.global_get(g).call(throw).unreachable();
}

fn throw_size(cx: &mut dyn Cx, s: &mut InstructionSink) {
    throw_range(cx, s, "Maximum BigInt size exceeded");
}

/// `while (i < end) { body; i++ }` over `i32` locals (`i` starts where the caller left it).
fn for_up(s: &mut InstructionSink, i: u32, end: u32, body: impl FnOnce(&mut InstructionSink)) {
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(i).local_get(end).i32_ge_s().br_if(1);
    body(s);
    s.local_get(i).i32_const(1).i32_add().local_set(i);
    s.br(0).end().end();
}

/// `$bigint_norm(r)`.
fn norm() -> Function {
    const R: u32 = 0;
    const N: u32 = 1;
    const O: u32 = 2;
    let mut f = Function::new([(1, ValType::I32), (1, nbig())]);
    let mut s = f.instructions();
    s.local_get(R).array_len().local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(N).i32_const(1).i32_le_s().br_if(1);
    s.local_get(R)
        .local_get(N)
        .i32_const(1)
        .i32_sub()
        .array_get(T_BIGINT)
        .br_if(1);
    s.local_get(N).i32_const(1).i32_sub().local_set(N);
    s.br(0).end().end();
    s.local_get(N).i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(R).i32_const(0).i32_const(0).array_set(T_BIGINT);
    s.end();
    s.local_get(N).local_get(R).array_len().i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(R).return_();
    s.end();
    s.local_get(N).array_new_default(T_BIGINT).local_set(O);
    s.local_get(O)
        .i32_const(0)
        .local_get(R)
        .i32_const(0)
        .local_get(N);
    s.array_copy(T_BIGINT, T_BIGINT);
    s.local_get(O).ref_as_non_null();
    s.end();
    f
}

/// `$bigint_mag_cmp(a, b)`.
fn mag_cmp() -> Function {
    const A: u32 = 0;
    const B: u32 = 1;
    const I: u32 = 2;
    const X: u32 = 3;
    const Y: u32 = 4;
    let mut f = Function::new([(3, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(A).array_len().local_tee(I);
    s.local_get(B).array_len().i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(1).i32_const(-1);
    s.local_get(A)
        .array_len()
        .local_get(B)
        .array_len()
        .i32_gt_u();
    s.select().return_();
    s.end();
    // I = len; compare elements len-1 down to 1.
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I)
        .i32_const(1)
        .i32_sub()
        .local_tee(I)
        .i32_eqz()
        .br_if(1);
    s.local_get(A).local_get(I).array_get(T_BIGINT).local_set(X);
    s.local_get(B).local_get(I).array_get(T_BIGINT).local_set(Y);
    s.local_get(X).local_get(Y).i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(1).i32_const(-1);
    s.local_get(X).local_get(Y).i32_gt_u();
    s.select().return_();
    s.end();
    s.br(0).end().end();
    s.i32_const(0);
    s.end();
    f
}

/// `$bigint_cmp(a, b)`.
fn cmp(cx: &mut dyn Cx) -> Function {
    let mag_cmp = bh(cx, Helper::MagCmp);
    const A: u32 = 0;
    const B: u32 = 1;
    const SA: u32 = 2;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(A)
        .i32_const(0)
        .array_get(T_BIGINT)
        .local_tee(SA);
    s.local_get(B).i32_const(0).array_get(T_BIGINT).i32_ne();
    s.if_(BlockType::Empty);
    s.i32_const(-1)
        .i32_const(1)
        .local_get(SA)
        .select()
        .return_();
    s.end();
    s.local_get(A).local_get(B).call(mag_cmp);
    s.i32_const(-1).i32_const(1).local_get(SA).select();
    s.i32_mul();
    s.end();
    f
}

/// `$bigint_mag_add(a, b, sign)`.
fn mag_add(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    const A: u32 = 0;
    const B: u32 = 1;
    const SIGN: u32 = 2;
    const LA: u32 = 3;
    const LB: u32 = 4;
    const I: u32 = 5;
    const T: u32 = 6;
    const R: u32 = 7;
    let mut f = Function::new([(3, ValType::I32), (1, ValType::I64), (1, nbig())]);
    let mut s = f.instructions();
    // Make `a` the longer one.
    s.local_get(A)
        .array_len()
        .local_get(B)
        .array_len()
        .i32_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(A).local_set(R);
    s.local_get(B).local_set(A);
    s.local_get(R).ref_as_non_null().local_set(B);
    s.end();
    limbs(&mut s, A);
    s.local_set(LA);
    limbs(&mut s, B);
    s.local_set(LB);
    s.local_get(LA)
        .i32_const(2)
        .i32_add()
        .array_new_default(T_BIGINT)
        .local_set(R);
    s.local_get(R)
        .i32_const(0)
        .local_get(SIGN)
        .array_set(T_BIGINT);
    for_up(&mut s, I, LA, |s| {
        // t = a_i + (i < lb ? b_i : 0) + carry (carry is t >> 32)
        s.local_get(T).i64_const(32).i64_shr_u();
        limb(s, A, I);
        s.i64_extend_i32_u().i64_add();
        s.local_get(I).local_get(LB).i32_lt_s();
        s.if_(BlockType::Result(ValType::I64));
        limb(s, B, I);
        s.i64_extend_i32_u();
        s.else_();
        s.i64_const(0);
        s.end();
        s.i64_add().local_set(T);
        s.local_get(R).local_get(I).i32_const(1).i32_add();
        s.local_get(T).i32_wrap_i64().array_set(T_BIGINT);
    });
    s.local_get(R).local_get(LA).i32_const(1).i32_add();
    s.local_get(T).i64_const(32).i64_shr_u().i32_wrap_i64();
    s.array_set(T_BIGINT);
    s.local_get(R).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_mag_sub(a, b, sign)`: |a| ≥ |b|.
fn mag_sub(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    const A: u32 = 0;
    const B: u32 = 1;
    const SIGN: u32 = 2;
    const LA: u32 = 3;
    const LB: u32 = 4;
    const I: u32 = 5;
    const T: u32 = 6;
    const R: u32 = 7;
    let mut f = Function::new([(3, ValType::I32), (1, ValType::I64), (1, nbig())]);
    let mut s = f.instructions();
    limbs(&mut s, A);
    s.local_set(LA);
    limbs(&mut s, B);
    s.local_set(LB);
    s.local_get(LA)
        .i32_const(1)
        .i32_add()
        .array_new_default(T_BIGINT)
        .local_set(R);
    s.local_get(R)
        .i32_const(0)
        .local_get(SIGN)
        .array_set(T_BIGINT);
    for_up(&mut s, I, LA, |s| {
        // t = a_i - b_i - borrow (borrow: t < 0 before)
        limb(s, A, I);
        s.i64_extend_i32_u();
        s.local_get(I).local_get(LB).i32_lt_s();
        s.if_(BlockType::Result(ValType::I64));
        limb(s, B, I);
        s.i64_extend_i32_u();
        s.else_();
        s.i64_const(0);
        s.end();
        s.i64_sub();
        s.local_get(T).i64_const(63).i64_shr_u();
        s.i64_sub().local_set(T);
        s.local_get(R).local_get(I).i32_const(1).i32_add();
        s.local_get(T).i32_wrap_i64().array_set(T_BIGINT);
    });
    s.local_get(R).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_add(a, b, sb)`.
fn add_s(cx: &mut dyn Cx) -> Function {
    let mag_add = bh(cx, Helper::MagAdd);
    let mag_sub = bh(cx, Helper::MagSub);
    let mag_cmp = bh(cx, Helper::MagCmp);
    const A: u32 = 0;
    const B: u32 = 1;
    const SB: u32 = 2;
    const SA: u32 = 3;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(A)
        .i32_const(0)
        .array_get(T_BIGINT)
        .local_tee(SA);
    s.local_get(SB).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(A)
        .local_get(B)
        .local_get(SA)
        .call(mag_add)
        .return_();
    s.end();
    s.local_get(A)
        .local_get(B)
        .call(mag_cmp)
        .i32_const(0)
        .i32_ge_s();
    s.if_(BlockType::Result(big()));
    s.local_get(A).local_get(B).local_get(SA).call(mag_sub);
    s.else_();
    s.local_get(B).local_get(A).local_get(SB).call(mag_sub);
    s.end();
    s.end();
    f
}

/// `$bigint_neg(a)`.
fn neg() -> Function {
    const A: u32 = 0;
    const R: u32 = 1;
    let mut f = Function::new([(1, nbig())]);
    let mut s = f.instructions();
    s.local_get(A).array_len().i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(A).return_();
    s.end();
    s.local_get(A)
        .array_len()
        .array_new_default(T_BIGINT)
        .local_set(R);
    s.local_get(R).i32_const(0).local_get(A).i32_const(0);
    s.local_get(A).array_len();
    s.array_copy(T_BIGINT, T_BIGINT);
    s.local_get(R).i32_const(0);
    s.local_get(A)
        .i32_const(0)
        .array_get(T_BIGINT)
        .i32_const(1)
        .i32_xor();
    s.array_set(T_BIGINT);
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$bigint_mul(a, b)`: schoolbook.
fn mul(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    const A: u32 = 0;
    const B: u32 = 1;
    const LA: u32 = 2;
    const LB: u32 = 3;
    const I: u32 = 4;
    const J: u32 = 5;
    const K: u32 = 6;
    const X: u32 = 7;
    const T: u32 = 8;
    const R: u32 = 9;
    let mut f = Function::new([(5, ValType::I32), (2, ValType::I64), (1, nbig())]);
    let mut s = f.instructions();
    limbs(&mut s, A);
    s.local_tee(LA).i32_eqz();
    limbs(&mut s, B);
    s.local_tee(LB).i32_eqz().i32_or();
    s.if_(BlockType::Empty);
    push_small(&mut s, 0);
    s.return_();
    s.end();
    s.local_get(LA)
        .local_get(LB)
        .i32_add()
        .i32_const(MAX_LIMBS)
        .i32_gt_u();
    s.if_(BlockType::Empty);
    throw_size(cx, &mut s);
    s.end();
    s.local_get(LA)
        .local_get(LB)
        .i32_add()
        .i32_const(1)
        .i32_add();
    s.array_new_default(T_BIGINT).local_set(R);
    s.local_get(R).i32_const(0);
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.local_get(B).i32_const(0).array_get(T_BIGINT);
    s.i32_xor().array_set(T_BIGINT);
    for_up(&mut s, I, LA, |s| {
        limb(s, A, I);
        s.i64_extend_i32_u().local_set(X);
        s.i64_const(0).local_set(T);
        s.i32_const(0).local_set(J);
        for_up(s, J, LB, |s| {
            // r[i + j + 1] = lo(t), t = x * b_j + r[i + j + 1] + hi(t)
            s.local_get(I)
                .local_get(J)
                .i32_add()
                .i32_const(1)
                .i32_add()
                .local_set(K);
            s.local_get(X);
            limb(s, B, J);
            s.i64_extend_i32_u().i64_mul();
            s.local_get(R)
                .local_get(K)
                .array_get(T_BIGINT)
                .i64_extend_i32_u();
            s.i64_add();
            s.local_get(T)
                .i64_const(32)
                .i64_shr_u()
                .i64_add()
                .local_set(T);
            s.local_get(R).local_get(K).local_get(T).i32_wrap_i64();
            s.array_set(T_BIGINT);
        });
        s.local_get(R)
            .local_get(I)
            .local_get(LB)
            .i32_add()
            .i32_const(1)
            .i32_add();
        s.local_get(T).i64_const(32).i64_shr_u().i32_wrap_i64();
        s.array_set(T_BIGINT);
    });
    s.local_get(R).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_div(a, b)` / `$bigint_rem(a, b)`: `b` isn't zero. Truncated division: the quotient's
/// sign is the product of the signs, the remainder's the dividend's.
fn div_mod(cx: &mut dyn Cx, rem: bool) -> Function {
    let norm = bh(cx, Helper::Norm);
    let mag_cmp = bh(cx, Helper::MagCmp);
    const A: u32 = 0;
    const B: u32 = 1;
    const M: u32 = 2; // limbs of a
    const N: u32 = 3; // limbs of b
    const I: u32 = 4;
    const J: u32 = 5;
    const SH: u32 = 6; // normalization shift
    const T: u32 = 7; // i64
    const D: u32 = 8; // i64
    const QHAT: u32 = 9; // i64
    const RHAT: u32 = 10; // i64
    const K: u32 = 11; // i64
    const P: u32 = 12; // i64
    const VTOP: u32 = 13; // i64
    const VNEXT: u32 = 14; // i64
    const Q: u32 = 15; // bigint
    const UN: u32 = 16; // i32s
    const VN: u32 = 17; // i32s
    let mut f = Function::new([
        (5, ValType::I32),
        (8, ValType::I64),
        (1, nbig()),
        (2, rt(true, T_I32S)),
    ]);
    let mut s = f.instructions();
    s.local_get(A)
        .local_get(B)
        .call(mag_cmp)
        .i32_const(0)
        .i32_lt_s();
    s.if_(BlockType::Empty);
    if rem {
        s.local_get(A).return_();
    } else {
        push_small(&mut s, 0);
        s.return_();
    }
    s.end();
    limbs(&mut s, A);
    s.local_set(M);
    limbs(&mut s, B);
    s.local_set(N);
    // Q: the quotient, sign and m - n + 1 limbs.
    s.local_get(M).local_get(N).i32_sub().i32_const(2).i32_add();
    s.array_new_default(T_BIGINT).local_set(Q);
    s.local_get(Q).i32_const(0);
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.local_get(B).i32_const(0).array_get(T_BIGINT);
    s.i32_xor().array_set(T_BIGINT);
    // One-limb divisor.
    s.local_get(N).i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    {
        s.local_get(B)
            .i32_const(1)
            .array_get(T_BIGINT)
            .i64_extend_i32_u()
            .local_set(D);
        s.local_get(M).local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(I).i32_eqz().br_if(1);
        s.local_get(I).i32_const(1).i32_sub().local_set(I);
        // t = (r << 32) | a_i
        s.local_get(K).i64_const(32).i64_shl();
        limb(&mut s, A, I);
        s.i64_extend_i32_u().i64_or().local_set(T);
        s.local_get(Q).local_get(I).i32_const(1).i32_add();
        s.local_get(T).local_get(D).i64_div_u().i32_wrap_i64();
        s.array_set(T_BIGINT);
        s.local_get(T).local_get(D).i64_rem_u().local_set(K);
        s.br(0).end().end();
        if rem {
            s.local_get(A).i32_const(0).array_get(T_BIGINT);
            s.local_get(K).i32_wrap_i64();
            s.array_new_fixed(T_BIGINT, 2).call(norm).return_();
        } else {
            s.local_get(Q).ref_as_non_null().call(norm).return_();
        }
    }
    s.end();
    // Knuth D. Normalize: shift so the divisor's top bit is set.
    s.local_get(B)
        .local_get(N)
        .array_get(T_BIGINT)
        .i32_clz()
        .local_set(SH);
    // vn[i] = hi32(((b_i << 32) | b_{i-1}) << sh)
    s.local_get(N).array_new_default(T_I32S).local_set(VN);
    s.i32_const(0).local_set(I);
    for_up(&mut s, I, N, |s| {
        s.local_get(VN).local_get(I);
        limb(s, B, I);
        s.i64_extend_i32_u().i64_const(32).i64_shl();
        s.local_get(I).if_(BlockType::Result(ValType::I64));
        s.local_get(B)
            .local_get(I)
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.else_();
        s.i64_const(0);
        s.end();
        s.i64_or();
        s.local_get(SH).i64_extend_i32_u().i64_shl();
        s.i64_const(32).i64_shr_u().i32_wrap_i64();
        s.array_set(T_I32S);
    });
    // un: m + 1 limbs, the same shift.
    s.local_get(M)
        .i32_const(1)
        .i32_add()
        .array_new_default(T_I32S)
        .local_set(UN);
    s.i32_const(0).local_set(I);
    for_up(&mut s, I, M, |s| {
        s.local_get(UN).local_get(I);
        limb(s, A, I);
        s.i64_extend_i32_u().i64_const(32).i64_shl();
        s.local_get(I).if_(BlockType::Result(ValType::I64));
        s.local_get(A)
            .local_get(I)
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.else_();
        s.i64_const(0);
        s.end();
        s.i64_or();
        s.local_get(SH).i64_extend_i32_u().i64_shl();
        s.i64_const(32).i64_shr_u().i32_wrap_i64();
        s.array_set(T_I32S);
    });
    s.local_get(UN).local_get(M);
    s.local_get(A)
        .local_get(M)
        .array_get(T_BIGINT)
        .i64_extend_i32_u();
    s.local_get(SH).i64_extend_i32_u().i64_shl();
    s.i64_const(32).i64_shr_u().i32_wrap_i64();
    s.array_set(T_I32S);
    let get = |s: &mut InstructionSink, arr: u32, idx: &dyn Fn(&mut InstructionSink)| {
        s.local_get(arr);
        idx(s);
        s.array_get(T_I32S).i64_extend_i32_u();
    };
    get(&mut s, VN, &|s| {
        s.local_get(N).i32_const(1).i32_sub();
    });
    s.local_set(VTOP);
    get(&mut s, VN, &|s| {
        s.local_get(N).i32_const(2).i32_sub();
    });
    s.local_set(VNEXT);
    // for j = m - n down to 0
    s.local_get(M).local_get(N).i32_sub().local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        // num = (un[j+n] << 32) | un[j+n-1]
        get(&mut s, UN, &|s| {
            s.local_get(J).local_get(N).i32_add();
        });
        s.i64_const(32).i64_shl();
        get(&mut s, UN, &|s| {
            s.local_get(J).local_get(N).i32_add().i32_const(1).i32_sub();
        });
        s.i64_or().local_tee(T);
        s.local_get(VTOP).i64_div_u().local_set(QHAT);
        s.local_get(T).local_get(VTOP).i64_rem_u().local_set(RHAT);
        // while qhat ≥ 2^32 or qhat·vn[n-2] > (rhat << 32 | un[j+n-2]): qhat--, rhat += vtop
        // (stop once rhat ≥ 2^32)
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(QHAT).i64_const(1 << 32).i64_ge_u();
        s.if_(BlockType::Result(ValType::I32));
        s.i32_const(1);
        s.else_();
        s.local_get(QHAT).local_get(VNEXT).i64_mul();
        s.local_get(RHAT).i64_const(32).i64_shl();
        get(&mut s, UN, &|s| {
            s.local_get(J).local_get(N).i32_add().i32_const(2).i32_sub();
        });
        s.i64_or().i64_gt_u();
        s.end();
        s.i32_eqz().br_if(1);
        s.local_get(QHAT).i64_const(1).i64_sub().local_set(QHAT);
        s.local_get(RHAT).local_get(VTOP).i64_add().local_tee(RHAT);
        s.i64_const(1 << 32).i64_lt_u().br_if(0);
        s.end().end();
        // Multiply and subtract: un[j..j+n] -= qhat · vn.
        s.i64_const(0).local_set(K);
        s.i32_const(0).local_set(I);
        for_up(&mut s, I, N, |s| {
            s.local_get(QHAT);
            s.local_get(VN)
                .local_get(I)
                .array_get(T_I32S)
                .i64_extend_i32_u();
            s.i64_mul().local_set(P);
            s.local_get(UN)
                .local_get(I)
                .local_get(J)
                .i32_add()
                .array_get(T_I32S);
            s.i64_extend_i32_u();
            s.local_get(K).i64_sub();
            s.local_get(P).i64_const(0xffff_ffff).i64_and().i64_sub();
            s.local_set(T);
            s.local_get(UN).local_get(I).local_get(J).i32_add();
            s.local_get(T).i32_wrap_i64().array_set(T_I32S);
            s.local_get(P).i64_const(32).i64_shr_u();
            s.local_get(T).i64_const(32).i64_shr_s();
            s.i64_sub().local_set(K);
        });
        get(&mut s, UN, &|s| {
            s.local_get(J).local_get(N).i32_add();
        });
        s.local_get(K).i64_sub().local_set(T);
        s.local_get(UN).local_get(J).local_get(N).i32_add();
        s.local_get(T).i32_wrap_i64().array_set(T_I32S);
        // Subtracted too much: add back.
        s.local_get(T).i64_const(0).i64_lt_s();
        s.if_(BlockType::Empty);
        {
            s.local_get(QHAT).i64_const(1).i64_sub().local_set(QHAT);
            s.i64_const(0).local_set(K);
            s.i32_const(0).local_set(I);
            for_up(&mut s, I, N, |s| {
                s.local_get(UN)
                    .local_get(I)
                    .local_get(J)
                    .i32_add()
                    .array_get(T_I32S);
                s.i64_extend_i32_u();
                s.local_get(VN)
                    .local_get(I)
                    .array_get(T_I32S)
                    .i64_extend_i32_u();
                s.i64_add().local_get(K).i64_add().local_set(T);
                s.local_get(UN).local_get(I).local_get(J).i32_add();
                s.local_get(T).i32_wrap_i64().array_set(T_I32S);
                s.local_get(T).i64_const(32).i64_shr_u().local_set(K);
            });
            s.local_get(UN).local_get(J).local_get(N).i32_add();
            get(&mut s, UN, &|s| {
                s.local_get(J).local_get(N).i32_add();
            });
            s.local_get(K).i64_add().i32_wrap_i64();
            s.array_set(T_I32S);
        }
        s.end();
        s.local_get(Q).local_get(J).i32_const(1).i32_add();
        s.local_get(QHAT).i32_wrap_i64().array_set(T_BIGINT);
        s.local_get(J).i32_eqz().br_if(1);
        s.local_get(J).i32_const(1).i32_sub().local_set(J);
        s.br(0);
    }
    s.end().end();
    if rem {
        // r_i = lo32(((un[i+1] << 32) | un[i]) >> sh), i < n.
        s.local_get(N)
            .i32_const(1)
            .i32_add()
            .array_new_default(T_BIGINT)
            .local_set(Q);
        s.local_get(Q).i32_const(0);
        s.local_get(A).i32_const(0).array_get(T_BIGINT);
        s.array_set(T_BIGINT);
        s.i32_const(0).local_set(I);
        for_up(&mut s, I, N, |s| {
            s.local_get(Q).local_get(I).i32_const(1).i32_add();
            s.local_get(UN)
                .local_get(I)
                .i32_const(1)
                .i32_add()
                .array_get(T_I32S);
            s.i64_extend_i32_u().i64_const(32).i64_shl();
            s.local_get(UN)
                .local_get(I)
                .array_get(T_I32S)
                .i64_extend_i32_u();
            s.i64_or();
            s.local_get(SH).i64_extend_i32_u().i64_shr_u();
            s.i32_wrap_i64().array_set(T_BIGINT);
        });
    }
    s.local_get(Q).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_pow(a, b)`: `b` ≥ 0. Square and multiply.
fn pow(cx: &mut dyn Cx) -> Function {
    let mul = bh(cx, Helper::Mul);
    const A: u32 = 0;
    const B: u32 = 1;
    const E: u32 = 2;
    const BITS: u32 = 3;
    const R: u32 = 4;
    let mut f = Function::new([(2, ValType::I32), (1, nbig())]);
    let mut s = f.instructions();
    // b == 0: 1n. a == 0: 0n.
    s.local_get(B).array_len().i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    push_small(&mut s, 1);
    s.return_();
    s.end();
    s.local_get(A).array_len().i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(A).return_();
    s.end();
    // |a| == 1: ±1n (negative for an odd exponent).
    s.local_get(A).array_len().i32_const(2).i32_eq();
    s.local_get(A)
        .i32_const(1)
        .array_get(T_BIGINT)
        .i32_const(1)
        .i32_eq();
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.local_get(B).i32_const(1).array_get(T_BIGINT);
    s.i32_and();
    s.if_(BlockType::Result(big()));
    s.local_get(A);
    s.else_();
    push_small(&mut s, 1);
    s.end();
    s.return_();
    s.end();
    // Too big: an exponent ≥ 2^32, or (bits(a) - 1) · e ≥ 2^30.
    s.local_get(B).array_len().i32_const(2).i32_gt_u();
    s.if_(BlockType::Empty);
    throw_size(cx, &mut s);
    s.end();
    s.local_get(B).i32_const(1).array_get(T_BIGINT).local_set(E);
    // bits(a) - 1 = 32 · (limbs - 1) + 31 - clz(top)
    s.local_get(A)
        .array_len()
        .i32_const(2)
        .i32_sub()
        .i32_const(5)
        .i32_shl();
    s.i32_const(31).i32_add();
    s.local_get(A)
        .local_get(A)
        .array_len()
        .i32_const(1)
        .i32_sub()
        .array_get(T_BIGINT)
        .i32_clz();
    s.i32_sub().local_set(BITS);
    s.local_get(BITS).i64_extend_i32_u();
    s.local_get(E).i64_extend_i32_u().i64_mul();
    s.i64_const(1 << 30).i64_ge_u();
    s.if_(BlockType::Empty);
    throw_size(cx, &mut s);
    s.end();
    push_small(&mut s, 1);
    s.local_set(R);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(E).i32_const(1).i32_and();
    s.if_(BlockType::Empty);
    s.local_get(R)
        .ref_as_non_null()
        .local_get(A)
        .call(mul)
        .local_set(R);
    s.end();
    s.local_get(E)
        .i32_const(1)
        .i32_shr_u()
        .local_tee(E)
        .i32_eqz()
        .br_if(1);
    s.local_get(A).local_get(A).call(mul).local_set(A);
    s.br(0).end().end();
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$bigint_shl(a, k)`.
fn shl(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    const A: u32 = 0;
    const K: u32 = 1;
    const LA: u32 = 2;
    const W: u32 = 3; // whole limbs
    const I: u32 = 4;
    const V: u32 = 5; // i64
    const R: u32 = 6;
    let mut f = Function::new([(3, ValType::I32), (1, ValType::I64), (1, nbig())]);
    let mut s = f.instructions();
    s.local_get(A).array_len().i32_const(1).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(A).return_();
    s.end();
    limbs(&mut s, A);
    s.local_set(LA);
    s.local_get(K).i32_const(5).i32_shr_u().local_set(W);
    s.local_get(LA)
        .local_get(W)
        .i32_add()
        .i32_const(MAX_LIMBS)
        .i32_gt_u();
    s.if_(BlockType::Empty);
    throw_size(cx, &mut s);
    s.end();
    s.local_get(LA)
        .local_get(W)
        .i32_add()
        .i32_const(2)
        .i32_add();
    s.array_new_default(T_BIGINT).local_set(R);
    s.local_get(R).i32_const(0);
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.array_set(T_BIGINT);
    for_up(&mut s, I, LA, |s| {
        limb(s, A, I);
        s.i64_extend_i32_u();
        s.local_get(K).i32_const(31).i32_and().i64_extend_i32_u();
        s.i64_shl().local_set(V);
        // r[i + w + 1] |= lo(v); r[i + w + 2] = hi(v)
        s.local_get(R)
            .local_get(I)
            .local_get(W)
            .i32_add()
            .i32_const(1)
            .i32_add();
        s.local_get(R)
            .local_get(I)
            .local_get(W)
            .i32_add()
            .i32_const(1)
            .i32_add()
            .array_get(T_BIGINT);
        s.local_get(V).i32_wrap_i64().i32_or();
        s.array_set(T_BIGINT);
        s.local_get(R)
            .local_get(I)
            .local_get(W)
            .i32_add()
            .i32_const(2)
            .i32_add();
        s.local_get(V).i64_const(32).i64_shr_u().i32_wrap_i64();
        s.array_set(T_BIGINT);
    });
    s.local_get(R).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_shr(a, k)`: floor division by 2^k (a negative value rounds down).
fn shr(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    let mag_add = bh(cx, Helper::MagAdd);
    const A: u32 = 0;
    const K: u32 = 1;
    const LA: u32 = 2;
    const W: u32 = 3;
    const I: u32 = 4;
    const NL: u32 = 5;
    const LOST: u32 = 6;
    const R: u32 = 7;
    let mut f = Function::new([(5, ValType::I32), (1, nbig())]);
    let mut s = f.instructions();
    limbs(&mut s, A);
    s.local_set(LA);
    s.local_get(K).i32_const(5).i32_shr_u().local_tee(W);
    s.local_get(LA).i32_ge_u();
    s.if_(BlockType::Empty);
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.if_(BlockType::Result(big()));
    push_small(&mut s, -1);
    s.else_();
    push_small(&mut s, 0);
    s.end();
    s.return_();
    s.end();
    s.local_get(LA).local_get(W).i32_sub().local_set(NL);
    s.local_get(NL)
        .i32_const(1)
        .i32_add()
        .array_new_default(T_BIGINT)
        .local_set(R);
    s.local_get(R).i32_const(0);
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.array_set(T_BIGINT);
    for_up(&mut s, I, NL, |s| {
        // r_i = lo(((a_{i+w+1} << 32) | a_{i+w}) >> (k & 31))
        s.local_get(R).local_get(I).i32_const(1).i32_add();
        s.local_get(I).local_get(W).i32_add().i32_const(1).i32_add();
        s.local_get(LA).i32_lt_u();
        s.if_(BlockType::Result(ValType::I64));
        s.local_get(A)
            .local_get(I)
            .local_get(W)
            .i32_add()
            .i32_const(2)
            .i32_add()
            .array_get(T_BIGINT);
        s.i64_extend_i32_u().i64_const(32).i64_shl();
        s.else_();
        s.i64_const(0);
        s.end();
        s.local_get(A)
            .local_get(I)
            .local_get(W)
            .i32_add()
            .i32_const(1)
            .i32_add()
            .array_get(T_BIGINT);
        s.i64_extend_i32_u().i64_or();
        s.local_get(K).i32_const(31).i32_and().i64_extend_i32_u();
        s.i64_shr_u().i32_wrap_i64();
        s.array_set(T_BIGINT);
    });
    // A negative value that lost bits rounds down: magnitude + 1.
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.if_(BlockType::Empty);
    {
        // lost = a_w & ((1 << (k & 31)) - 1), or any lower limb
        s.local_get(A)
            .local_get(W)
            .i32_const(1)
            .i32_add()
            .array_get(T_BIGINT);
        s.i32_const(1)
            .local_get(K)
            .i32_const(31)
            .i32_and()
            .i32_shl()
            .i32_const(1)
            .i32_sub();
        s.i32_and().local_set(LOST);
        s.i32_const(0).local_set(I);
        for_up(&mut s, I, W, |s| {
            limb(s, A, I);
            s.local_get(LOST).i32_or().local_set(LOST);
        });
        s.local_get(LOST);
        s.if_(BlockType::Empty);
        s.local_get(R).ref_as_non_null();
        push_small(&mut s, 1);
        s.i32_const(1).call(mag_add).return_();
        s.end();
    }
    s.end();
    s.local_get(R).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_shift(a, b, right)`: `a << b` (`right` 0) or `a >> b`; a negative `b` shifts the
/// other way.
fn shift(cx: &mut dyn Cx) -> Function {
    let shl = bh(cx, Helper::Shl);
    let shr = bh(cx, Helper::Shr);
    const A: u32 = 0;
    const B: u32 = 1;
    const RIGHT: u32 = 2;
    const K: u32 = 3;
    const HUGE: u32 = 4;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    // |b| ≥ 2^30: huge.
    s.local_get(B).array_len().i32_const(2).i32_gt_u();
    s.if_(BlockType::Empty);
    s.i32_const(1).local_set(HUGE);
    s.else_();
    s.local_get(B).array_len().i32_const(2).i32_eq();
    s.if_(BlockType::Empty);
    s.local_get(B).i32_const(1).array_get(T_BIGINT).local_tee(K);
    s.i32_const(1 << 30).i32_ge_u().local_set(HUGE);
    s.end();
    s.end();
    s.local_get(RIGHT);
    s.local_get(B).i32_const(0).array_get(T_BIGINT);
    s.i32_xor();
    s.if_(BlockType::Empty);
    s.local_get(A);
    s.i32_const(i32::MAX).local_get(K).local_get(HUGE).select();
    s.call(shr).return_();
    s.end();
    s.local_get(HUGE)
        .local_get(A)
        .array_len()
        .i32_const(1)
        .i32_gt_u()
        .i32_and();
    s.if_(BlockType::Empty);
    throw_size(cx, &mut s);
    s.end();
    s.local_get(A).local_get(K).call(shl);
    s.end();
    f
}

/// `$bigint_to_tc(a, len)`: `len` limbs of a's two's complement (`len` > limbs of `a`).
fn to_tc() -> Function {
    const A: u32 = 0;
    const L: u32 = 1;
    const LA: u32 = 2;
    const I: u32 = 3;
    const NEG: u32 = 4;
    const V: u32 = 5;
    const T: u32 = 6; // i64
    const R: u32 = 7;
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64), (1, rt(true, T_I32S))]);
    let mut s = f.instructions();
    limbs(&mut s, A);
    s.local_set(LA);
    s.local_get(A)
        .i32_const(0)
        .array_get(T_BIGINT)
        .local_set(NEG);
    s.local_get(L).array_new_default(T_I32S).local_set(R);
    // t's high half is the carry of ~m + 1.
    s.i64_const(1 << 32).local_set(T);
    for_up(&mut s, I, L, |s| {
        s.local_get(I).local_get(LA).i32_lt_s();
        s.if_(BlockType::Result(ValType::I32));
        limb(s, A, I);
        s.else_();
        s.i32_const(0);
        s.end();
        s.local_set(V);
        s.local_get(NEG);
        s.if_(BlockType::Empty);
        s.local_get(V).i32_const(-1).i32_xor().i64_extend_i32_u();
        s.local_get(T)
            .i64_const(32)
            .i64_shr_u()
            .i64_add()
            .local_tee(T);
        s.i32_wrap_i64().local_set(V);
        s.end();
        s.local_get(R).local_get(I).local_get(V).array_set(T_I32S);
    });
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

/// `$bigint_from_tc(x)`.
fn from_tc(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    const X: u32 = 0;
    const L: u32 = 1;
    const I: u32 = 2;
    const NEG: u32 = 3;
    const V: u32 = 4;
    const T: u32 = 5; // i64
    const R: u32 = 6;
    let mut f = Function::new([(4, ValType::I32), (1, ValType::I64), (1, nbig())]);
    let mut s = f.instructions();
    s.local_get(X).array_len().local_set(L);
    s.local_get(X)
        .local_get(L)
        .i32_const(1)
        .i32_sub()
        .array_get(T_I32S)
        .i32_const(31)
        .i32_shr_u()
        .local_set(NEG);
    s.local_get(L)
        .i32_const(1)
        .i32_add()
        .array_new_default(T_BIGINT)
        .local_set(R);
    s.local_get(R)
        .i32_const(0)
        .local_get(NEG)
        .array_set(T_BIGINT);
    s.i64_const(1 << 32).local_set(T);
    for_up(&mut s, I, L, |s| {
        s.local_get(X).local_get(I).array_get(T_I32S).local_set(V);
        s.local_get(NEG);
        s.if_(BlockType::Empty);
        s.local_get(V).i32_const(-1).i32_xor().i64_extend_i32_u();
        s.local_get(T)
            .i64_const(32)
            .i64_shr_u()
            .i64_add()
            .local_tee(T);
        s.i32_wrap_i64().local_set(V);
        s.end();
        s.local_get(R)
            .local_get(I)
            .i32_const(1)
            .i32_add()
            .local_get(V);
        s.array_set(T_BIGINT);
    });
    s.local_get(R).ref_as_non_null().call(norm);
    s.end();
    f
}

/// `$bigint_bitwise(a, b)`.
fn bitwise(cx: &mut dyn Cx, op: u8) -> Function {
    let to_tc = bh(cx, Helper::ToTc);
    let from_tc = bh(cx, Helper::FromTc);
    const A: u32 = 0;
    const B: u32 = 1;
    const L: u32 = 2;
    const I: u32 = 3;
    const X: u32 = 4;
    const Y: u32 = 5;
    let i32s = rt(true, T_I32S);
    let mut f = Function::new([(2, ValType::I32), (2, i32s)]);
    let mut s = f.instructions();
    // len = max(limbs) + 1
    s.local_get(A).array_len().local_get(B).array_len();
    s.local_get(A)
        .array_len()
        .local_get(B)
        .array_len()
        .i32_gt_u();
    s.select().local_set(L);
    s.local_get(A).local_get(L).call(to_tc).local_set(X);
    s.local_get(B).local_get(L).call(to_tc).local_set(Y);
    for_up(&mut s, I, L, |s| {
        s.local_get(X).local_get(I);
        s.local_get(X).local_get(I).array_get(T_I32S);
        s.local_get(Y).local_get(I).array_get(T_I32S);
        match op {
            0 => s.i32_and(),
            1 => s.i32_or(),
            _ => s.i32_xor(),
        };
        s.array_set(T_I32S);
    });
    s.local_get(X).ref_as_non_null().call(from_tc);
    s.end();
    f
}

/// `$bigint_from_f64(x)`: `x` is integral.
fn from_f64(cx: &mut dyn Cx) -> Function {
    let norm = bh(cx, Helper::Norm);
    let shl = bh(cx, Helper::Shl);
    const X: u32 = 0;
    const NEG: u32 = 1;
    const U: u32 = 2; // i64
    let mut f = Function::new([(1, ValType::I32), (1, ValType::I64)]);
    let mut s = f.instructions();
    s.local_get(X).f64_const(0.0.into()).f64_lt().local_set(NEG);
    s.local_get(X).f64_abs().local_tee(X);
    s.f64_const(18446744073709551616.0.into()).f64_lt();
    s.if_(BlockType::Empty);
    s.local_get(X).i64_trunc_f64_u().local_set(U);
    s.local_get(NEG);
    s.local_get(U).i32_wrap_i64();
    s.local_get(U).i64_const(32).i64_shr_u().i32_wrap_i64();
    s.array_new_fixed(T_BIGINT, 3).call(norm).return_();
    s.end();
    // x ≥ 2^64: mantissa (53 bits) << (exponent - 52)
    s.local_get(X).i64_reinterpret_f64().local_set(U);
    s.local_get(NEG);
    s.local_get(U)
        .i64_const((1 << 52) - 1)
        .i64_and()
        .i64_const(1 << 52)
        .i64_or()
        .local_tee(U);
    s.i32_wrap_i64();
    s.local_get(U).i64_const(32).i64_shr_u().i32_wrap_i64();
    s.array_new_fixed(T_BIGINT, 3);
    s.local_get(X)
        .i64_reinterpret_f64()
        .i64_const(52)
        .i64_shr_u();
    s.i32_wrap_i64()
        .i32_const(0x7ff)
        .i32_and()
        .i32_const(1075)
        .i32_sub();
    s.call(shl);
    s.end();
    f
}

/// `$bigint_to_f64(a)`: the top 64 bits with a sticky bit for the rest, converted (one rounding),
/// then scaled by a power of two.
fn to_f64() -> Function {
    const A: u32 = 0;
    const LA: u32 = 1;
    const Z: u32 = 2;
    const I: u32 = 3;
    const STICKY: u32 = 4;
    const M: u32 = 5; // i64
    const E: u32 = 6;
    const R: u32 = 7; // f64
    let mut f = Function::new([
        (4, ValType::I32),
        (1, ValType::I64),
        (1, ValType::I32),
        (1, ValType::F64),
    ]);
    let mut s = f.instructions();
    limbs(&mut s, A);
    s.local_tee(LA).i32_const(2).i32_le_u();
    s.if_(BlockType::Empty);
    {
        s.local_get(LA).i32_const(1).i32_ge_u();
        s.if_(BlockType::Result(ValType::I64));
        s.local_get(A)
            .i32_const(1)
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.else_();
        s.i64_const(0);
        s.end();
        s.local_get(LA).i32_const(2).i32_eq();
        s.if_(BlockType::Result(ValType::I64));
        s.local_get(A)
            .i32_const(2)
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.i64_const(32).i64_shl();
        s.else_();
        s.i64_const(0);
        s.end();
        s.i64_or().f64_convert_i64_u().local_set(R);
    }
    s.else_();
    {
        // top limb's leading zeros
        s.local_get(A)
            .local_get(LA)
            .array_get(T_BIGINT)
            .i32_clz()
            .local_set(Z);
        // m = ((a_{la-1} << 32 | a_{la-2}) << z) | (a_{la-3} >> (32 - z))
        s.local_get(A)
            .local_get(LA)
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.i64_const(32).i64_shl();
        s.local_get(A)
            .local_get(LA)
            .i32_const(1)
            .i32_sub()
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.i64_or();
        s.local_get(Z).i64_extend_i32_u().i64_shl();
        s.local_get(A)
            .local_get(LA)
            .i32_const(2)
            .i32_sub()
            .array_get(T_BIGINT)
            .i64_extend_i32_u();
        s.i64_const(32)
            .local_get(Z)
            .i64_extend_i32_u()
            .i64_sub()
            .i64_shr_u();
        s.i64_or().local_set(M);
        // sticky: the bits of a_{la-3} below those, and every lower limb
        s.local_get(A)
            .local_get(LA)
            .i32_const(2)
            .i32_sub()
            .array_get(T_BIGINT);
        s.local_get(Z).i32_shl().local_set(STICKY);
        s.i32_const(1).local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(I)
            .local_get(LA)
            .i32_const(2)
            .i32_sub()
            .i32_ge_s()
            .br_if(1);
        s.local_get(A).local_get(I).array_get(T_BIGINT);
        s.local_get(STICKY).i32_or().local_set(STICKY);
        s.local_get(I).i32_const(1).i32_add().local_set(I);
        s.br(0).end().end();
        s.local_get(M);
        s.local_get(STICKY).i32_const(0).i32_ne().i64_extend_i32_u();
        s.i64_or().f64_convert_i64_u().local_set(R);
        // e = bits - 64 = 32·la - z - 64
        s.local_get(LA)
            .i32_const(5)
            .i32_shl()
            .local_get(Z)
            .i32_sub();
        s.i32_const(64).i32_sub().local_tee(E);
        s.i32_const(1023).i32_gt_s();
        s.if_(BlockType::Result(ValType::F64));
        s.f64_const(f64::INFINITY.into());
        s.else_();
        s.local_get(R);
        s.local_get(E)
            .i32_const(1023)
            .i32_add()
            .i64_extend_i32_u()
            .i64_const(52)
            .i64_shl();
        s.f64_reinterpret_i64().f64_mul();
        s.end();
        s.local_set(R);
    }
    s.end();
    s.local_get(R).f64_neg().local_get(R);
    s.local_get(A).i32_const(0).array_get(T_BIGINT).select();
    s.end();
    f
}

/// `$bigint_cmp_number(a, x)`.
fn cmp_num(cx: &mut dyn Cx) -> Function {
    let cmp = bh(cx, Helper::Cmp);
    let from_f64 = bh(cx, Helper::FromF64);
    const A: u32 = 0;
    const X: u32 = 1;
    const C: u32 = 2;
    const T: u32 = 3; // f64
    let mut f = Function::new([(1, ValType::I32), (1, ValType::F64)]);
    let mut s = f.instructions();
    s.local_get(X).local_get(X).f64_ne();
    s.if_(BlockType::Empty);
    s.i32_const(2).return_();
    s.end();
    s.local_get(X)
        .f64_abs()
        .f64_const(f64::INFINITY.into())
        .f64_eq();
    s.if_(BlockType::Empty);
    s.i32_const(-1).i32_const(1);
    s.local_get(X)
        .f64_const(0.0.into())
        .f64_gt()
        .select()
        .return_();
    s.end();
    s.local_get(A);
    s.local_get(X).f64_trunc().local_tee(T).call(from_f64);
    s.call(cmp).local_tee(C);
    s.if_(BlockType::Empty);
    s.local_get(C).return_();
    s.end();
    // a == trunc(x): the fraction decides.
    s.local_get(X).local_get(T).f64_sub().local_tee(T);
    s.f64_const(0.0.into()).f64_gt();
    s.if_(BlockType::Result(ValType::I32));
    s.i32_const(-1);
    s.else_();
    s.local_get(T).f64_const(0.0.into()).f64_lt();
    s.end();
    s.end();
    f
}

/// `$bigint_to_string(a, radix)`: divide by radix^k (the largest below 2^32) and emit k digits
/// per step.
fn to_string(cx: &mut dyn Cx) -> Function {
    let zero = cx.str("0");
    const A: u32 = 0;
    const RADIX: u32 = 1;
    const N: u32 = 2;
    const I: u32 = 3;
    const K: u32 = 4;
    const C: u32 = 5;
    const POS: u32 = 6;
    const DIG: u32 = 7;
    const D: u32 = 8; // i64
    const REM: u32 = 9; // i64
    const T: u32 = 10; // i64
    const MAG: u32 = 11; // i32s
    const BUF: u32 = 12; // string
    const OUT: u32 = 13; // string
    let mut f = Function::new([
        (6, ValType::I32),
        (3, ValType::I64),
        (1, rt(true, T_I32S)),
        (2, rt(true, T_STRING)),
    ]);
    let mut s = f.instructions();
    limbs(&mut s, A);
    s.local_tee(N).i32_eqz();
    s.if_(BlockType::Empty);
    s.global_get(zero).return_();
    s.end();
    // d = radix^k < 2^32
    s.local_get(RADIX).i64_extend_i32_u().local_set(D);
    s.i32_const(1).local_set(K);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(D).local_get(RADIX).i64_extend_i32_u().i64_mul();
    s.i64_const(1 << 32).i64_ge_u().br_if(1);
    s.local_get(D)
        .local_get(RADIX)
        .i64_extend_i32_u()
        .i64_mul()
        .local_set(D);
    s.local_get(K).i32_const(1).i32_add().local_set(K);
    s.br(0).end().end();
    // mag = a copy of the limbs
    s.local_get(N).array_new_default(T_I32S).local_set(MAG);
    s.local_get(MAG)
        .i32_const(0)
        .local_get(A)
        .i32_const(1)
        .local_get(N);
    s.array_copy(T_I32S, T_BIGINT);
    s.local_get(N).i32_const(5).i32_shl().i32_const(2).i32_add();
    s.local_tee(POS).array_new_default(T_STRING).local_set(BUF);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    {
        // mag /= d, rem = mag % d
        s.i64_const(0).local_set(REM);
        s.local_get(N).local_set(I);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(I).i32_eqz().br_if(1);
        s.local_get(I).i32_const(1).i32_sub().local_set(I);
        s.local_get(REM).i64_const(32).i64_shl();
        s.local_get(MAG)
            .local_get(I)
            .array_get(T_I32S)
            .i64_extend_i32_u();
        s.i64_or().local_set(T);
        s.local_get(MAG).local_get(I);
        s.local_get(T).local_get(D).i64_div_u().i32_wrap_i64();
        s.array_set(T_I32S);
        s.local_get(T).local_get(D).i64_rem_u().local_set(REM);
        s.br(0).end().end();
        // drop zero top limbs
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(N).i32_eqz().br_if(1);
        s.local_get(MAG)
            .local_get(N)
            .i32_const(1)
            .i32_sub()
            .array_get(T_I32S)
            .br_if(1);
        s.local_get(N).i32_const(1).i32_sub().local_set(N);
        s.br(0).end().end();
        // k digits (fewer for the top chunk: no leading zeros)
        s.i32_const(0).local_set(C);
        s.block(BlockType::Empty).loop_(BlockType::Empty);
        s.local_get(C).local_get(K).i32_ge_u().br_if(1);
        s.local_get(N).i32_eqz();
        s.local_get(REM).i64_eqz().i32_and().br_if(1);
        s.local_get(REM)
            .local_get(RADIX)
            .i64_extend_i32_u()
            .i64_rem_u()
            .i32_wrap_i64()
            .local_set(DIG);
        s.local_get(REM)
            .local_get(RADIX)
            .i64_extend_i32_u()
            .i64_div_u()
            .local_set(REM);
        s.local_get(POS).i32_const(1).i32_sub().local_set(POS);
        s.local_get(BUF).local_get(POS);
        s.local_get(DIG).i32_const(48).i32_add();
        s.local_get(DIG).i32_const(87).i32_add();
        s.local_get(DIG).i32_const(10).i32_lt_u().select();
        s.array_set(T_STRING);
        s.local_get(C).i32_const(1).i32_add().local_set(C);
        s.br(0).end().end();
        s.local_get(N).br_if(0);
    }
    s.end().end();
    s.local_get(A).i32_const(0).array_get(T_BIGINT);
    s.if_(BlockType::Empty);
    s.local_get(POS).i32_const(1).i32_sub().local_set(POS);
    s.local_get(BUF)
        .local_get(POS)
        .i32_const(45)
        .array_set(T_STRING);
    s.end();
    // the digits: buf[pos..]
    s.local_get(BUF)
        .array_len()
        .local_get(POS)
        .i32_sub()
        .local_tee(N);
    s.array_new_default(T_STRING).local_set(OUT);
    s.local_get(OUT)
        .i32_const(0)
        .local_get(BUF)
        .local_get(POS)
        .local_get(N);
    s.array_copy(T_STRING, T_STRING);
    s.local_get(OUT).ref_as_non_null();
    s.end();
    f
}

/// `$bigint_hash(a)`.
fn hash() -> Function {
    const A: u32 = 0;
    const I: u32 = 1;
    const H: u32 = 2;
    let mut f = Function::new([(2, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(A).array_len().local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).i32_eqz().br_if(1);
    s.local_get(I).i32_const(1).i32_sub().local_set(I);
    s.local_get(H).i32_const(31).i32_mul();
    s.local_get(A)
        .local_get(I)
        .array_get(T_BIGINT)
        .i32_add()
        .local_set(H);
    s.br(0).end().end();
    s.local_get(H);
    s.end();
    f
}

/// `$bigint_parse(s)`: the prelude's StringToBigInt; without it (never linked) a trap.
fn parse(cx: &mut dyn Cx) -> Function {
    let f_ = cx.string_to_bigint();
    let mut f = Function::new([]);
    let mut s = f.instructions();
    match f_ {
        Some(p) => s.local_get(0).call(p),
        None => s.unreachable(),
    };
    s.end();
    f
}

/// `$bigint_loose_eq(a, v)`.
fn loose_eq(cx: &mut dyn Cx) -> Function {
    let cmp = bh(cx, Helper::Cmp);
    let cmp_num = bh(cx, Helper::CmpNum);
    let parse = bh(cx, Helper::Parse);
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    const A: u32 = 0;
    const V: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    runtime::is_string(&mut s, V);
    s.if_(BlockType::Empty);
    s.local_get(V).call(parse).local_set(V);
    s.end();
    is_bigint(&mut s, V);
    s.if_(BlockType::Empty);
    s.local_get(A);
    s.local_get(V).ref_cast_non_null(bigint());
    s.call(cmp).i32_eqz().return_();
    s.end();
    // A number or a boolean compares by value; anything else (a failed parse) is unequal.
    s.local_get(V).call(is_number);
    s.local_get(V)
        .ref_test_non_null(HeapType::Concrete(runtime::T_ODDBALL));
    s.i32_or();
    s.if_(BlockType::Empty);
    s.local_get(V).call(runtime::is_nullish_fn(cx)).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(A)
        .local_get(V)
        .call(to_number)
        .call(cmp_num)
        .i32_eqz()
        .return_();
    s.end();
    s.end();
    s.i32_const(0);
    s.end();
    f
}

/// `$bigint_compare(x, y)`.
fn cmp_any(cx: &mut dyn Cx) -> Function {
    let cmp = bh(cx, Helper::Cmp);
    let cmp_num = bh(cx, Helper::CmpNum);
    let parse = bh(cx, Helper::Parse);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    const X: u32 = 0;
    const Y: u32 = 1;
    const C: u32 = 2;
    const N: u32 = 3; // f64
    let mut f = Function::new([(1, ValType::I32), (1, ValType::F64)]);
    let mut s = f.instructions();
    // A string next to a BigInt parses as one; not an integer: unordered.
    for v in [X, Y] {
        runtime::is_string(&mut s, v);
        s.if_(BlockType::Empty);
        s.local_get(v).call(parse).local_tee(v);
        s.ref_test_non_null(bigint()).i32_eqz();
        s.if_(BlockType::Empty);
        s.i32_const(2).return_();
        s.end();
        s.end();
    }
    is_bigint(&mut s, X);
    is_bigint(&mut s, Y);
    s.i32_and();
    s.if_(BlockType::Empty);
    s.local_get(X).ref_cast_non_null(bigint());
    s.local_get(Y).ref_cast_non_null(bigint());
    s.call(cmp).return_();
    s.end();
    is_bigint(&mut s, X);
    s.if_(BlockType::Empty);
    s.local_get(X).ref_cast_non_null(bigint());
    s.local_get(Y).call(to_number).call(cmp_num).return_();
    s.end();
    s.local_get(X).call(to_number).local_set(N);
    s.local_get(Y).ref_cast_non_null(bigint());
    s.local_get(N).call(cmp_num).local_tee(C);
    s.i32_const(0).local_get(C).i32_sub();
    s.local_get(C).i32_const(2).i32_eq().select();
    s.end();
    f
}

/// `$to_numeric(v)`.
fn to_numeric(cx: &mut dyn Cx) -> Function {
    let is_number = cx.helper(runtime::Helper::IsNumber);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let box_ = cx.helper(runtime::Helper::Box);
    let to_primitive = cx.helper(runtime::Helper::Obj(object::Helper::ToPrimitive));
    const V: u32 = 0;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.local_get(V).call(is_number);
    is_bigint(&mut s, V);
    s.i32_or();
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.local_get(V).ref_test_non_null(runtime::object());
    s.if_(BlockType::Empty);
    s.local_get(V)
        .ref_cast_non_null(runtime::object())
        .i32_const(1)
        .call(to_primitive)
        .local_tee(V);
    s.ref_test_non_null(bigint());
    s.if_(BlockType::Empty);
    s.local_get(V).return_();
    s.end();
    s.end();
    s.local_get(V).call(to_number).call(box_);
    s.end();
    f
}

/// `$numeric(a, b)` / `$numeric(v)`: ToNumeric, then the BigInt or the number operation.
fn numeric(cx: &mut dyn Cx, op: NumericOp) -> Function {
    let to_numeric = bh(cx, Helper::ToNumeric);
    let to_number = cx.helper(runtime::Helper::ToNumber);
    let box_ = cx.helper(runtime::Helper::Box);
    const A: u32 = 0;
    const B: u32 = 1;
    let binary = matches!(op, NumericOp::Binary(_));
    // A scratch `f64` after the params.
    let scratch = if binary { 2 } else { 1 };
    let mut f = Function::new([(1, ValType::F64)]);
    let mut s = f.instructions();
    s.local_get(A).call(to_numeric).local_set(A);
    if binary {
        s.local_get(B).call(to_numeric).local_set(B);
        is_bigint(&mut s, A);
        is_bigint(&mut s, B);
        s.i32_or();
        s.if_(BlockType::Empty);
        let NumericOp::Binary(bop) = op else {
            unreachable!()
        };
        let h = bh(cx, Helper::Binary(bop));
        s.local_get(A).local_get(B).call(h).return_();
        s.end();
    } else {
        is_bigint(&mut s, A);
        s.if_(BlockType::Empty);
        s.local_get(A).ref_cast_non_null(bigint());
        match op {
            NumericOp::Neg => {
                let neg = bh(cx, Helper::Neg);
                s.call(neg);
            }
            NumericOp::BitNot => {
                // ~a = -(a + 1)
                let add = bh(cx, Helper::AddS);
                let neg = bh(cx, Helper::Neg);
                push_small(&mut s, 1);
                s.i32_const(0).call(add).call(neg);
            }
            _ => {
                let add = bh(cx, Helper::AddS);
                push_small(&mut s, 1);
                s.i32_const((op == NumericOp::Dec) as i32).call(add);
            }
        }
        s.return_();
        s.end();
    }
    // Numbers.
    s.local_get(A).call(to_number);
    if binary {
        s.local_get(B).call(to_number);
    }
    let int = |cx: &mut dyn Cx, s: &mut InstructionSink| {
        // (x, y) f64 → (x, y) i32
        let to_int32 = cx.to_int32();
        s.local_set(scratch).call(to_int32);
        s.local_get(scratch).call(to_int32);
    };
    match op {
        NumericOp::Binary(bop) => match bop {
            BinOp::Sub => {
                s.f64_sub();
            }
            BinOp::Mul => {
                s.f64_mul();
            }
            BinOp::Div => {
                s.f64_div();
            }
            BinOp::Rem => {
                let rem = cx.js_rem();
                s.call(rem);
            }
            BinOp::Pow => {
                let pow = cx.js_pow();
                s.call(pow);
            }
            BinOp::And | BinOp::Or | BinOp::Xor | BinOp::Shl | BinOp::Shr => {
                int(cx, &mut s);
                match bop {
                    BinOp::And => s.i32_and(),
                    BinOp::Or => s.i32_or(),
                    BinOp::Xor => s.i32_xor(),
                    BinOp::Shl => s.i32_shl(),
                    _ => s.i32_shr_s(),
                };
                s.f64_convert_i32_s();
            }
            BinOp::ShrU => {
                int(cx, &mut s);
                s.i32_shr_u().f64_convert_i32_u();
            }
            _ => unreachable!("not a numeric operator: {bop:?}"),
        },
        NumericOp::Neg => {
            s.f64_neg();
        }
        NumericOp::BitNot => {
            let to_int32 = cx.to_int32();
            s.call(to_int32).i32_const(-1).i32_xor().f64_convert_i32_s();
        }
        NumericOp::Inc => {
            s.f64_const(1.0.into()).f64_add();
        }
        NumericOp::Dec => {
            s.f64_const(1.0.into()).f64_sub();
        }
    }
    s.call(box_);
    s.end();
    f
}

/// `$bigint_binary(a, b)`.
fn binary(cx: &mut dyn Cx, op: BinOp) -> Function {
    const A: u32 = 0;
    const B: u32 = 1;
    const X: u32 = 2;
    const Y: u32 = 3;
    let mut f = Function::new([(2, nbig())]);
    let mut s = f.instructions();
    is_bigint(&mut s, A);
    is_bigint(&mut s, B);
    s.i32_and().i32_eqz();
    s.if_(BlockType::Empty);
    throw_type(
        cx,
        &mut s,
        "Cannot mix BigInt and other types, use explicit conversions",
    );
    s.end();
    s.local_get(A).ref_cast_non_null(bigint()).local_set(X);
    s.local_get(B).ref_cast_non_null(bigint()).local_set(Y);
    let x = |s: &mut InstructionSink| {
        s.local_get(X).ref_as_non_null();
    };
    let y = |s: &mut InstructionSink| {
        s.local_get(Y).ref_as_non_null();
    };
    match op {
        BinOp::Add | BinOp::Sub => {
            let add = bh(cx, Helper::AddS);
            x(&mut s);
            y(&mut s);
            s.local_get(Y).i32_const(0).array_get(T_BIGINT);
            if op == BinOp::Sub {
                s.i32_const(1).i32_xor();
            }
            s.call(add);
        }
        BinOp::Mul => {
            let mul = bh(cx, Helper::Mul);
            x(&mut s);
            y(&mut s);
            s.call(mul);
        }
        BinOp::Div | BinOp::Rem => {
            s.local_get(Y).array_len().i32_const(1).i32_eq();
            s.if_(BlockType::Empty);
            throw_range(cx, &mut s, "Division by zero");
            s.end();
            let h = bh(cx, Helper::DivMod(op == BinOp::Rem));
            x(&mut s);
            y(&mut s);
            s.call(h);
        }
        BinOp::Pow => {
            s.local_get(Y).i32_const(0).array_get(T_BIGINT);
            s.if_(BlockType::Empty);
            // V8's text (node 24).
            throw_range(cx, &mut s, "undefined must be positive");
            s.end();
            let h = bh(cx, Helper::Pow);
            x(&mut s);
            y(&mut s);
            s.call(h);
        }
        BinOp::And | BinOp::Or | BinOp::Xor => {
            let k = match op {
                BinOp::And => 0,
                BinOp::Or => 1,
                _ => 2,
            };
            let h = bh(cx, Helper::Bitwise(k));
            x(&mut s);
            y(&mut s);
            s.call(h);
        }
        BinOp::Shl | BinOp::Shr => {
            let h = bh(cx, Helper::Shift);
            x(&mut s);
            y(&mut s);
            s.i32_const((op == BinOp::Shr) as i32).call(h);
        }
        BinOp::ShrU => {
            throw_type(
                cx,
                &mut s,
                "BigInts have no unsigned right shift, use >> instead",
            );
        }
        _ => unreachable!("not a numeric operator: {op:?}"),
    }
    s.end();
    f
}
