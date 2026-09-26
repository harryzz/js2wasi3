//! `$js_pow(x: f64, y: f64) -> f64` (T24): JS `x ** y` (Number::exponentiate).
//!
//! A line-by-line port of fdlibm's `__ieee754_pow` (e_pow.c, the algorithm V8's pow comes
//! from), with the ECMAScript difference that `(±1) ** ±Infinity` is NaN. Error < 1 ulp.
//! Node 24 no longer matches it: its pow is bit for bit glibc's (the ARM optimized-routines
//! algorithm, almost always correctly rounded); this one differs in the last bit on ~10% of
//! random pairs. Node 20 differed on ~3%. See F77 in `tasks/README.md`.

// The constants are copied digit for digit from e_pow.c.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use wasm_encoder::{BlockType, Function, InstructionSink, ValType};

// Params.
const X: u32 = 0;
const Y: u32 = 1;
// f64 locals.
const Z: u32 = 2;
const AX: u32 = 3;
const Z_H: u32 = 4;
const Z_L: u32 = 5;
const P_H: u32 = 6;
const P_L: u32 = 7;
const Y1: u32 = 8;
const T1: u32 = 9;
const T2: u32 = 10;
const R: u32 = 11;
const S: u32 = 12;
const T: u32 = 13;
const U: u32 = 14;
const V: u32 = 15;
const W: u32 = 16;
const SS: u32 = 17;
const S2: u32 = 18;
const S_H: u32 = 19;
const S_L: u32 = 20;
const T_H: u32 = 21;
const T_L: u32 = 22;
// i32 locals.
const I: u32 = 23;
const J: u32 = 24;
const K: u32 = 25;
const YISINT: u32 = 26;
const N: u32 = 27;
const HX: u32 = 28;
const HY: u32 = 29;
const IX: u32 = 30;
const IY: u32 = 31;
const LX: u32 = 32;
const LY: u32 = 33;

const DP_H1: f64 = 5.84962487220764160156e-01;
const DP_L1: f64 = 1.35003920212974897128e-08;
const TWO53: f64 = 9007199254740992.0;
const HUGE: f64 = 1.0e300;
const TINY: f64 = 1.0e-300;
const L1: f64 = 5.99999999999994648725e-01;
const L2: f64 = 4.28571428578550184252e-01;
const L3: f64 = 3.33333329818377432918e-01;
const L4: f64 = 2.72728123808534006489e-01;
const L5: f64 = 2.30660745775561754067e-01;
const L6: f64 = 2.06975017800338417784e-01;
const P1: f64 = 1.66666666666666019037e-01;
const P2: f64 = -2.77777777770155933842e-03;
const P3: f64 = 6.61375632143793436117e-05;
const P4: f64 = -1.65339022054652515390e-06;
const P5: f64 = 4.13813679705723846039e-08;
const LG2: f64 = 6.93147180559945286227e-01;
const LG2_H: f64 = 6.93147182464599609375e-01;
const LG2_L: f64 = -1.90465429995776804525e-09;
const OVT: f64 = 8.0085662595372944372e-17;
const CP: f64 = 9.61796693925975554329e-01;
const CP_H: f64 = 9.61796700954437255859e-01;
const CP_L: f64 = -7.02846165095275826516e-09;
const IVLN2: f64 = 1.44269504088896338700e+00;
const IVLN2_H: f64 = 1.44269502162933349609e+00;
const IVLN2_L: f64 = 1.92596299112661746887e-08;

fn f(s: &mut InstructionSink, v: f64) {
    s.f64_const(v.into());
}

/// Push the high word of f64 local `l` (an i32).
fn hi(s: &mut InstructionSink, l: u32) {
    s.local_get(l)
        .i64_reinterpret_f64()
        .i64_const(32)
        .i64_shr_u();
    s.i32_wrap_i64();
}

/// Push the low word of f64 local `l`.
fn lo(s: &mut InstructionSink, l: u32) {
    s.local_get(l).i64_reinterpret_f64().i32_wrap_i64();
}

