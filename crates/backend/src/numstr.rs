//! Number → string (JS `Number::toString(10)`), emitted as Wasm helpers.
//!
//! The shortest decimal that round-trips, choosing the closest (ties to even), comes from
//! **Ryu** (Ulf Adams, 2018: `d2s.c`, the general path only). The tables `5^q` / `2^k / 5^q`
//! (125-bit splits) are computed here with a small bignum and shipped in a passive data segment,
//! loaded into an `(array i64)` on first use (so no linear memory is needed).
//! The digits are then laid out per ECMA-262 Number::toString: plain integer, decimal point,
//! `0.000ddd`, or exponent form `d.ddde+n`.
//!
//! Verified against node on 30k random doubles plus edge cases (the Rust tests and
//! `tests/fixtures/numstr.js`).

use std::sync::OnceLock;

use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, RefType, ValType};

use crate::runtime::{self, Cx, G_TABLES, T_I64S, T_STRING};

const POW5_INV_BITCOUNT: i32 = 125;
const POW5_BITCOUNT: i32 = 125;
/// Entries in the `2^k / 5^q` table (index q) and the `5^i` table (index i).
const INV_ENTRIES: usize = 342;
const POW_ENTRIES: usize = 326;
/// Offset (in i64s) of the `5^i` table in the data.
const POW_OFFSET: i32 = 2 * INV_ENTRIES as i32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Helper {
    NumberToString,
    D2d,
    MulHi,
    MulShift,
    Pow5Ge,
    DecLen,
    WriteDigits,
    Tables,
}

impl Helper {
    pub fn name(self) -> &'static str {
        match self {
            Helper::NumberToString => "number_to_string",
            Helper::D2d => "ryu_d2d",
            Helper::MulHi => "umul128_hi",
            Helper::MulShift => "ryu_mul_shift",
            Helper::Pow5Ge => "ryu_pow5_ge",
            Helper::DecLen => "decimal_length",
            Helper::WriteDigits => "write_digits",
            Helper::Tables => "ryu_tables",
        }
    }

    pub fn signature(self) -> (Vec<ValType>, Vec<ValType>) {
        use ValType::{F64, I32, I64};
        match self {
            Helper::NumberToString => (vec![F64], vec![runtime::string_ref()]),
            Helper::D2d => (vec![I64], vec![I64, I32]),
            Helper::MulHi => (vec![I64, I64], vec![I64]),
            Helper::MulShift => (vec![I64, I64, I64, I32], vec![I64]),
            Helper::Pow5Ge => (vec![I64, I32], vec![I32]),
            Helper::DecLen => (vec![I64], vec![I32]),
            Helper::WriteDigits => (vec![runtime::string_ref(), I32, I64], vec![]),
            Helper::Tables => (vec![], vec![tables_ref()]),
        }
    }

    pub fn body(self, cx: &mut dyn Cx) -> Function {
        match self {
            Helper::NumberToString => number_to_string(cx),
            Helper::D2d => d2d(cx),
            Helper::MulHi => mul_hi(),
            Helper::MulShift => {
                let hi = cx.helper(runtime::Helper::Num(Helper::MulHi));
                mul_shift(hi)
            }
            Helper::Pow5Ge => pow5_ge(),
            Helper::DecLen => dec_len(),
            Helper::WriteDigits => write_digits(),
            Helper::Tables => {
                let seg = cx.tables_data();
                tables(seg)
            }
        }
    }
}

fn tables_ref() -> ValType {
    ValType::Ref(RefType {
        nullable: false,
        heap_type: HeapType::Concrete(T_I64S),
    })
}

fn num(cx: &mut dyn Cx, h: Helper) -> u32 {
    cx.helper(runtime::Helper::Num(h))
}

// ---- tables (computed at compile time) -------------------------------------------------

/// Little-endian bytes of the i64 table: `INV_ENTRIES` (lo, hi) pairs of `floor(2^j / 5^q) + 1`,
/// then `POW_ENTRIES` pairs of the top 125 bits of `5^i`.
pub fn table_bytes() -> &'static [u8] {
    static BYTES: OnceLock<Vec<u8>> = OnceLock::new();
    BYTES.get_or_init(|| {
        let mut out = Vec::with_capacity(16 * (INV_ENTRIES + POW_ENTRIES));
        let mut push = |v: u128| {
            out.extend_from_slice(&(v as u64).to_le_bytes());
            out.extend_from_slice(&((v >> 64) as u64).to_le_bytes());
        };
        let mut p = Big::one();
        for _ in 0..INV_ENTRIES {
            let j = p.bit_len() - 1 + POW5_INV_BITCOUNT as usize;
            push(Big::pow2_div(j, &p) + 1);
            p.mul_small(5);
        }
        let mut p = Big::one();
        for _ in 0..POW_ENTRIES {
            let len = p.bit_len() as i64;
            let j = len - POW5_BITCOUNT as i64;
            push(if j >= 0 {
                p.bits(j as usize, POW5_BITCOUNT as usize)
            } else {
                p.bits(0, len as usize) << (-j)
            });
            p.mul_small(5);
        }
        out
    })
}

/// Just enough unsigned bignum for the tables (little-endian u32 limbs).
#[derive(Clone)]
struct Big(Vec<u32>);

impl Big {
    fn one() -> Big {
        Big(vec![1])
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for l in &mut self.0 {
            let v = *l as u64 * m as u64 + carry;
            *l = v as u32;
            carry = v >> 32;
        }
        if carry > 0 {
            self.0.push(carry as u32);
        }
    }

    fn bit_len(&self) -> usize {
        let top = *self.0.last().unwrap();
        32 * (self.0.len() - 1) + (32 - top.leading_zeros() as usize)
    }

    fn bit(&self, i: usize) -> bool {
        self.0.get(i / 32).is_some_and(|l| l >> (i % 32) & 1 == 1)
    }

    /// Bits `[from, from + n)` as a number (`n <= 128`).
    fn bits(&self, from: usize, n: usize) -> u128 {
        (0..n)
            .rev()
            .fold(0u128, |acc, i| acc << 1 | self.bit(from + i) as u128)
    }

    fn ge(&self, o: &Big) -> bool {
        let n = self.0.len().max(o.0.len());
        for i in (0..n).rev() {
            let (a, b) = (
                self.0.get(i).copied().unwrap_or(0),
                o.0.get(i).copied().unwrap_or(0),
            );
            if a != b {
                return a > b;
            }
        }
        true
    }

    fn sub(&mut self, o: &Big) {
        let mut borrow = 0i64;
        for i in 0..self.0.len() {
            let v = self.0[i] as i64 - o.0.get(i).copied().unwrap_or(0) as i64 - borrow;
            borrow = (v < 0) as i64;
            self.0[i] = v.rem_euclid(1 << 32) as u32;
        }
    }

    fn shl1_or(&mut self, bit: bool) {
        let mut carry = bit as u32;
        for l in &mut self.0 {
            let next = *l >> 31;
            *l = *l << 1 | carry;
            carry = next;
        }
        if carry > 0 {
            self.0.push(carry);
        }
    }

    /// `floor(2^j / d)`, which must fit in 128 bits.
    fn pow2_div(j: usize, d: &Big) -> u128 {
        let mut r = Big(vec![0]);
        let mut q = 0u128;
        for i in (0..=j).rev() {
            r.shl1_or(i == j);
            q <<= 1;
            if r.ge(d) {
                r.sub(d);
                q |= 1;
            }
        }
        q
    }
}

/// `$ryu_tables() -> (ref $i64s)`: the tables, loaded from the data segment on first use.
fn tables(seg: u32) -> Function {
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.block(BlockType::Result(tables_ref()));
    s.global_get(G_TABLES).br_on_non_null(0);
    s.i32_const(0)
        .i32_const(((INV_ENTRIES + POW_ENTRIES) * 2) as i32)
        .array_new_data(T_I64S, seg);
    s.global_set(G_TABLES);
    s.global_get(G_TABLES).ref_as_non_null();
    s.end();
    s.end();
    f
}

// ---- Ryu --------------------------------------------------------------------------------