/// Clear the low word of f64 local `l`.
fn clear_lo(s: &mut InstructionSink, l: u32) {
    s.local_get(l).i64_reinterpret_f64();
    s.i64_const(-0x1_0000_0000).i64_and().f64_reinterpret_i64();
    s.local_set(l);
}

/// Replace the high word of f64 local `l` with the i32 on the stack.
fn set_hi(s: &mut InstructionSink, l: u32) {
    s.i64_extend_i32_u().i64_const(32).i64_shl();
    s.local_get(l)
        .i64_reinterpret_f64()
        .i64_const(0xffff_ffff)
        .i64_and();
    s.i64_or().f64_reinterpret_i64().local_set(l);
}

/// The f64 whose high word is the i32 on the stack and whose low word is 0.
fn from_hi(s: &mut InstructionSink) {
    s.i64_extend_i32_u()
        .i64_const(32)
        .i64_shl()
        .f64_reinterpret_i64();
}

/// `return <f64 on the stack>`.
fn ret(s: &mut InstructionSink) {
    s.return_();
}

/// `s * huge * huge` (overflow) or `s * tiny * tiny` (underflow) with sign local `S` (or 1).
fn over_under(s: &mut InstructionSink, over: bool, signed: bool) {
    let c = if over { HUGE } else { TINY };
    if signed {
        s.local_get(S);
        f(s, c);
        s.f64_mul();
    } else {
        f(s, c);
    }
    f(s, c);
    s.f64_mul();
}