/// `$umul128_hi(a, b) -> i64`: the high 64 bits of the 128-bit product.
fn mul_hi() -> Function {
    const A: u32 = 0;
    const B: u32 = 1;
    const B00: u32 = 2;
    const MID1: u32 = 3;
    const MID2: u32 = 4;
    let mut f = Function::new([(3, ValType::I64)]);
    let mut s = f.instructions();
    let lo = |s: &mut InstructionSink, x: u32| {
        s.local_get(x).i64_const(0xffff_ffff).i64_and();
    };
    let hi = |s: &mut InstructionSink, x: u32| {
        s.local_get(x).i64_const(32).i64_shr_u();
    };
    // b00 = aLo*bLo; mid1 = aHi*bLo + (b00 >> 32); mid2 = aLo*bHi + (mid1 & M32)
    lo(&mut s, A);
    lo(&mut s, B);
    s.i64_mul().local_set(B00);
    hi(&mut s, A);
    lo(&mut s, B);
    s.i64_mul();
    s.local_get(B00)
        .i64_const(32)
        .i64_shr_u()
        .i64_add()
        .local_set(MID1);
    lo(&mut s, A);
    hi(&mut s, B);
    s.i64_mul();
    s.local_get(MID1)
        .i64_const(0xffff_ffff)
        .i64_and()
        .i64_add()
        .local_set(MID2);
    // hi = aHi*bHi + (mid1 >> 32) + (mid2 >> 32)
    hi(&mut s, A);
    hi(&mut s, B);
    s.i64_mul();
    s.local_get(MID1).i64_const(32).i64_shr_u().i64_add();
    s.local_get(MID2).i64_const(32).i64_shr_u().i64_add();
    s.end();
    f
}

/// `$ryu_mul_shift(m, lo, hi, j) -> i64`: `(m * (hi·2^64 + lo)) >> j` for `64 < j < 128`.
fn mul_shift(mul_hi: u32) -> Function {
    const M: u32 = 0;
    const LO: u32 = 1;
    const HI: u32 = 2;
    const J: u32 = 3;
    const HIGH1: u32 = 4;
    const SUM: u32 = 5;
    const HIGH0: u32 = 6;
    let mut f = Function::new([(3, ValType::I64)]);
    let mut s = f.instructions();
    s.local_get(M).local_get(HI).call(mul_hi).local_set(HIGH1);
    s.local_get(M).local_get(LO).call(mul_hi).local_set(HIGH0);
    // sum = high0 + lo64(m * hi), carrying into high1.
    s.local_get(HIGH0)
        .local_get(M)
        .local_get(HI)
        .i64_mul()
        .i64_add()
        .local_tee(SUM);
    s.local_get(HIGH0).i64_lt_u();
    s.if_(BlockType::Empty);
    s.local_get(HIGH1).i64_const(1).i64_add().local_set(HIGH1);
    s.end();
    // (high1 << (64 - dist)) | (sum >> dist), dist = j - 64
    s.local_get(HIGH1);
    s.i64_const(128).local_get(J).i64_extend_i32_u().i64_sub();
    s.i64_shl();
    s.local_get(SUM);
    s.local_get(J).i32_const(64).i32_sub().i64_extend_i32_u();
    s.i64_shr_u();
    s.i64_or();
    s.end();
    f
}

/// `$ryu_pow5_ge(v, q) -> i32`: is `v` divisible by `5^q`?
fn pow5_ge() -> Function {
    const V: u32 = 0;
    const Q: u32 = 1;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.loop_(BlockType::Empty);
    s.local_get(Q).i32_const(0).i32_le_s();
    s.if_(BlockType::Empty);
    s.i32_const(1).return_();
    s.end();
    s.local_get(V).i64_const(5).i64_rem_u().i64_eqz().i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(0).return_();
    s.end();
    s.local_get(V).i64_const(5).i64_div_u().local_set(V);
    s.local_get(Q).i32_const(1).i32_sub().local_set(Q);
    s.br(0);
    s.end();
    s.unreachable();
    s.end();
    f
}

/// `$ryu_d2d(bits) -> (digits, exponent)`: the shortest `digits · 10^exponent` that rounds to the
/// positive finite nonzero double `bits`. A port of Ryu's `d2d` (general path).
fn d2d(cx: &mut dyn Cx) -> Function {
    let tables = num(cx, Helper::Tables);
    let mul_shift = num(cx, Helper::MulShift);
    let pow5_ge = num(cx, Helper::Pow5Ge);
    // params
    const BITS: u32 = 0;
    // i64 locals
    const MANT: u32 = 1;
    const M2: u32 = 2;
    const MV: u32 = 3;
    const VR: u32 = 4;
    const VP: u32 = 5;
    const VM: u32 = 6;
    const LO: u32 = 7;
    const HI: u32 = 8;
    // i32 locals
    const EX: u32 = 9;
    const E2: u32 = 10;
    const ACCEPT: u32 = 11;
    const MMSHIFT: u32 = 12;
    const E10: u32 = 13;
    const VM_TZ: u32 = 14;
    const VR_TZ: u32 = 15;
    const Q: u32 = 16;
    const SHIFT: u32 = 17;
    const IDX: u32 = 18;
    const REMOVED: u32 = 19;
    const LAST: u32 = 20;
    // (ref $i64s)
    const TBL: u32 = 21;
    let mut f = Function::new([
        (8, ValType::I64),
        (12, ValType::I32),
        (
            1,
            ValType::Ref(RefType {
                nullable: true,
                heap_type: HeapType::Concrete(T_I64S),
            }),
        ),
    ]);
    let mut s = f.instructions();

    // mant = bits & (2^52 - 1); ex = (bits >> 52) & 0x7ff
    s.local_get(BITS)
        .i64_const((1 << 52) - 1)
        .i64_and()
        .local_set(MANT);
    s.local_get(BITS)
        .i64_const(52)
        .i64_shr_u()
        .i32_wrap_i64()
        .i32_const(0x7ff)
        .i32_and()
        .local_set(EX);
    // Subnormal: e2 = 1 - 1023 - 52 - 2, m2 = mant; else e2 = ex - 1077, m2 = mant | 2^52
    s.local_get(EX).i32_eqz();
    s.if_(BlockType::Empty);
    s.i32_const(-1076).local_set(E2);
    s.local_get(MANT).local_set(M2);
    s.else_();
    s.local_get(EX).i32_const(1077).i32_sub().local_set(E2);
    s.local_get(MANT).i64_const(1 << 52).i64_or().local_set(M2);
    s.end();
    s.local_get(M2)
        .i64_const(1)
        .i64_and()
        .i64_eqz()
        .local_set(ACCEPT);
    s.local_get(M2).i64_const(2).i64_shl().local_set(MV);
    s.local_get(MANT).i64_eqz().i32_eqz();
    s.local_get(EX).i32_const(1).i32_le_u();
    s.i32_or().local_set(MMSHIFT);
    s.call(tables).local_set(TBL);

    // Push mv - 1 - mmShift.
    let mm = |s: &mut InstructionSink| {
        s.local_get(MV)
            .i64_const(1)
            .i64_sub()
            .local_get(MMSHIFT)
            .i64_extend_i32_u()
            .i64_sub();
    };
    // Load table entry (lo, hi) at i64 index `IDX`.
    let load = |s: &mut InstructionSink| {
        s.local_get(TBL)
            .local_get(IDX)
            .array_get(T_I64S)
            .local_set(LO);
        s.local_get(TBL)
            .local_get(IDX)
            .i32_const(1)
            .i32_add()
            .array_get(T_I64S)
            .local_set(HI);
    };
    // vr, vp, vm = mulShift(mv | mv+2 | mm, entry, SHIFT)
    let mul_all = |s: &mut InstructionSink| {
        s.local_get(MV)
            .local_get(LO)
            .local_get(HI)
            .local_get(SHIFT)
            .call(mul_shift)
            .local_set(VR);
        s.local_get(MV)
            .i64_const(2)
            .i64_add()
            .local_get(LO)
            .local_get(HI)
            .local_get(SHIFT)
            .call(mul_shift)
            .local_set(VP);
        mm(s);
        s.local_get(LO)
            .local_get(HI)
            .local_get(SHIFT)
            .call(mul_shift)
            .local_set(VM);
    };

    s.local_get(E2).i32_const(0).i32_ge_s();
    s.if_(BlockType::Empty);
    {
        // q = log10Pow2(e2) - (e2 > 3)
        s.local_get(E2)
            .i32_const(78913)
            .i32_mul()
            .i32_const(18)
            .i32_shr_u();
        s.local_get(E2).i32_const(3).i32_gt_s();
        s.i32_sub().local_tee(Q).local_set(E10);
        // k = 125 + pow5bits(q) - 1; shift = -e2 + q + k; pow5bits(q) = ((q * 1217359) >> 19) + 1
        s.local_get(Q)
            .i32_const(1217359)
            .i32_mul()
            .i32_const(19)
            .i32_shr_u()
            .i32_const(POW5_INV_BITCOUNT)
            .i32_add();
        s.local_get(Q)
            .i32_add()
            .local_get(E2)
            .i32_sub()
            .local_set(SHIFT);
        s.local_get(Q).i32_const(2).i32_mul().local_set(IDX);
        load(&mut s);
        mul_all(&mut s);
        s.local_get(Q).i32_const(21).i32_le_u();
        s.if_(BlockType::Empty);
        {
            s.local_get(MV).i64_const(5).i64_rem_u().i64_eqz();
            s.if_(BlockType::Empty);
            s.local_get(MV).local_get(Q).call(pow5_ge).local_set(VR_TZ);
            s.else_();
            s.local_get(ACCEPT);
            s.if_(BlockType::Empty);
            mm(&mut s);
            s.local_get(Q).call(pow5_ge).local_set(VM_TZ);
            s.else_();
            s.local_get(VP);
            s.local_get(MV)
                .i64_const(2)
                .i64_add()
                .local_get(Q)
                .call(pow5_ge);
            s.i64_extend_i32_u().i64_sub().local_set(VP);
            s.end();
            s.end();
        }
        s.end();
    }
    s.else_();
    {
        // q = log10Pow5(-e2) - (-e2 > 1); e10 = q + e2
        s.i32_const(0)
            .local_get(E2)
            .i32_sub()
            .i32_const(732923)
            .i32_mul()
            .i32_const(20)
            .i32_shr_u();
        s.i32_const(0)
            .local_get(E2)
            .i32_sub()
            .i32_const(1)
            .i32_gt_s();
        s.i32_sub()
            .local_tee(Q)
            .local_get(E2)
            .i32_add()
            .local_set(E10);
        // i = -e2 - q; k = pow5bits(i) - 125; shift = q - k
        s.i32_const(0)
            .local_get(E2)
            .i32_sub()
            .local_get(Q)
            .i32_sub()
            .local_tee(IDX);
        s.i32_const(1217359)
            .i32_mul()
            .i32_const(19)
            .i32_shr_u()
            .i32_const(1)
            .i32_add()
            .i32_const(POW5_BITCOUNT)
            .i32_sub();
        s.local_set(SHIFT);
        s.local_get(Q).local_get(SHIFT).i32_sub().local_set(SHIFT);
        s.local_get(IDX)
            .i32_const(2)
            .i32_mul()
            .i32_const(POW_OFFSET)
            .i32_add()
            .local_set(IDX);
        load(&mut s);
        mul_all(&mut s);
        s.local_get(Q).i32_const(1).i32_le_u();
        s.if_(BlockType::Empty);
        {
            s.i32_const(1).local_set(VR_TZ);
            s.local_get(ACCEPT);
            s.if_(BlockType::Empty);
            s.local_get(MMSHIFT).local_set(VM_TZ);
            s.else_();
            s.local_get(VP).i64_const(1).i64_sub().local_set(VP);
            s.end();
        }
        s.else_();
        {
            s.local_get(Q).i32_const(63).i32_lt_u();
            s.if_(BlockType::Empty);
            // vrTZ = (mv & ((1 << q) - 1)) == 0
            s.local_get(MV);
            s.i64_const(1)
                .local_get(Q)
                .i64_extend_i32_u()
                .i64_shl()
                .i64_const(1)
                .i64_sub();
            s.i64_and().i64_eqz().local_set(VR_TZ);
            s.end();
        }
        s.end();
    }
    s.end();

    // Remove one digit from vr/vp/vm, tracking the last removed digit of vr.
    let remove_digit = |s: &mut InstructionSink| {
        // vrTZ &= last == 0; last = vr % 10
        s.local_get(VR_TZ)
            .local_get(LAST)
            .i32_eqz()
            .i32_and()
            .local_set(VR_TZ);
        s.local_get(VR)
            .i64_const(10)
            .i64_rem_u()
            .i32_wrap_i64()
            .local_set(LAST);
        for v in [VR, VP, VM] {
            s.local_get(v).i64_const(10).i64_div_u().local_set(v);
        }
        s.local_get(REMOVED)
            .i32_const(1)
            .i32_add()
            .local_set(REMOVED);
    };
    // while (vp / 10 > vm / 10) { vmTZ &= vm % 10 == 0; remove }
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(VP).i64_const(10).i64_div_u();
    s.local_get(VM).i64_const(10).i64_div_u();
    s.i64_le_u().br_if(1);
    s.local_get(VM_TZ)
        .local_get(VM)
        .i64_const(10)
        .i64_rem_u()
        .i64_eqz()
        .i32_and()
        .local_set(VM_TZ);
    remove_digit(&mut s);
    s.br(0).end().end();
    // if vmTZ: while (vm % 10 == 0) remove
    s.local_get(VM_TZ);
    s.if_(BlockType::Empty);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(VM)
        .i64_const(10)
        .i64_rem_u()
        .i64_eqz()
        .i32_eqz()
        .br_if(1);
    remove_digit(&mut s);
    s.br(0).end().end();
    s.end();
    // Round half to even when the exact value ends in 5000…: last = 4
    s.local_get(VR_TZ);
    s.local_get(LAST).i32_const(5).i32_eq().i32_and();
    s.local_get(VR).i64_const(1).i64_and().i64_eqz().i32_and();
    s.if_(BlockType::Empty);
    s.i32_const(4).local_set(LAST);
    s.end();
    // out = vr + ((vr == vm && (!accept || !vmTZ)) || last >= 5)
    s.local_get(VR);
    s.local_get(VR).local_get(VM).i64_eq();
    s.local_get(ACCEPT)
        .i32_eqz()
        .local_get(VM_TZ)
        .i32_eqz()
        .i32_or()
        .i32_and();
    s.local_get(LAST).i32_const(5).i32_ge_u().i32_or();
    s.i64_extend_i32_u().i64_add();
    s.local_get(E10).local_get(REMOVED).i32_add();
    s.end();
    f
}