pub fn js_pow() -> Function {
    let mut func = Function::new([(21, ValType::F64), (11, ValType::I32)]);
    let s = &mut func.instructions();

    hi(s, X);
    s.local_set(HX);
    lo(s, X);
    s.local_set(LX);
    hi(s, Y);
    s.local_set(HY);
    lo(s, Y);
    s.local_set(LY);
    s.local_get(HX)
        .i32_const(0x7fffffff)
        .i32_and()
        .local_set(IX);
    s.local_get(HY)
        .i32_const(0x7fffffff)
        .i32_and()
        .local_set(IY);

    // y == ±0: 1.
    s.local_get(IY).local_get(LY).i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    f(s, 1.0);
    ret(s);
    s.end();

    // NaN operand: NaN (JS: also NaN ** 0 handled above).
    s.local_get(IX).i32_const(0x7ff00000).i32_gt_u();
    s.local_get(IX).i32_const(0x7ff00000).i32_eq();
    s.local_get(LX).i32_const(0).i32_ne().i32_and().i32_or();
    s.local_get(IY).i32_const(0x7ff00000).i32_gt_u().i32_or();
    s.local_get(IY).i32_const(0x7ff00000).i32_eq();
    s.local_get(LY).i32_const(0).i32_ne().i32_and().i32_or();
    s.if_(BlockType::Empty);
    s.local_get(X).local_get(Y).f64_add();
    ret(s);
    s.end();

    // yisint: 0 not an integer, 1 odd, 2 even (only when x < 0).
    s.local_get(HX).i32_const(0).i32_lt_s();
    s.if_(BlockType::Empty);
    {
        s.local_get(IY).i32_const(0x43400000).i32_ge_s();
        s.if_(BlockType::Empty);
        s.i32_const(2).local_set(YISINT);
        s.else_();
        s.local_get(IY).i32_const(0x3ff00000).i32_ge_s();
        s.if_(BlockType::Empty);
        {
            s.local_get(IY)
                .i32_const(20)
                .i32_shr_s()
                .i32_const(0x3ff)
                .i32_sub();
            s.local_set(K);
            s.local_get(K).i32_const(20).i32_gt_s();
            s.if_(BlockType::Empty);
            {
                s.local_get(LY)
                    .i32_const(52)
                    .local_get(K)
                    .i32_sub()
                    .i32_shr_u();
                s.local_set(J);
                s.local_get(J)
                    .i32_const(52)
                    .local_get(K)
                    .i32_sub()
                    .i32_shl();
                s.local_get(LY).i32_eq();
                s.if_(BlockType::Empty);
                s.i32_const(2).local_get(J).i32_const(1).i32_and().i32_sub();
                s.local_set(YISINT);
                s.end();
            }
            s.else_();
            {
                s.local_get(LY).i32_eqz();
                s.if_(BlockType::Empty);
                s.local_get(IY)
                    .i32_const(20)
                    .local_get(K)
                    .i32_sub()
                    .i32_shr_s();
                s.local_set(J);
                s.local_get(J)
                    .i32_const(20)
                    .local_get(K)
                    .i32_sub()
                    .i32_shl();
                s.local_get(IY).i32_eq();
                s.if_(BlockType::Empty);
                s.i32_const(2).local_get(J).i32_const(1).i32_and().i32_sub();
                s.local_set(YISINT);
                s.end();
                s.end();
            }
            s.end();
        }
        s.end();
        s.end();
    }
    s.end();

    // Special values of y.
    s.local_get(LY).i32_eqz();
    s.if_(BlockType::Empty);
    {
        s.local_get(IY).i32_const(0x7ff00000).i32_eq();
        s.if_(BlockType::Empty);
        {
            // y is ±Infinity. JS: (±1) ** ±Infinity is NaN.
            s.local_get(IX).i32_const(0x3ff00000).i32_sub();
            s.local_get(LX).i32_or().i32_eqz();
            s.if_(BlockType::Empty);
            f(s, f64::NAN);
            ret(s);
            s.end();
            s.local_get(IX).i32_const(0x3ff00000).i32_ge_s();
            s.if_(BlockType::Empty);
            // (|x| > 1) ** +Inf = Inf, ** -Inf = 0.
            s.local_get(Y);
            f(s, 0.0);
            s.local_get(HY).i32_const(0).i32_ge_s();
            s.select();
            ret(s);
            s.end();
            // (|x| < 1) ** -Inf = Inf, ** +Inf = 0.
            s.local_get(Y).f64_neg();
            f(s, 0.0);
            s.local_get(HY).i32_const(0).i32_lt_s();
            s.select();
            ret(s);
        }
        s.end();
        s.local_get(IY).i32_const(0x3ff00000).i32_eq();
        s.if_(BlockType::Empty);
        {
            // y is ±1.
            s.local_get(HY).i32_const(0).i32_lt_s();
            s.if_(BlockType::Empty);
            f(s, 1.0);
            s.local_get(X).f64_div();
            ret(s);
            s.end();
            s.local_get(X);
            ret(s);
        }
        s.end();
        s.local_get(HY).i32_const(0x40000000).i32_eq();
        s.if_(BlockType::Empty);
        s.local_get(X).local_get(X).f64_mul();
        ret(s);
        s.end();
        s.local_get(HY).i32_const(0x3fe00000).i32_eq();
        s.local_get(HX).i32_const(0).i32_ge_s().i32_and();
        s.if_(BlockType::Empty);
        s.local_get(X).f64_sqrt();
        ret(s);
        s.end();
    }
    s.end();

    s.local_get(X).f64_abs().local_set(AX);
    // Special values of x: ±0, ±Infinity, ±1.
    s.local_get(LX).i32_eqz();
    s.local_get(IX).i32_const(0x7ff00000).i32_eq();
    s.local_get(IX).i32_eqz().i32_or();
    s.local_get(IX).i32_const(0x3ff00000).i32_eq().i32_or();
    s.i32_and();
    s.if_(BlockType::Empty);
    {
        s.local_get(AX).local_set(Z);
        s.local_get(HY).i32_const(0).i32_lt_s();
        s.if_(BlockType::Empty);
        f(s, 1.0);
        s.local_get(Z).f64_div().local_set(Z);
        s.end();
        s.local_get(HX).i32_const(0).i32_lt_s();
        s.if_(BlockType::Empty);
        {
            s.local_get(IX).i32_const(0x3ff00000).i32_sub();
            s.local_get(YISINT).i32_or().i32_eqz();
            s.if_(BlockType::Empty);
            // (-1) ** non-integer: NaN.
            f(s, f64::NAN);
            s.local_set(Z);
            s.else_();
            s.local_get(YISINT).i32_const(1).i32_eq();
            s.if_(BlockType::Empty);
            s.local_get(Z).f64_neg().local_set(Z);
            s.end();
            s.end();
        }
        s.end();
        s.local_get(Z);
        ret(s);
    }
    s.end();

    // n = 0 for x < 0, 1 otherwise.
    s.local_get(HX)
        .i32_const(31)
        .i32_shr_u()
        .i32_const(1)
        .i32_xor();
    s.local_set(N);
    // (x < 0) ** non-integer: NaN.
    s.local_get(N).local_get(YISINT).i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    f(s, f64::NAN);
    ret(s);
    s.end();
    // s: the sign of the result.
    f(s, 1.0);
    s.local_set(S);
    s.local_get(N);
    s.local_get(YISINT).i32_const(1).i32_sub();
    s.i32_or().i32_eqz();
    s.if_(BlockType::Empty);
    f(s, -1.0);
    s.local_set(S);
    s.end();

    s.local_get(IY).i32_const(0x41e00000).i32_gt_s();
    s.if_(BlockType::Empty);
    {
        // |y| > 2^31.
        s.local_get(IY).i32_const(0x43f00000).i32_gt_s();
        s.if_(BlockType::Empty);
        {
            // |y| > 2^64: overflow or underflow.
            s.local_get(IX).i32_const(0x3fefffff).i32_le_s();
            s.if_(BlockType::Empty);
            s.local_get(HY).i32_const(0).i32_lt_s();
            s.if_(BlockType::Result(ValType::F64));
            over_under(s, true, false);
            s.else_();
            over_under(s, false, false);
            s.end();
            ret(s);
            s.end();
            s.local_get(IX).i32_const(0x3ff00000).i32_ge_s();
            s.if_(BlockType::Empty);
            s.local_get(HY).i32_const(0).i32_gt_s();
            s.if_(BlockType::Result(ValType::F64));
            over_under(s, true, false);
            s.else_();
            over_under(s, false, false);
            s.end();
            ret(s);
            s.end();
        }
        s.end();
        // Over/underflow if x isn't close to 1.
        s.local_get(IX).i32_const(0x3fefffff).i32_lt_s();
        s.if_(BlockType::Empty);
        s.local_get(HY).i32_const(0).i32_lt_s();
        s.if_(BlockType::Result(ValType::F64));
        over_under(s, true, true);
        s.else_();
        over_under(s, false, true);
        s.end();
        ret(s);
        s.end();
        s.local_get(IX).i32_const(0x3ff00000).i32_gt_s();
        s.if_(BlockType::Empty);
        s.local_get(HY).i32_const(0).i32_gt_s();
        s.if_(BlockType::Result(ValType::F64));
        over_under(s, true, true);
        s.else_();
        over_under(s, false, true);
        s.end();
        ret(s);
        s.end();
        // |1 - x| <= 2^-20: log(x) by x - x^2/2 + x^3/3 - x^4/4.
        s.local_get(AX);
        f(s, 1.0);
        s.f64_sub().local_set(T);
        s.local_get(T).local_get(T).f64_mul();
        f(s, 0.5);
        s.local_get(T);
        f(s, 0.3333333333333333333333);
        s.local_get(T);
        f(s, 0.25);
        s.f64_mul().f64_sub().f64_mul().f64_sub();
        s.f64_mul().local_set(W);
        f(s, IVLN2_H);
        s.local_get(T).f64_mul().local_set(U);
        s.local_get(T);
        f(s, IVLN2_L);
        s.f64_mul();
        s.local_get(W);
        f(s, IVLN2);
        s.f64_mul().f64_sub().local_set(V);
        s.local_get(U).local_get(V).f64_add().local_set(T1);
        clear_lo(s, T1);
        s.local_get(V);
        s.local_get(T1).local_get(U).f64_sub();
        s.f64_sub().local_set(T2);
    }
    s.else_();
    {
        s.i32_const(0).local_set(N);
        // Subnormal x.
        s.local_get(IX).i32_const(0x00100000).i32_lt_s();
        s.if_(BlockType::Empty);
        s.local_get(AX);
        f(s, TWO53);
        s.f64_mul().local_set(AX);
        s.local_get(N).i32_const(53).i32_sub().local_set(N);
        hi(s, AX);
        s.local_set(IX);
        s.end();
        s.local_get(N);
        s.local_get(IX)
            .i32_const(20)
            .i32_shr_s()
            .i32_const(0x3ff)
            .i32_sub();
        s.i32_add().local_set(N);
        s.local_get(IX).i32_const(0x000fffff).i32_and().local_set(J);
        s.local_get(J).i32_const(0x3ff00000).i32_or().local_set(IX);
        // The interval: k = 0 for |x| < sqrt(3/2), 1 for < sqrt(3), else halve.
        s.local_get(J).i32_const(0x3988E).i32_le_s();
        s.if_(BlockType::Empty);
        s.i32_const(0).local_set(K);
        s.else_();
        s.local_get(J).i32_const(0xBB67A).i32_lt_s();
        s.if_(BlockType::Empty);
        s.i32_const(1).local_set(K);
        s.else_();
        s.i32_const(0).local_set(K);
        s.local_get(N).i32_const(1).i32_add().local_set(N);
        s.local_get(IX)
            .i32_const(0x00100000)
            .i32_sub()
            .local_set(IX);
        s.end();
        s.end();
        s.local_get(IX);
        set_hi(s, AX);
        // bp[k]: 1.0 or 1.5.
        let bp = |s: &mut InstructionSink| {
            f(s, 1.5);
            f(s, 1.0);
            s.local_get(K);
            s.select();
        };
        // ss = s_h + s_l = (x - 1) / (x + 1) or (x - 1.5) / (x + 1.5).
        s.local_get(AX);
        bp(s);
        s.f64_sub().local_set(U);
        f(s, 1.0);
        s.local_get(AX);
        bp(s);
        s.f64_add().f64_div().local_set(V);
        s.local_get(U).local_get(V).f64_mul().local_set(SS);
        s.local_get(SS).local_set(S_H);
        clear_lo(s, S_H);
        // t_h = ax + bp[k], high part.
        s.local_get(IX)
            .i32_const(1)
            .i32_shr_s()
            .i32_const(0x20000000)
            .i32_or();
        s.i32_const(0x00080000).i32_add();
        s.local_get(K).i32_const(18).i32_shl().i32_add();
        from_hi(s);
        s.local_set(T_H);
        s.local_get(AX);
        s.local_get(T_H);
        bp(s);
        s.f64_sub().f64_sub().local_set(T_L);
        s.local_get(V);
        s.local_get(U);
        s.local_get(S_H).local_get(T_H).f64_mul().f64_sub();
        s.local_get(S_H).local_get(T_L).f64_mul().f64_sub();
        s.f64_mul().local_set(S_L);
        // log(ax).
        s.local_get(SS).local_get(SS).f64_mul().local_set(S2);
        s.local_get(S2).local_get(S2).f64_mul();
        // L1 + s2 * (L2 + s2 * (… L6))
        f(s, L1);
        s.local_get(S2);
        f(s, L2);
        s.local_get(S2);
        f(s, L3);
        s.local_get(S2);
        f(s, L4);
        s.local_get(S2);
        f(s, L5);
        s.local_get(S2);
        f(s, L6);
        for _ in 0..5 {
            s.f64_mul().f64_add();
        }
        s.f64_mul().local_set(R);
        s.local_get(R);
        s.local_get(S_L);
        s.local_get(S_H).local_get(SS).f64_add();
        s.f64_mul().f64_add().local_set(R);
        s.local_get(S_H).local_get(S_H).f64_mul().local_set(S2);
        f(s, 3.0);
        s.local_get(S2)
            .f64_add()
            .local_get(R)
            .f64_add()
            .local_set(T_H);
        clear_lo(s, T_H);
        s.local_get(R);
        s.local_get(T_H);
        f(s, 3.0);
        s.f64_sub().local_get(S2).f64_sub();
        s.f64_sub().local_set(T_L);
        // u + v = ss * (1 + …)
        s.local_get(S_H).local_get(T_H).f64_mul().local_set(U);
        s.local_get(S_L).local_get(T_H).f64_mul();
        s.local_get(T_L).local_get(SS).f64_mul();
        s.f64_add().local_set(V);
        // 2 / (3 log2) * (ss + …)
        s.local_get(U).local_get(V).f64_add().local_set(P_H);
        clear_lo(s, P_H);
        s.local_get(V);
        s.local_get(P_H).local_get(U).f64_sub();
        s.f64_sub().local_set(P_L);
        f(s, CP_H);
        s.local_get(P_H).f64_mul().local_set(Z_H);
        f(s, CP_L);
        s.local_get(P_H).f64_mul();
        s.local_get(P_L);
        f(s, CP);
        s.f64_mul().f64_add();
        f(s, DP_L1);
        f(s, 0.0);
        s.local_get(K);
        s.select();
        s.f64_add().local_set(Z_L);
        // log2(ax) = n + dp_h + z_h + z_l
        s.local_get(N).f64_convert_i32_s().local_set(T);
        let dp_h = |s: &mut InstructionSink| {
            f(s, DP_H1);
            f(s, 0.0);
            s.local_get(K);
            s.select();
        };
        s.local_get(Z_H).local_get(Z_L).f64_add();
        dp_h(s);
        s.f64_add().local_get(T).f64_add().local_set(T1);
        clear_lo(s, T1);
        s.local_get(Z_L);
        s.local_get(T1).local_get(T).f64_sub();
        dp_h(s);
        s.f64_sub();
        s.local_get(Z_H).f64_sub();
        s.f64_sub().local_set(T2);
    }
    s.end();

    // (y1 + y2) * (t1 + t2), y1 the high part of y.
    s.local_get(Y).local_set(Y1);
    clear_lo(s, Y1);
    s.local_get(Y)
        .local_get(Y1)
        .f64_sub()
        .local_get(T1)
        .f64_mul();
    s.local_get(Y).local_get(T2).f64_mul();
    s.f64_add().local_set(P_L);
    s.local_get(Y1).local_get(T1).f64_mul().local_set(P_H);
    s.local_get(P_L).local_get(P_H).f64_add().local_set(Z);
    hi(s, Z);
    s.local_set(J);
    lo(s, Z);
    s.local_set(I);
    s.local_get(J).i32_const(0x40900000).i32_ge_s();
    s.if_(BlockType::Empty);
    {
        // z >= 1024.
        s.local_get(J).i32_const(0x40900000).i32_sub();
        s.local_get(I).i32_or().i32_const(0).i32_ne();
        s.if_(BlockType::Empty);
        over_under(s, true, true);
        ret(s);
        s.end();
        s.local_get(P_L);
        f(s, OVT);
        s.f64_add();
        s.local_get(Z).local_get(P_H).f64_sub();
        s.f64_gt();
        s.if_(BlockType::Empty);
        over_under(s, true, true);
        ret(s);
        s.end();
    }
    s.else_();
    {
        s.local_get(J).i32_const(0x7fffffff).i32_and();
        s.i32_const(0x4090cc00).i32_ge_s();
        s.if_(BlockType::Empty);
        {
            // z <= -1075.
            s.local_get(J).i32_const(0xc090cc00_u32 as i32).i32_sub();
            s.local_get(I).i32_or().i32_const(0).i32_ne();
            s.if_(BlockType::Empty);
            over_under(s, false, true);
            ret(s);
            s.end();
            s.local_get(P_L);
            s.local_get(Z).local_get(P_H).f64_sub();
            s.f64_le();
            s.if_(BlockType::Empty);
            over_under(s, false, true);
            ret(s);
            s.end();
        }
        s.end();
    }
    s.end();

    // 2 ** (p_h + p_l).
    s.local_get(J).i32_const(0x7fffffff).i32_and().local_set(I);
    s.local_get(I)
        .i32_const(20)
        .i32_shr_s()
        .i32_const(0x3ff)
        .i32_sub()
        .local_set(K);
    s.i32_const(0).local_set(N);
    s.local_get(I).i32_const(0x3fe00000).i32_gt_s();
    s.if_(BlockType::Empty);
    {
        // |z| > 0.5: n = [z + 0.5].
        s.local_get(J);
        s.i32_const(0x00100000);
        s.local_get(K).i32_const(1).i32_add();
        s.i32_shr_s().i32_add().local_set(N);
        s.local_get(N).i32_const(0x7fffffff).i32_and();
        s.i32_const(20)
            .i32_shr_s()
            .i32_const(0x3ff)
            .i32_sub()
            .local_set(K);
        s.local_get(N);
        s.i32_const(0x000fffff).local_get(K).i32_shr_s();
        s.i32_const(-1).i32_xor().i32_and();
        from_hi(s);
        s.local_set(T);
        s.local_get(N)
            .i32_const(0x000fffff)
            .i32_and()
            .i32_const(0x00100000)
            .i32_or();
        s.i32_const(20)
            .local_get(K)
            .i32_sub()
            .i32_shr_s()
            .local_set(N);
        s.local_get(J).i32_const(0).i32_lt_s();
        s.if_(BlockType::Empty);
        s.i32_const(0).local_get(N).i32_sub().local_set(N);
        s.end();
        s.local_get(P_H).local_get(T).f64_sub().local_set(P_H);
    }
    s.end();
    s.local_get(P_L).local_get(P_H).f64_add().local_set(T);
    clear_lo(s, T);
    s.local_get(T);
    f(s, LG2_H);
    s.f64_mul().local_set(U);
    s.local_get(P_L);
    s.local_get(T).local_get(P_H).f64_sub();
    s.f64_sub();
    f(s, LG2);
    s.f64_mul();
    s.local_get(T);
    f(s, LG2_L);
    s.f64_mul().f64_add().local_set(V);
    s.local_get(U).local_get(V).f64_add().local_set(Z);
    s.local_get(V);
    s.local_get(Z).local_get(U).f64_sub();
    s.f64_sub().local_set(W);
    s.local_get(Z).local_get(Z).f64_mul().local_set(T);
    // t1 = z - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))))
    s.local_get(Z);
    s.local_get(T);
    f(s, P1);
    s.local_get(T);
    f(s, P2);
    s.local_get(T);
    f(s, P3);
    s.local_get(T);
    f(s, P4);
    s.local_get(T);
    f(s, P5);
    for _ in 0..4 {
        s.f64_mul().f64_add();
    }
    s.f64_mul().f64_sub().local_set(T1);
    // r = (z * t1) / (t1 - 2) - (w + z * w)
    s.local_get(Z).local_get(T1).f64_mul();
    s.local_get(T1);
    f(s, 2.0);
    s.f64_sub().f64_div();
    s.local_get(W);
    s.local_get(Z).local_get(W).f64_mul();
    s.f64_add().f64_sub().local_set(R);
    // z = 1 - (r - z)
    f(s, 1.0);
    s.local_get(R).local_get(Z).f64_sub();
    s.f64_sub().local_set(Z);
    hi(s, Z);
    s.local_get(N)
        .i32_const(20)
        .i32_shl()
        .i32_add()
        .local_set(J);
    s.local_get(J)
        .i32_const(20)
        .i32_shr_s()
        .i32_const(0)
        .i32_le_s();
    s.if_(BlockType::Empty);
    {
        // Subnormal result: z * 2^n in two steps (n < -1022 here): the exact one first, so
        // only the last multiplication rounds.
        s.local_get(Z);
        s.local_get(N).i32_const(1022 + 1023).i32_add();
        s.i64_extend_i32_u()
            .i64_const(52)
            .i64_shl()
            .f64_reinterpret_i64();
        s.f64_mul();
        f(s, f64::from_bits(1u64 << 52)); // 2^-1022
        s.f64_mul().local_set(Z);
    }
    s.else_();
    s.local_get(J);
    set_hi(s, Z);
    s.end();
    s.local_get(S).local_get(Z).f64_mul();
    s.end();
    func
}