/// `$decimal_length(v) -> i32`: number of decimal digits (at least 1).
fn dec_len() -> Function {
    const V: u32 = 0;
    const N: u32 = 1;
    let mut f = Function::new([(1, ValType::I32)]);
    let mut s = f.instructions();
    s.i32_const(1).local_set(N);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(V).i64_const(10).i64_lt_u().br_if(1);
    s.local_get(V).i64_const(10).i64_div_u().local_set(V);
    s.local_get(N).i32_const(1).i32_add().local_set(N);
    s.br(0).end().end();
    s.local_get(N);
    s.end();
    f
}

/// `$write_digits(s, end, v)`: write the decimal digits of `v` into `s`, ending just before
/// index `end` (at least one digit, so 0 writes "0").
fn write_digits() -> Function {
    const S: u32 = 0;
    const END: u32 = 1;
    const V: u32 = 2;
    let mut f = Function::new([]);
    let mut s = f.instructions();
    s.loop_(BlockType::Empty);
    s.local_get(END).i32_const(1).i32_sub().local_set(END);
    s.local_get(S).local_get(END);
    s.local_get(V)
        .i64_const(10)
        .i64_rem_u()
        .i32_wrap_i64()
        .i32_const('0' as i32)
        .i32_add();
    s.array_set(T_STRING);
    s.local_get(V).i64_const(10).i64_div_u().local_tee(V);
    s.i64_eqz().i32_eqz().br_if(0);
    s.end();
    s.end();
    f
}

/// `$number_to_string(x: f64) -> (ref $string)`: ECMA-262 Number::toString(x) in base 10.
fn number_to_string(cx: &mut dyn Cx) -> Function {
    let d2d = num(cx, Helper::D2d);
    let dec_len = num(cx, Helper::DecLen);
    let write = num(cx, Helper::WriteDigits);
    let nan = cx.str("NaN");
    let zero = cx.str("0");
    let inf = cx.str("Infinity");
    let neg_inf = cx.str("-Infinity");
    const X: u32 = 0;
    // i64
    const OUT: u32 = 1;
    // i32
    const E: u32 = 2;
    const K: u32 = 3;
    const N: u32 = 4;
    const SIGN: u32 = 5;
    const LEN: u32 = 6;
    const P: u32 = 7;
    const AE: u32 = 8;
    // (ref null $string)
    const D: u32 = 9;
    const R: u32 = 10;
    let sref = ValType::Ref(RefType {
        nullable: true,
        heap_type: runtime::string(),
    });
    let mut f = Function::new([(1, ValType::I64), (7, ValType::I32), (2, sref)]);
    let mut s = f.instructions();
    let ret_global = |s: &mut InstructionSink, g: u32| {
        s.global_get(g).return_();
    };
    // NaN, ±0, ±∞
    s.local_get(X).local_get(X).f64_ne();
    s.if_(BlockType::Empty);
    ret_global(&mut s, nan);
    s.end();
    s.local_get(X).f64_const(0.0.into()).f64_eq();
    s.if_(BlockType::Empty);
    ret_global(&mut s, zero);
    s.end();
    s.local_get(X).f64_const(f64::INFINITY.into()).f64_eq();
    s.if_(BlockType::Empty);
    ret_global(&mut s, inf);
    s.end();
    s.local_get(X).f64_const(f64::NEG_INFINITY.into()).f64_eq();
    s.if_(BlockType::Empty);
    ret_global(&mut s, neg_inf);
    s.end();

    s.local_get(X)
        .f64_const(0.0.into())
        .f64_lt()
        .local_set(SIGN);
    s.local_get(X).f64_abs().i64_reinterpret_f64().call(d2d);
    s.local_set(E).local_set(OUT);
    s.local_get(OUT).call(dec_len).local_set(K);
    s.local_get(E).local_get(K).i32_add().local_set(N);
    // d = the k digits
    s.local_get(K).array_new_default(T_STRING).local_set(D);
    s.local_get(D)
        .ref_as_non_null()
        .local_get(K)
        .local_get(OUT)
        .call(write);

    let new_result = |s: &mut InstructionSink| {
        s.local_get(LEN).array_new_default(T_STRING).local_set(R);
        s.local_get(SIGN);
        s.if_(BlockType::Empty);
        s.local_get(R)
            .i32_const(0)
            .i32_const('-' as i32)
            .array_set(T_STRING);
        s.end();
    };
    // r[at] = c
    let put = |s: &mut InstructionSink, at: &dyn Fn(&mut InstructionSink), c: char| {
        s.local_get(R);
        at(s);
        s.i32_const(c as i32).array_set(T_STRING);
    };
    // array.copy r[dst..] = d[src..src+len]
    let copy = |s: &mut InstructionSink,
                dst: &dyn Fn(&mut InstructionSink),
                src: &dyn Fn(&mut InstructionSink),
                len: &dyn Fn(&mut InstructionSink)| {
        s.local_get(R);
        dst(s);
        s.local_get(D);
        src(s);
        len(s);
        s.array_copy(T_STRING, T_STRING);
    };
    let sign = |s: &mut InstructionSink| {
        s.local_get(SIGN);
    };
    let zero_i = |s: &mut InstructionSink| {
        s.i32_const(0);
    };

    // Case 1: k <= n <= 21 → digits then n-k zeros.
    s.local_get(K).local_get(N).i32_le_s();
    s.local_get(N).i32_const(21).i32_le_s();
    s.i32_and();
    s.if_(BlockType::Empty);
    {
        s.local_get(SIGN).local_get(N).i32_add().local_set(LEN);
        new_result(&mut s);
        copy(&mut s, &sign, &zero_i, &|s| {
            s.local_get(K);
        });
        // array.fill r (sign+k) '0' (n-k)
        s.local_get(R);
        s.local_get(SIGN).local_get(K).i32_add();
        s.i32_const('0' as i32);
        s.local_get(N).local_get(K).i32_sub();
        s.array_fill(T_STRING);
        s.local_get(R).ref_as_non_null().return_();
    }
    s.end();
    // Case 2: 0 < n <= 21 → d[0..n] "." d[n..k]
    s.local_get(N).i32_const(0).i32_gt_s();
    s.local_get(N).i32_const(21).i32_le_s();
    s.i32_and();
    s.if_(BlockType::Empty);
    {
        s.local_get(SIGN)
            .local_get(K)
            .i32_add()
            .i32_const(1)
            .i32_add()
            .local_set(LEN);
        new_result(&mut s);
        copy(&mut s, &sign, &zero_i, &|s| {
            s.local_get(N);
        });
        put(
            &mut s,
            &|s| {
                s.local_get(SIGN).local_get(N).i32_add();
            },
            '.',
        );
        copy(
            &mut s,
            &|s| {
                s.local_get(SIGN)
                    .local_get(N)
                    .i32_add()
                    .i32_const(1)
                    .i32_add();
            },
            &|s| {
                s.local_get(N);
            },
            &|s| {
                s.local_get(K).local_get(N).i32_sub();
            },
        );
        s.local_get(R).ref_as_non_null().return_();
    }
    s.end();
    // Case 3: -6 < n <= 0 → "0." (-n zeros) digits
    s.local_get(N).i32_const(-6).i32_gt_s();
    s.local_get(N).i32_const(0).i32_le_s();
    s.i32_and();
    s.if_(BlockType::Empty);
    {
        s.local_get(SIGN)
            .i32_const(2)
            .i32_add()
            .local_get(N)
            .i32_sub()
            .local_get(K)
            .i32_add()
            .local_set(LEN);
        new_result(&mut s);
        put(&mut s, &sign, '0');
        put(
            &mut s,
            &|s| {
                s.local_get(SIGN).i32_const(1).i32_add();
            },
            '.',
        );
        s.local_get(R);
        s.local_get(SIGN).i32_const(2).i32_add();
        s.i32_const('0' as i32);
        s.i32_const(0).local_get(N).i32_sub();
        s.array_fill(T_STRING);
        copy(
            &mut s,
            &|s| {
                s.local_get(SIGN)
                    .i32_const(2)
                    .i32_add()
                    .local_get(N)
                    .i32_sub();
            },
            &zero_i,
            &|s| {
                s.local_get(K);
            },
        );
        s.local_get(R).ref_as_non_null().return_();
    }
    s.end();
    // Case 4: exponent form d[0] ("." d[1..k])? "e" ("+"|"-") |n-1|
    s.local_get(N).i32_const(1).i32_sub().local_tee(AE);
    s.i32_const(0).i32_lt_s();
    s.if_(BlockType::Empty);
    s.i32_const(0).local_get(AE).i32_sub().local_set(AE);
    s.end();
    // len = sign + 1 + (k > 1 ? k : 0) + 2 + digits(ae)
    s.local_get(SIGN).i32_const(3).i32_add();
    s.local_get(K)
        .i32_const(0)
        .local_get(K)
        .i32_const(1)
        .i32_gt_s()
        .select();
    s.i32_add();
    s.local_get(AE).i64_extend_i32_u().call(dec_len);
    s.i32_add().local_set(LEN);
    new_result(&mut s);
    copy(&mut s, &sign, &zero_i, &|s| {
        s.i32_const(1);
    });
    s.local_get(SIGN).i32_const(1).i32_add().local_set(P);
    s.local_get(K).i32_const(1).i32_gt_s();
    s.if_(BlockType::Empty);
    {
        put(
            &mut s,
            &|s| {
                s.local_get(P);
            },
            '.',
        );
        copy(
            &mut s,
            &|s| {
                s.local_get(P).i32_const(1).i32_add();
            },
            &|s| {
                s.i32_const(1);
            },
            &|s| {
                s.local_get(K).i32_const(1).i32_sub();
            },
        );
        s.local_get(P).local_get(K).i32_add().local_set(P);
    }
    s.end();
    put(
        &mut s,
        &|s| {
            s.local_get(P);
        },
        'e',
    );
    s.local_get(R);
    s.local_get(P).i32_const(1).i32_add();
    s.i32_const('-' as i32).i32_const('+' as i32);
    s.local_get(N).i32_const(1).i32_lt_s();
    s.select();
    s.array_set(T_STRING);
    s.local_get(R)
        .ref_as_non_null()
        .local_get(LEN)
        .local_get(AE)
        .i64_extend_i32_u()
        .call(write);
    s.local_get(R).ref_as_non_null();
    s.end();
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(i: usize) -> u128 {
        let b = table_bytes();
        let lo = u64::from_le_bytes(b[16 * i..16 * i + 8].try_into().unwrap());
        let hi = u64::from_le_bytes(b[16 * i + 8..16 * i + 16].try_into().unwrap());
        (hi as u128) << 64 | lo as u128
    }

    /// Spot checks against Ryu's published `d2s_full_table.h`.
    #[test]
    fn tables_match_ryu() {
        assert_eq!(entry(0), 1u128 << 125 | 1); // DOUBLE_POW5_INV_SPLIT[0]
        assert_eq!(
            entry(1),
            (1844674407370955161u128 << 64) | 11068046444225730970
        );
        assert_eq!(entry(INV_ENTRIES), 1u128 << 124); // DOUBLE_POW5_SPLIT[0]
        assert_eq!(entry(INV_ENTRIES + 1), 1441151880758558720u128 << 64);
        assert_eq!(entry(INV_ENTRIES + 2), 1801439850948198400u128 << 64);
        assert_eq!(table_bytes().len(), 16 * (INV_ENTRIES + POW_ENTRIES));
    }
}
