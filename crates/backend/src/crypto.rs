//! T152: `crypto`'s throughput primitives as Wasm over a typed array's bytes (`$typed` → `$buffer` → `$bytes`), as `typed.rs`'s
//! `Utf8Write`: no copies, linked when `crypto.js` reaches them. Padding, buffering, HMAC and node's errors stay in
//! JavaScript; these run the compression functions over whole blocks.
//!
//! The state of a hash is a `Uint8Array` in the digest's byte order (SHA-2: big-endian words), so the digest is read straight out of it.

use crate::runtime::{T_BYTES, T_I32S, rt};
use crate::typed::{F_BUF, F_DATA, F_OFFSET, typed};
use crate::runtime::{T_BUFFER, T_TYPED};
use wasm_encoder::{BlockType, Function, HeapType, InstructionSink, ValType};

/// `(jsval)` local `l` → its `$bytes` in local `to`.
fn bytes_of(s: &mut InstructionSink, l: u32, to: u32) {
    s.local_get(l).ref_cast_non_null(typed());
    s.struct_get(T_TYPED, F_BUF).struct_get(T_BUFFER, F_DATA).local_set(to);
}

/// `typed.byteOffset` of jsval local `l`.
fn offset_of(s: &mut InstructionSink, l: u32) {
    s.local_get(l).ref_cast_non_null(typed()).struct_get(T_TYPED, F_OFFSET);
}

/// The big-endian word at byte `base + k` of `$bytes` local `d` (`base` is a local).
fn be32(s: &mut InstructionSink, d: u32, base: u32, k: i32) {
    for j in 0..4 {
        s.local_get(d).local_get(base).i32_const(k + j).i32_add().array_get_u(T_BYTES);
        let shift = 24 - 8 * j;
        if shift > 0 {
            s.i32_const(shift).i32_shl();
        }
        if j > 0 {
            s.i32_or();
        }
    }
}

/// Store the i32 in local `v` big-endian at byte `base + k` of `$bytes` local `d`.
fn put_be32(s: &mut InstructionSink, d: u32, base: u32, k: i32, v: u32) {
    for j in 0..4 {
        s.local_get(d).local_get(base).i32_const(k + j).i32_add();
        s.local_get(v);
        let shift = 24 - 8 * j;
        if shift > 0 {
            s.i32_const(shift).i32_shr_u();
        }
        s.array_set(T_BYTES);
    }
}

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
    0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
    0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
    0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
    0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// `sha256_blocks(state, data, offset, blocks)`: SHA-256's compression over `blocks` 64-byte blocks of `data` from `offset`;
/// `state` is the 32 bytes of the eight words, big-endian, updated in place. One block per loop turn, its 64 rounds unrolled
/// with the message schedule in sixteen locals and the constants as immediates (a GC array access costs far more than a local).
pub fn sha256_blocks() -> Function {
    // Parameters.
    const STATE: u32 = 0;
    const DATA: u32 = 1;
    const OFF: u32 = 2;
    const BLOCKS: u32 = 3;
    // Locals.
    const ST: u32 = 4;
    const D: u32 = 5;
    const P: u32 = 6;
    const STB: u32 = 7;
    const T1: u32 = 8;
    const X: u32 = 9;
    const A: u32 = 10; // a..h = 10..17
    const W: u32 = 18; // w0..w15 = 18..33
    let bytes = rt(true, T_BYTES);
    let mut f = Function::new([(2, bytes), (28, ValType::I32)]);
    let mut s = f.instructions();
    bytes_of(&mut s, STATE, ST);
    bytes_of(&mut s, DATA, D);
    offset_of(&mut s, STATE);
    s.local_set(STB);
    offset_of(&mut s, DATA);
    s.local_get(OFF).i32_add().local_set(P);
    let rotr = |s: &mut InstructionSink, n: i32| {
        s.i32_const(n).i32_rotr();
    };

    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(BLOCKS).i32_eqz().br_if(1);
    for k in 0..8 {
        be32(&mut s, ST, STB, 4 * k);
        s.local_set(A + k as u32);
    }
    for i in 0..16 {
        be32(&mut s, D, P, 4 * i);
        s.local_set(W + i as u32);
    }
    for i in 0..64u32 {
        let w = |k: u32| W + (i + 16 - k) % 16;
        if i >= 16 {
            // w[i] = s1(w[i-2]) + w[i-7] + s0(w[i-15]) + w[i-16]
            s.local_get(w(2)).local_set(X);
            s.local_get(X);
            rotr(&mut s, 17);
            s.local_get(X);
            rotr(&mut s, 19);
            s.i32_xor().local_get(X).i32_const(10).i32_shr_u().i32_xor();
            s.local_get(w(7)).i32_add();
            s.local_get(w(15)).local_set(X);
            s.local_get(X);
            rotr(&mut s, 7);
            s.local_get(X);
            rotr(&mut s, 18);
            s.i32_xor().local_get(X).i32_const(3).i32_shr_u().i32_xor();
            s.i32_add();
            s.local_get(w(16)).i32_add().local_set(w(16));
        }
        // role k (a = 0 … h = 7) lives in local A + (k + 8 - i % 8) % 8
        let role = |k: u32| A + (k + 8 - i % 8) % 8;
        let (ra, rb, rc, rd, re, rf, rg, rh) = (role(0), role(1), role(2), role(3), role(4), role(5), role(6), role(7));
        // t1 = h + S1(e) + ch(e, f, g) + K[i] + w[i]
        s.local_get(rh);
        s.local_get(re);
        rotr(&mut s, 6);
        s.local_get(re);
        rotr(&mut s, 11);
        s.i32_xor().local_get(re);
        rotr(&mut s, 25);
        s.i32_xor().i32_add();
        // ch = (e & f) ^ (~e & g) = g ^ (e & (f ^ g))
        s.local_get(rg).local_get(rf).local_get(rg).i32_xor().local_get(re).i32_and().i32_xor().i32_add();
        s.i32_const(SHA256_K[i as usize] as i32).i32_add();
        s.local_get(w(16)).i32_add();
        s.local_set(T1);
        // d += t1
        s.local_get(rd).local_get(T1).i32_add().local_set(rd);
        // h = t1 + S0(a) + maj(a, b, c); maj = (a & b) | (c & (a | b))
        s.local_get(ra);
        rotr(&mut s, 2);
        s.local_get(ra);
        rotr(&mut s, 13);
        s.i32_xor().local_get(ra);
        rotr(&mut s, 22);
        s.i32_xor();
        s.local_get(ra).local_get(rb).i32_and();
        s.local_get(rc).local_get(ra).local_get(rb).i32_or().i32_and().i32_or();
        s.i32_add().local_get(T1).i32_add().local_set(rh);
    }
    // state += a..h
    for k in 0..8 {
        be32(&mut s, ST, STB, 4 * k);
        s.local_get(A + k as u32).i32_add().local_set(X);
        put_be32(&mut s, ST, STB, 4 * k, X);
    }
    s.local_get(P).i32_const(64).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    s.end();
    f
}

/// The little-endian word at byte `base + k` of `$bytes` local `d`.
fn le32(s: &mut InstructionSink, d: u32, base: u32, k: i32) {
    for j in 0..4 {
        s.local_get(d).local_get(base).i32_const(k + j).i32_add().array_get_u(T_BYTES);
        if j > 0 {
            s.i32_const(8 * j).i32_shl().i32_or();
        }
    }
}

/// Store the i32 in local `v` little-endian at byte `base + k`.
fn put_le32(s: &mut InstructionSink, d: u32, base: u32, k: i32, v: u32) {
    for j in 0..4 {
        s.local_get(d).local_get(base).i32_const(k + j).i32_add();
        s.local_get(v);
        if j > 0 {
            s.i32_const(8 * j).i32_shr_u();
        }
        s.array_set(T_BYTES);
    }
}

/// The 64-bit word at byte `base + k`, big- or little-endian.
fn load64(s: &mut InstructionSink, d: u32, base: u32, k: i32, big: bool) {
    for j in 0..8 {
        s.local_get(d).local_get(base).i32_const(k + j).i32_add().array_get_u(T_BYTES).i64_extend_i32_u();
        let shift = if big { 56 - 8 * j } else { 8 * j };
        if shift > 0 {
            s.i64_const(shift as i64).i64_shl();
        }
        if j > 0 {
            s.i64_or();
        }
    }
}

/// Store the i64 in local `v` at byte `base + k`, big- or little-endian.
fn store64(s: &mut InstructionSink, d: u32, base: u32, k: i32, v: u32, big: bool) {
    for j in 0..8 {
        s.local_get(d).local_get(base).i32_const(k + j).i32_add();
        s.local_get(v);
        let shift = if big { 56 - 8 * j } else { 8 * j };
        if shift > 0 {
            s.i64_const(shift as i64).i64_shr_u();
        }
        s.i32_wrap_i64().array_set(T_BYTES);
    }
}

/// The common start of the block functions: `$bytes` of the state and the data, the data position and the state offset.
fn prologue(s: &mut InstructionSink, st: u32, d: u32, p: u32, stb: u32) {
    // Parameters are always (state, data, offset, blocks, ...).
    bytes_of(s, 0, st);
    bytes_of(s, 1, d);
    offset_of(s, 0);
    s.local_set(stb);
    offset_of(s, 1);
    s.local_get(2).i32_add().local_set(p);
}

const SHA1_K: [u32; 4] = [0x5a827999, 0x6ed9eba1, 0x8f1bbcdc, 0xca62c1d6];

/// `sha1_blocks(state, data, offset, blocks)`: as [`sha256_blocks`] for SHA-1 (five big-endian words, 80 rounds).
pub fn sha1_blocks() -> Function {
    const BLOCKS: u32 = 3;
    const ST: u32 = 4;
    const D: u32 = 5;
    const P: u32 = 6;
    const STB: u32 = 7;
    const T: u32 = 8;
    const X: u32 = 9;
    const A: u32 = 10; // a..e = 10..14
    const W: u32 = 15; // w0..w15 = 15..30
    let mut f = Function::new([(2, rt(true, T_BYTES)), (27, ValType::I32)]);
    let mut s = f.instructions();
    prologue(&mut s, ST, D, P, STB);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(BLOCKS).i32_eqz().br_if(1);
    for k in 0..5 {
        be32(&mut s, ST, STB, 4 * k);
        s.local_set(A + k as u32);
    }
    for i in 0..16 {
        be32(&mut s, D, P, 4 * i);
        s.local_set(W + i as u32);
    }
    for i in 0..80u32 {
        let w = |k: u32| W + (i + 16 - k) % 16;
        if i >= 16 {
            s.local_get(w(3)).local_get(w(8)).i32_xor().local_get(w(14)).i32_xor().local_get(w(16)).i32_xor();
            s.i32_const(1).i32_rotl().local_set(w(16));
        }
        let role = |k: u32| A + (k + 5 - i % 5) % 5;
        let (ra, rb, rc, rd, re) = (role(0), role(1), role(2), role(3), role(4));
        // temp = rotl(a, 5) + f(b, c, d) + e + K + w[i]
        s.local_get(ra).i32_const(5).i32_rotl();
        match i / 20 {
            0 => {
                // d ^ (b & (c ^ d))
                s.local_get(rd).local_get(rc).local_get(rd).i32_xor().local_get(rb).i32_and().i32_xor();
            }
            2 => {
                // (b & c) | (d & (b | c))
                s.local_get(rb).local_get(rc).i32_and();
                s.local_get(rd).local_get(rb).local_get(rc).i32_or().i32_and().i32_or();
            }
            _ => {
                s.local_get(rb).local_get(rc).i32_xor().local_get(rd).i32_xor();
            }
        }
        s.i32_add().local_get(re).i32_add();
        s.i32_const(SHA1_K[(i / 20) as usize] as i32).i32_add();
        s.local_get(w(16)).i32_add().local_set(T);
        // c' = rotl(b, 30) in b's slot; the new a goes to e's slot
        s.local_get(rb).i32_const(30).i32_rotl().local_set(rb);
        s.local_get(T).local_set(re);
    }
    for k in 0..5 {
        be32(&mut s, ST, STB, 4 * k);
        s.local_get(A + k as u32).i32_add().local_set(X);
        put_be32(&mut s, ST, STB, 4 * k, X);
    }
    s.local_get(P).i32_const(64).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    s.end();
    f
}

const MD5_S: [i32; 16] = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];

/// `md5_blocks(state, data, offset, blocks)`: as [`sha256_blocks`] for MD5 (four little-endian words, 64 rounds).
pub fn md5_blocks() -> Function {
    const BLOCKS: u32 = 3;
    const ST: u32 = 4;
    const D: u32 = 5;
    const P: u32 = 6;
    const STB: u32 = 7;
    const X: u32 = 8;
    const A: u32 = 9; // a..d = 9..12
    const M: u32 = 13; // m0..m15 = 13..28
    let mut f = Function::new([(2, rt(true, T_BYTES)), (27, ValType::I32)]);
    let mut s = f.instructions();
    prologue(&mut s, ST, D, P, STB);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(BLOCKS).i32_eqz().br_if(1);
    for k in 0..4 {
        le32(&mut s, ST, STB, 4 * k);
        s.local_set(A + k as u32);
    }
    for i in 0..16 {
        le32(&mut s, D, P, 4 * i);
        s.local_set(M + i as u32);
    }
    for i in 0..64u32 {
        let role = |k: u32| A + (k + 4 - i % 4) % 4;
        let (ra, rb, rc, rd) = (role(0), role(1), role(2), role(3));
        let g = match i / 16 {
            0 => i,
            1 => (5 * i + 1) % 16,
            2 => (3 * i + 5) % 16,
            _ => (7 * i) % 16,
        };
        let k = ((((i + 1) as f64).sin().abs() * 4294967296.0).floor() as u64) as u32;
        // a = b + rotl(a + F(b, c, d) + K[i] + m[g], s)
        s.local_get(rb);
        s.local_get(ra);
        match i / 16 {
            0 => {
                s.local_get(rd).local_get(rc).local_get(rd).i32_xor().local_get(rb).i32_and().i32_xor();
            }
            1 => {
                s.local_get(rc).local_get(rb).local_get(rc).i32_xor().local_get(rd).i32_and().i32_xor();
            }
            2 => {
                s.local_get(rb).local_get(rc).i32_xor().local_get(rd).i32_xor();
            }
            _ => {
                // c ^ (b | ~d)
                s.local_get(rc).local_get(rd).i32_const(-1).i32_xor().local_get(rb).i32_or().i32_xor();
            }
        }
        s.i32_add().i32_const(k as i32).i32_add().local_get(M + g).i32_add();
        s.i32_const(MD5_S[((i / 16) * 4 + i % 4) as usize]).i32_rotl();
        s.i32_add().local_set(ra);
    }
    for k in 0..4 {
        le32(&mut s, ST, STB, 4 * k);
        s.local_get(A + k as u32).i32_add().local_set(X);
        put_le32(&mut s, ST, STB, 4 * k, X);
    }
    s.local_get(P).i32_const(64).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    s.end();
    f
}

const SHA512_K: [u64; 80] = [
    0x428a2f98d728ae22,
    0x7137449123ef65cd,
    0xb5c0fbcfec4d3b2f,
    0xe9b5dba58189dbbc,
    0x3956c25bf348b538,
    0x59f111f1b605d019,
    0x923f82a4af194f9b,
    0xab1c5ed5da6d8118,
    0xd807aa98a3030242,
    0x12835b0145706fbe,
    0x243185be4ee4b28c,
    0x550c7dc3d5ffb4e2,
    0x72be5d74f27b896f,
    0x80deb1fe3b1696b1,
    0x9bdc06a725c71235,
    0xc19bf174cf692694,
    0xe49b69c19ef14ad2,
    0xefbe4786384f25e3,
    0x0fc19dc68b8cd5b5,
    0x240ca1cc77ac9c65,
    0x2de92c6f592b0275,
    0x4a7484aa6ea6e483,
    0x5cb0a9dcbd41fbd4,
    0x76f988da831153b5,
    0x983e5152ee66dfab,
    0xa831c66d2db43210,
    0xb00327c898fb213f,
    0xbf597fc7beef0ee4,
    0xc6e00bf33da88fc2,
    0xd5a79147930aa725,
    0x06ca6351e003826f,
    0x142929670a0e6e70,
    0x27b70a8546d22ffc,
    0x2e1b21385c26c926,
    0x4d2c6dfc5ac42aed,
    0x53380d139d95b3df,
    0x650a73548baf63de,
    0x766a0abb3c77b2a8,
    0x81c2c92e47edaee6,
    0x92722c851482353b,
    0xa2bfe8a14cf10364,
    0xa81a664bbc423001,
    0xc24b8b70d0f89791,
    0xc76c51a30654be30,
    0xd192e819d6ef5218,
    0xd69906245565a910,
    0xf40e35855771202a,
    0x106aa07032bbd1b8,
    0x19a4c116b8d2d0c8,
    0x1e376c085141ab53,
    0x2748774cdf8eeb99,
    0x34b0bcb5e19b48a8,
    0x391c0cb3c5c95a63,
    0x4ed8aa4ae3418acb,
    0x5b9cca4f7763e373,
    0x682e6ff3d6b2b8a3,
    0x748f82ee5defb2fc,
    0x78a5636f43172f60,
    0x84c87814a1f0ab72,
    0x8cc702081a6439ec,
    0x90befffa23631e28,
    0xa4506cebde82bde9,
    0xbef9a3f7b2c67915,
    0xc67178f2e372532b,
    0xca273eceea26619c,
    0xd186b8c721c0c207,
    0xeada7dd6cde0eb1e,
    0xf57d4f7fee6ed178,
    0x06f067aa72176fba,
    0x0a637dc5a2c898a6,
    0x113f9804bef90dae,
    0x1b710b35131c471b,
    0x28db77f523047d84,
    0x32caab7b40c72493,
    0x3c9ebe0a15c9bebc,
    0x431d67c49c100d4c,
    0x4cc5d4becb3e42b6,
    0x597f299cfc657e2a,
    0x5fcb6fab3ad6faec,
    0x6c44198c4a475817,
];

/// `sha512_blocks(state, data, offset, blocks)`: as [`sha256_blocks`] for SHA-512 (eight big-endian 64-bit words, 128-byte
/// blocks, 80 rounds on `i64` locals); SHA-384 and SHA-512/t differ in the initial state and the digest cut only.
pub fn sha512_blocks() -> Function {
    const BLOCKS: u32 = 3;
    const ST: u32 = 4;
    const D: u32 = 5;
    const P: u32 = 6;
    const STB: u32 = 7;
    // i64 locals: t1, x, a..h, w0..w15 (after the four i32s above).
    const T1: u32 = 8;
    const X: u32 = 9;
    const A: u32 = 10; // a..h = 10..17
    const W: u32 = 18; // w0..w15 = 18..33
    let mut f = Function::new([(2, rt(true, T_BYTES)), (2, ValType::I32), (26, ValType::I64)]);
    let mut s = f.instructions();
    prologue(&mut s, ST, D, P, STB);
    let rotr = |s: &mut InstructionSink, n: i64| {
        s.i64_const(n).i64_rotr();
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(BLOCKS).i32_eqz().br_if(1);
    for k in 0..8 {
        load64(&mut s, ST, STB, 8 * k, true);
        s.local_set(A + k as u32);
    }
    for i in 0..16 {
        load64(&mut s, D, P, 8 * i, true);
        s.local_set(W + i as u32);
    }
    for i in 0..80u32 {
        let w = |k: u32| W + (i + 16 - k) % 16;
        if i >= 16 {
            // w[i] = s1(w[i-2]) + w[i-7] + s0(w[i-15]) + w[i-16]; s1 = rotr19 ^ rotr61 ^ shr6; s0 = rotr1 ^ rotr8 ^ shr7
            s.local_get(w(2)).local_set(X);
            s.local_get(X);
            rotr(&mut s, 19);
            s.local_get(X);
            rotr(&mut s, 61);
            s.i64_xor().local_get(X).i64_const(6).i64_shr_u().i64_xor();
            s.local_get(w(7)).i64_add();
            s.local_get(w(15)).local_set(X);
            s.local_get(X);
            rotr(&mut s, 1);
            s.local_get(X);
            rotr(&mut s, 8);
            s.i64_xor().local_get(X).i64_const(7).i64_shr_u().i64_xor();
            s.i64_add();
            s.local_get(w(16)).i64_add().local_set(w(16));
        }
        let role = |k: u32| A + (k + 8 - i % 8) % 8;
        let (ra, rb, rc, rd, re, rf, rg, rh) = (role(0), role(1), role(2), role(3), role(4), role(5), role(6), role(7));
        s.local_get(rh);
        s.local_get(re);
        rotr(&mut s, 14);
        s.local_get(re);
        rotr(&mut s, 18);
        s.i64_xor().local_get(re);
        rotr(&mut s, 41);
        s.i64_xor().i64_add();
        s.local_get(rg).local_get(rf).local_get(rg).i64_xor().local_get(re).i64_and().i64_xor().i64_add();
        s.i64_const(SHA512_K[i as usize] as i64).i64_add();
        s.local_get(w(16)).i64_add();
        s.local_set(T1);
        s.local_get(rd).local_get(T1).i64_add().local_set(rd);
        s.local_get(ra);
        rotr(&mut s, 28);
        s.local_get(ra);
        rotr(&mut s, 34);
        s.i64_xor().local_get(ra);
        rotr(&mut s, 39);
        s.i64_xor();
        s.local_get(ra).local_get(rb).i64_and();
        s.local_get(rc).local_get(ra).local_get(rb).i64_or().i64_and().i64_or();
        s.i64_add().local_get(T1).i64_add().local_set(rh);
    }
    for k in 0..8 {
        load64(&mut s, ST, STB, 8 * k, true);
        s.local_get(A + k as u32).i64_add().local_set(X);
        store64(&mut s, ST, STB, 8 * k, X, true);
    }
    s.local_get(P).i32_const(128).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    s.end();
    f
}

/// Keccak's rho offsets, `RHO[x + 5 * y]`.
const RHO: [i64; 25] = [0, 1, 62, 28, 27, 36, 44, 6, 55, 20, 3, 10, 43, 25, 39, 41, 45, 15, 21, 8, 18, 2, 61, 56, 14];

/// `keccak_blocks(state, data, offset, blocks, rate)`: Keccak-f[1600] for the SHA-3 family. The state is the 200 bytes of the 25
/// lanes (little-endian). Each of `blocks` blocks of `rate` bytes of `data` is xored into the state, then it is permuted; with
/// `rate` 0 the state is permuted once (squeezing more output). The round constants come from the specification's LFSR.
pub fn keccak_blocks() -> Function {
    // Parameters: state, data, offset, blocks, rate.
    const BLOCKS: u32 = 3;
    const RATE: u32 = 4;
    const ST: u32 = 5;
    const D: u32 = 6;
    const P: u32 = 7;
    const STB: u32 = 8;
    const ROUND: u32 = 9;
    const LFSR: u32 = 10;
    const J: u32 = 11;
    const ONCE: u32 = 12;
    // i64 locals: rc, then A[25], B[25], C[5], D[5].
    const RC: u32 = 13;
    const A: u32 = 14;
    const B: u32 = A + 25;
    const C: u32 = B + 25;
    const DD: u32 = C + 5;
    let mut f = Function::new([(2, rt(true, T_BYTES)), (6, ValType::I32), (1 + 25 + 25 + 5 + 5, ValType::I64)]);
    let mut s = f.instructions();
    prologue(&mut s, ST, D, P, STB);
    s.i32_const(1).local_set(LFSR);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    // rate 0: one permutation without input; else while blocks remain.
    s.local_get(RATE).i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(ONCE).br_if(2);
    s.i32_const(1).local_set(ONCE);
    s.else_();
    s.local_get(BLOCKS).i32_eqz().br_if(2);
    s.end();
    for k in 0..25 {
        load64(&mut s, ST, STB, 8 * k, false);
        s.local_set(A + k as u32);
    }
    // xor the block in: lane k for k < rate / 8
    for k in 0..25 {
        s.i32_const(8 * k + 8).local_get(RATE).i32_le_u();
        s.if_(BlockType::Empty);
        s.local_get(A + k as u32);
        load64(&mut s, D, P, 8 * k, false);
        s.i64_xor().local_set(A + k as u32);
        s.end();
    }
    s.i32_const(0).local_set(ROUND);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(ROUND).i32_const(24).i32_ge_u().br_if(1);
    // theta
    for x in 0..5u32 {
        s.local_get(A + x).local_get(A + x + 5).i64_xor().local_get(A + x + 10).i64_xor();
        s.local_get(A + x + 15).i64_xor().local_get(A + x + 20).i64_xor().local_set(C + x);
    }
    for x in 0..5u32 {
        s.local_get(C + (x + 4) % 5).local_get(C + (x + 1) % 5).i64_const(1).i64_rotl().i64_xor().local_set(DD + x);
    }
    for y in 0..5u32 {
        for x in 0..5u32 {
            s.local_get(A + x + 5 * y).local_get(DD + x).i64_xor().local_set(A + x + 5 * y);
        }
    }
    // rho and pi: B[y + 5 * ((2x + 3y) % 5)] = rotl(A[x + 5y], RHO[x + 5y])
    for y in 0..5u32 {
        for x in 0..5u32 {
            let from = x + 5 * y;
            let to = y + 5 * ((2 * x + 3 * y) % 5);
            s.local_get(A + from);
            if RHO[from as usize] != 0 {
                s.i64_const(RHO[from as usize]).i64_rotl();
            }
            s.local_set(B + to);
        }
    }
    // chi
    for y in 0..5u32 {
        for x in 0..5u32 {
            s.local_get(B + x + 5 * y);
            s.local_get(B + (x + 1) % 5 + 5 * y).i64_const(-1).i64_xor().local_get(B + (x + 2) % 5 + 5 * y).i64_and();
            s.i64_xor().local_set(A + x + 5 * y);
        }
    }
    // iota: the round constant has bit 2^j - 1 set when the LFSR's output bit at step 7 * round + j is 1
    s.i64_const(0).local_set(RC);
    s.i32_const(0).local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(J).i32_const(7).i32_ge_u().br_if(1);
    s.local_get(LFSR).i32_const(1).i32_and();
    s.if_(BlockType::Empty);
    s.local_get(RC).i64_const(1).i32_const(1).local_get(J).i32_shl().i32_const(1).i32_sub().i64_extend_i32_u().i64_shl().i64_xor().local_set(RC);
    s.end();
    s.local_get(LFSR).i32_const(1).i32_shl().local_get(LFSR).i32_const(0x80).i32_and();
    s.if_(BlockType::Result(ValType::I32)).i32_const(0x71).else_().i32_const(0).end();
    s.i32_xor().i32_const(0xff).i32_and().local_set(LFSR);
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(A).local_get(RC).i64_xor().local_set(A);
    s.local_get(ROUND).i32_const(1).i32_add().local_set(ROUND);
    s.br(0).end().end();
    // the LFSR has advanced 7 * 24 steps; it must restart for the next permutation
    s.i32_const(1).local_set(LFSR);
    for k in 0..25 {
        store64(&mut s, ST, STB, 8 * k, A + k as u32, false);
    }
    s.local_get(P).local_get(RATE).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    s.end();
    f
}

/// The `i32s` (`__intrinsic_i32_new`) in jsval local `l`, in local `to`.
fn i32s_of(s: &mut InstructionSink, l: u32, to: u32) {
    s.local_get(l).ref_cast_non_null(HeapType::Concrete(T_I32S)).local_set(to);
}

/// AES's S-box, computed (FIPS 197 5.1.1).
fn aes_sbox() -> [u8; 256] {
    fn mul(mut a: u8, mut b: u8) -> u8 {
        let mut p = 0;
        while b != 0 {
            if b & 1 != 0 {
                p ^= a;
            }
            let hi = a & 0x80;
            a <<= 1;
            if hi != 0 {
                a ^= 0x1b;
            }
            b >>= 1;
        }
        p
    }
    let mut sbox = [0u8; 256];
    for x in 0..256usize {
        // inverse by x^254
        let mut inv = 1u8;
        let mut base = x as u8;
        let mut e = 254;
        while e > 0 {
            if e & 1 != 0 {
                inv = mul(inv, base);
            }
            base = mul(base, base);
            e >>= 1;
        }
        if x == 0 {
            inv = 0;
        }
        let mut y = inv;
        let mut r = inv;
        for _ in 0..4 {
            y = y.rotate_left(1);
            r ^= y;
        }
        sbox[x] = r ^ 0x63;
    }
    sbox
}

/// A byte permutation `idx` as a `v128.const` for `i8x16.swizzle`.
fn perm(idx: [u8; 16]) -> i128 {
    u128::from_le_bytes(idx) as i128
}

/// `out[op + k..k + 8] = data[dp + k..] ^ word` (little-endian), the word in i64 local `v`.
fn store_xor(s: &mut InstructionSink, out: u32, op: u32, k: i32, v: u32, data: u32, dp: u32, dk: i32) {
    let _ = dk;
    for j in 0..8i32 {
        s.local_get(out).local_get(op).i32_const(k + j).i32_add();
        s.local_get(data).local_get(dp).i32_const(k + j).i32_add().array_get_u(T_BYTES);
        s.local_get(v);
        if j > 0 {
            s.i64_const(8 * j as i64).i64_shr_u();
        }
        s.i32_wrap_i64().i32_xor().array_set(T_BYTES);
    }
}

/// `aes_ctr(rk, rounds, st, data, data_offset, out, out_offset, len)`: `out[i] = data[i] ^ keystream` for `len` bytes, the keystream being
/// AES-encrypted counter blocks whose last 32 bits count up (GCM; CTR with a carry past 32 bits is not done here). `rk`: the round keys
/// as an `i32s` (big-endian words); `st`: a `Uint8Array` of 33 bytes, the next counter block, the current keystream block and how many of
/// its bytes are used (16 = none left). The cipher is SIMD: SubBytes is sixteen `i8x16.swizzle`s over the S-box's rows (an index of 16 or
/// more reads 0, so `x - 16 h` selects row `h` alone), ShiftRows and MixColumns more swizzles and xors, the round keys sit in `v128` locals.
pub fn aes_ctr() -> Function {
    const RKV: u32 = 0;
    const ROUNDS: u32 = 1;
    const STV: u32 = 2;
    const DATAV: u32 = 3;
    const DOFF: u32 = 4;
    const OUTV: u32 = 5;
    const OOFF: u32 = 6;
    const LEN: u32 = 7;
    const RK: u32 = 8;
    const ST: u32 = 9;
    const DATA: u32 = 10;
    const OUT: u32 = 11;
    const STB: u32 = 12;
    const DP: u32 = 13;
    const OP: u32 = 14;
    const USED: u32 = 15;
    const N: u32 = 16;
    const J: u32 = 17;
    const X: u32 = 18;
    const LO: u32 = 19;
    const HI: u32 = 20;
    const CTR: u32 = 21;
    const S: u32 = 22;
    const Y: u32 = 23;
    const ACC: u32 = 24;
    const R: u32 = 25; // r0..r3 = 25..28
    const K: u32 = 29; // k0..k14 = 29..43
    let mut f = Function::new([
        (1, rt(true, T_I32S)),
        (3, rt(true, T_BYTES)),
        (7, ValType::I32),
        (2, ValType::I64),
        (4 + 4 + 15, ValType::V128),
    ]);
    let mut s = f.instructions();
    i32s_of(&mut s, RKV, RK);
    bytes_of(&mut s, STV, ST);
    bytes_of(&mut s, DATAV, DATA);
    bytes_of(&mut s, OUTV, OUT);
    offset_of(&mut s, STV);
    s.local_set(STB);
    offset_of(&mut s, DATAV);
    s.local_get(DOFF).i32_add().local_set(DP);
    offset_of(&mut s, OUTV);
    s.local_get(OOFF).i32_add().local_set(OP);
    s.local_get(ST).local_get(STB).i32_const(32).i32_add().array_get_u(T_BYTES).local_set(USED);

    let sbox = aes_sbox();
    let rev32 = perm([3, 2, 1, 0, 7, 6, 5, 4, 11, 10, 9, 8, 15, 14, 13, 12]);
    let shift_rows = |k: usize| -> i128 {
        let mut idx = [0u8; 16];
        for c in 0..4 {
            for r in 0..4 {
                let r2 = (r + k) % 4;
                idx[4 * c + r] = (4 * ((c + r2) % 4) + r2) as u8;
            }
        }
        perm(idx)
    };
    // The round keys: four words → one vector (bytes in word order), only as many as the key needs.
    let load_key = |s: &mut InstructionSink, i: u32| {
        s.local_get(RK).i32_const(4 * i as i32).array_get(T_I32S).i32x4_splat();
        for lane in 1..4u8 {
            s.local_get(RK).i32_const(4 * i as i32 + lane as i32).array_get(T_I32S).i32x4_replace_lane(lane);
        }
        s.v128_const(rev32).i8x16_swizzle().local_set(K + i);
    };
    for i in 0..11 {
        load_key(&mut s, i);
    }
    s.local_get(ROUNDS).i32_const(12).i32_ge_u();
    s.if_(BlockType::Empty);
    load_key(&mut s, 11);
    load_key(&mut s, 12);
    s.end();
    s.local_get(ROUNDS).i32_const(14).i32_ge_u();
    s.if_(BlockType::Empty);
    load_key(&mut s, 13);
    load_key(&mut s, 14);
    s.end();
    // The counter block.
    load64(&mut s, ST, STB, 0, false);
    s.i64x2_splat();
    load64(&mut s, ST, STB, 8, false);
    s.i64x2_replace_lane(1).local_set(CTR);

    // SubBytes of the vector in local `S` into `ACC`.
    let sub_bytes = |s: &mut InstructionSink| {
        s.v128_const(perm(sbox[0..16].try_into().unwrap())).local_get(S).i8x16_swizzle().local_set(ACC);
        s.local_get(S).local_set(Y);
        for h in 1..16 {
            s.local_get(Y).v128_const(perm([16; 16])).i8x16_sub().local_tee(Y);
            s.local_set(R); // scratch
            s.v128_const(perm(sbox[16 * h..16 * h + 16].try_into().unwrap())).local_get(R).i8x16_swizzle();
            s.local_get(ACC).v128_or().local_set(ACC);
        }
    };

    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(LEN).i32_eqz().br_if(1);
    s.local_get(USED).i32_const(16).i32_eq();
    s.if_(BlockType::Empty);
    // The next keystream block into st[16..32].
    s.local_get(CTR).local_get(K).v128_xor().local_set(S);
    s.block(BlockType::Empty);
    for r in 1..=14u32 {
        if r == 10 || r == 12 {
            s.local_get(ROUNDS).i32_const(r as i32).i32_eq();
            s.if_(BlockType::Empty);
            sub_bytes(&mut s);
            s.local_get(ACC).v128_const(shift_rows(0)).i8x16_swizzle().local_get(K + r).v128_xor().local_set(S);
            s.br(1);
            s.end();
        }
        if r == 14 {
            sub_bytes(&mut s);
            s.local_get(ACC).v128_const(shift_rows(0)).i8x16_swizzle().local_get(K + r).v128_xor().local_set(S);
            break;
        }
        sub_bytes(&mut s);
        for k in 0..4 {
            s.local_get(ACC).v128_const(shift_rows(k)).i8x16_swizzle().local_set(R + k as u32);
        }
        // xt(r0 ^ r1) ^ r1 ^ r2 ^ r3 ^ key; xt(a) = (a << 1) ^ (0x1b & (a >>s 7)) per byte
        s.local_get(R).local_get(R + 1).v128_xor().local_tee(S);
        s.i32_const(7).i8x16_shr_s().v128_const(perm([0x1b; 16])).v128_and();
        s.local_get(S).i32_const(1).i8x16_shl().v128_xor();
        s.local_get(R + 1).v128_xor().local_get(R + 2).v128_xor().local_get(R + 3).v128_xor();
        s.local_get(K + r).v128_xor().local_set(S);
    }
    s.end();
    // counter: the last 32 bits (big-endian in the block) + 1
    s.local_get(CTR).i32x4_extract_lane(3).local_set(X);
    // bswap(X) + 1, then bswap back
    let bswap = |s: &mut InstructionSink| {
        s.local_tee(X).i32_const(8).i32_rotl().i32_const(0x00ff00ff).i32_and();
        s.local_get(X).i32_const(24).i32_rotl().i32_const(0xff00ff00u32 as i32).i32_and().i32_or();
    };
    s.local_get(X);
    bswap(&mut s);
    s.i32_const(1).i32_add();
    bswap(&mut s);
    s.local_set(X);
    s.local_get(CTR).local_get(X).i32x4_replace_lane(3).local_set(CTR);
    s.local_get(S).i64x2_extract_lane(0).local_set(LO);
    s.local_get(S).i64x2_extract_lane(1).local_set(HI);
    // A whole block left: xor it with the data now, no keystream kept.
    s.local_get(LEN).i32_const(16).i32_ge_u();
    s.if_(BlockType::Empty);
    store_xor(&mut s, OUT, OP, 0, LO, DATA, DP, 0);
    store_xor(&mut s, OUT, OP, 8, HI, DATA, DP, 8);
    s.local_get(DP).i32_const(16).i32_add().local_set(DP);
    s.local_get(OP).i32_const(16).i32_add().local_set(OP);
    s.local_get(LEN).i32_const(16).i32_sub().local_set(LEN);
    s.br(2);
    s.end();
    store64(&mut s, ST, STB, 16, LO, false);
    store64(&mut s, ST, STB, 24, HI, false);
    s.i32_const(0).local_set(USED);
    s.end();
    // N = min(len, 16 - used); out[i] = data[i] ^ keystream
    s.i32_const(16).local_get(USED).i32_sub().local_tee(N);
    s.local_get(LEN).local_get(N).local_get(LEN).i32_lt_u().select().local_set(N);
    s.i32_const(0).local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(J).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT).local_get(OP).local_get(J).i32_add();
    s.local_get(DATA).local_get(DP).local_get(J).i32_add().array_get_u(T_BYTES);
    s.local_get(ST).local_get(STB).i32_const(16).i32_add().local_get(USED).i32_add().local_get(J).i32_add().array_get_u(T_BYTES);
    s.i32_xor().array_set(T_BYTES);
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(DP).local_get(N).i32_add().local_set(DP);
    s.local_get(OP).local_get(N).i32_add().local_set(OP);
    s.local_get(USED).local_get(N).i32_add().local_set(USED);
    s.local_get(LEN).local_get(N).i32_sub().local_set(LEN);
    s.br(0).end().end();
    // The counter goes back into st[0..16] so the next call continues.
    s.local_get(CTR).i64x2_extract_lane(0).local_set(LO);
    s.local_get(CTR).i64x2_extract_lane(1).local_set(HI);
    store64(&mut s, ST, STB, 0, LO, false);
    store64(&mut s, ST, STB, 8, HI, false);
    s.local_get(ST).local_get(STB).i32_const(32).i32_add().local_get(USED).array_set(T_BYTES);
    s.end();
    f
}

/// The bit reversal of the i64 on the stack.
fn rev64(s: &mut InstructionSink, t: u32) {
    // swap bits, pairs, nibbles, then the bytes (shifts and masks, then a byte swap by 16- and 32-bit rotations)
    s.local_set(t);
    for (shift, mask) in [(1i64, 0x5555555555555555u64), (2, 0x3333333333333333), (4, 0x0f0f0f0f0f0f0f0f)] {
        s.local_get(t).i64_const(mask as i64).i64_and().i64_const(shift).i64_shl();
        s.local_get(t).i64_const(shift).i64_shr_u().i64_const(mask as i64).i64_and();
        s.i64_or().local_set(t);
    }
    for (shift, mask) in [(8i64, 0x00ff00ff00ff00ffu64), (16, 0x0000ffff0000ffff)] {
        s.local_get(t).i64_const(mask as i64).i64_and().i64_const(shift).i64_shl();
        s.local_get(t).i64_const(shift).i64_shr_u().i64_const(mask as i64).i64_and();
        s.i64_or().local_set(t);
    }
    s.local_get(t).i64_const(32).i64_rotl();
}

/// The low 64 bits of the carry-less product of the i64 locals `x` and `y` (BearSSL's `bmul64`: four bit-interleaved integer multiplications
/// whose carries cannot collide), temporaries `tmp[0..12]`; the result is left on the stack.
fn bmul64(s: &mut InstructionSink, x: u32, y: u32, tmp: u32) {
    const M: u64 = 0x1111111111111111;
    for k in 0..4u32 {
        s.local_get(x).i64_const((M << k) as i64).i64_and().local_set(tmp + k);
        s.local_get(y).i64_const((M << k) as i64).i64_and().local_set(tmp + 4 + k);
    }
    // z_k = sum over i + j = k (mod 4) of x_i * y_j
    for k in 0..4u32 {
        for i in 0..4u32 {
            let j = (k + 4 - i) % 4;
            s.local_get(tmp + i).local_get(tmp + 4 + j).i64_mul();
            if i > 0 {
                s.i64_xor();
            }
        }
        s.i64_const((M << k) as i64).i64_and();
        if k > 0 {
            s.i64_or();
        }
    }
}

/// `ghash_init(h, tab)`: the hash key `h` (16 bytes) in the form `ghash_blocks` multiplies by, into the `i32s` `tab` (at least 8 words):
/// `[h0, h1, H_hi, H_lo]` as i64s (low word first), `h0`/`h1` being the bit reversals of the big-endian halves.
pub fn ghash_init() -> Function {
    const HV: u32 = 0;
    const TABV: u32 = 1;
    const H: u32 = 2;
    const TAB: u32 = 3;
    const HB: u32 = 4;
    const HI: u32 = 5;
    const LO: u32 = 6;
    const T: u32 = 7;
    let mut f = Function::new([(1, rt(true, T_BYTES)), (1, rt(true, T_I32S)), (1, ValType::I32), (4, ValType::I64)]);
    let mut s = f.instructions();
    bytes_of(&mut s, HV, H);
    i32s_of(&mut s, TABV, TAB);
    offset_of(&mut s, HV);
    s.local_set(HB);
    load64(&mut s, H, HB, 0, true);
    s.local_set(HI);
    load64(&mut s, H, HB, 8, true);
    s.local_set(LO);
    let put = |s: &mut InstructionSink, at: i32, v: u32| {
        s.local_get(TAB).i32_const(at).local_get(v).i32_wrap_i64().array_set(T_I32S);
        s.local_get(TAB).i32_const(at + 1).local_get(v).i64_const(32).i64_shr_u().i32_wrap_i64().array_set(T_I32S);
    };
    s.local_get(HI);
    rev64(&mut s, T);
    s.local_set(T + 1);
    put(&mut s, 0, T + 1);
    s.local_get(LO);
    rev64(&mut s, T);
    s.local_set(T + 1);
    put(&mut s, 2, T + 1);
    put(&mut s, 4, HI);
    put(&mut s, 6, LO);
    s.end();
    f
}

/// `ghash_blocks(tab, st, data, offset, blocks)`: `st = (st ^ block) * H` in GF(2^128) for each 16-byte block of `data` (`st`: 16 bytes).
/// The blocks are bit-reversed into ordinary polynomials (GCM's first bit is `x^0`), multiplied by three 64-bit carry-less products
/// (Karatsuba) with the high halves taken from the product of the reversed operands, reduced by `x^128 + x^7 + x^2 + x + 1` and reversed back.
pub fn ghash_blocks() -> Function {
    const TABV: u32 = 0;
    const STV: u32 = 1;
    const DATAV: u32 = 2;
    const OFF: u32 = 3;
    const BLOCKS: u32 = 4;
    const TAB: u32 = 5;
    const ST: u32 = 6;
    const DATA: u32 = 7;
    const STB: u32 = 8;
    const P: u32 = 9;
    // i64 locals
    const XH: u32 = 10;
    const XL: u32 = 11;
    const A: u32 = 12; // a0..a2 (the operand), 12..14
    const AR: u32 = 15; // reversed operands
    const B: u32 = 18; // h0, h1, h0 ^ h1
    const BR: u32 = 21; // H_hi, H_lo, their xor
    const LOW: u32 = 24; // lo0..lo2
    const HIGH: u32 = 27; // hi0..hi2
    const TMP: u32 = 30; // 12 temporaries
    const RT: u32 = 42; // rev64's scratch
    const C: u32 = 43; // c0..c3
    const O: u32 = 47;
    const R0: u32 = 48;
    const R1: u32 = 49;
    let mut f = Function::new([(1, rt(true, T_I32S)), (2, rt(true, T_BYTES)), (2, ValType::I32), (41, ValType::I64)]);
    let mut s = f.instructions();
    i32s_of(&mut s, TABV, TAB);
    bytes_of(&mut s, STV, ST);
    bytes_of(&mut s, DATAV, DATA);
    offset_of(&mut s, STV);
    s.local_set(STB);
    offset_of(&mut s, DATAV);
    s.local_get(OFF).i32_add().local_set(P);
    // h0, h1, H_hi, H_lo from the table
    for (k, dst) in [(0, B), (2, B + 1), (4, BR), (6, BR + 1)] {
        s.local_get(TAB).i32_const(k).array_get(T_I32S).i64_extend_i32_u();
        s.local_get(TAB).i32_const(k + 1).array_get(T_I32S).i64_extend_i32_u().i64_const(32).i64_shl().i64_or().local_set(dst);
    }
    s.local_get(B).local_get(B + 1).i64_xor().local_set(B + 2);
    s.local_get(BR).local_get(BR + 1).i64_xor().local_set(BR + 2);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(BLOCKS).i32_eqz().br_if(1);
    load64(&mut s, ST, STB, 0, true);
    load64(&mut s, DATA, P, 0, true);
    s.i64_xor().local_set(XH);
    load64(&mut s, ST, STB, 8, true);
    load64(&mut s, DATA, P, 8, true);
    s.i64_xor().local_set(XL);
    // the operands and their reversals
    s.local_get(XH);
    rev64(&mut s, RT);
    s.local_set(A);
    s.local_get(XL);
    rev64(&mut s, RT);
    s.local_set(A + 1);
    s.local_get(A).local_get(A + 1).i64_xor().local_set(A + 2);
    s.local_get(XH).local_set(AR);
    s.local_get(XL).local_set(AR + 1);
    s.local_get(XH).local_get(XL).i64_xor().local_set(AR + 2);
    for k in 0..3 {
        bmul64(&mut s, A + k, B + k, TMP);
        s.local_set(LOW + k);
        bmul64(&mut s, AR + k, BR + k, TMP);
        rev64(&mut s, RT);
        s.i64_const(1).i64_shr_u().local_set(HIGH + k);
    }
    // z2 = z2 ^ z0 ^ z1; C = z0 + z2 x^64 + z1 x^128
    s.local_get(LOW).local_set(C);
    s.local_get(HIGH).local_get(LOW + 2).i64_xor().local_get(LOW).i64_xor().local_get(LOW + 1).i64_xor().local_set(C + 1);
    s.local_get(LOW + 1).local_get(HIGH + 2).i64_xor().local_get(HIGH).i64_xor().local_get(HIGH + 1).i64_xor().local_set(C + 2);
    s.local_get(HIGH + 1).local_set(C + 3);
    // reduce: v = (c2, c3); O = the bits the shifts push past x^127
    s.local_get(C + 3).i64_const(63).i64_shr_u().local_get(C + 3).i64_const(62).i64_shr_u().i64_xor();
    s.local_get(C + 3).i64_const(57).i64_shr_u().i64_xor().local_set(O);
    s.local_get(C).local_get(C + 2).i64_xor();
    for sh in [1i64, 2, 7] {
        s.local_get(C + 2).i64_const(sh).i64_shl().i64_xor();
    }
    s.local_get(O).i64_xor();
    for sh in [1i64, 2, 7] {
        s.local_get(O).i64_const(sh).i64_shl().i64_xor();
    }
    s.local_set(R0);
    s.local_get(C + 1).local_get(C + 3).i64_xor();
    for sh in [1i64, 2, 7] {
        s.local_get(C + 3).i64_const(sh).i64_shl().local_get(C + 2).i64_const(64 - sh).i64_shr_u().i64_or().i64_xor();
    }
    s.local_set(R1);
    s.local_get(R0);
    rev64(&mut s, RT);
    s.local_set(R0);
    s.local_get(R1);
    rev64(&mut s, RT);
    s.local_set(R1);
    store64(&mut s, ST, STB, 0, R0, true);
    store64(&mut s, ST, STB, 8, R1, true);
    s.local_get(P).i32_const(16).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    s.end();
    f
}

/// `chacha20_xor(st, data, data_offset, out, out_offset, len)`: `out[i] = data[i] ^ keystream` for `len` bytes of ChaCha20 (RFC 8439).
/// `st`: a `Uint8Array` of 129 bytes: the sixteen little-endian state words (constants, key, block counter, nonce), the current keystream
/// block, and how many of its bytes are used (64 = none left). The counter (word 12) carries into word 13 when it wraps, as OpenSSL's.
pub fn chacha20_xor() -> Function {
    const STV: u32 = 0;
    const DATAV: u32 = 1;
    const DOFF: u32 = 2;
    const OUTV: u32 = 3;
    const OOFF: u32 = 4;
    const LEN: u32 = 5;
    const ST: u32 = 6;
    const DATA: u32 = 7;
    const OUT: u32 = 8;
    const STB: u32 = 9;
    const DP: u32 = 10;
    const OP: u32 = 11;
    const USED: u32 = 12;
    const N: u32 = 13;
    const J: u32 = 14;
    const X: u32 = 15;
    const I: u32 = 16;
    const W: u32 = 17; // w0..w15 = 17..32
    let mut f = Function::new([(3, rt(true, T_BYTES)), (11, ValType::I32), (16, ValType::I32)]);
    let mut s = f.instructions();
    bytes_of(&mut s, STV, ST);
    bytes_of(&mut s, DATAV, DATA);
    bytes_of(&mut s, OUTV, OUT);
    offset_of(&mut s, STV);
    s.local_set(STB);
    offset_of(&mut s, DATAV);
    s.local_get(DOFF).i32_add().local_set(DP);
    offset_of(&mut s, OUTV);
    s.local_get(OOFF).i32_add().local_set(OP);
    s.local_get(ST).local_get(STB).i32_const(128).i32_add().array_get_u(T_BYTES).local_set(USED);
    let quarter = |s: &mut InstructionSink, a: u32, b: u32, c: u32, d: u32| {
        for (x, y, z, rot) in [(a, b, d, 16), (c, d, b, 12), (a, b, d, 8), (c, d, b, 7)] {
            // x += y; z ^= x; z = rotl(z, rot) -- the pairs (a b d 16) (c d b 12) (a b d 8) (c d b 7)
            s.local_get(x).local_get(y).i32_add().local_set(x);
            s.local_get(z).local_get(x).i32_xor().i32_const(rot).i32_rotl().local_set(z);
        }
    };
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(LEN).i32_eqz().br_if(1);
    s.local_get(USED).i32_const(64).i32_eq();
    s.if_(BlockType::Empty);
    for k in 0..16 {
        le32(&mut s, ST, STB, 4 * k);
        s.local_set(W + k as u32);
    }
    s.i32_const(10).local_set(I);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(I).i32_eqz().br_if(1);
    quarter(&mut s, W, W + 4, W + 8, W + 12);
    quarter(&mut s, W + 1, W + 5, W + 9, W + 13);
    quarter(&mut s, W + 2, W + 6, W + 10, W + 14);
    quarter(&mut s, W + 3, W + 7, W + 11, W + 15);
    quarter(&mut s, W, W + 5, W + 10, W + 15);
    quarter(&mut s, W + 1, W + 6, W + 11, W + 12);
    quarter(&mut s, W + 2, W + 7, W + 8, W + 13);
    quarter(&mut s, W + 3, W + 4, W + 9, W + 14);
    s.local_get(I).i32_const(1).i32_sub().local_set(I);
    s.br(0).end().end();
    for k in 0..16 {
        le32(&mut s, ST, STB, 4 * k);
        s.local_get(W + k as u32).i32_add().local_set(W + k as u32);
    }
    // block counter += 1
    le32(&mut s, ST, STB, 48);
    s.i32_const(1).i32_add().local_set(X);
    put_le32(&mut s, ST, STB, 48, X);
    // OpenSSL's `chacha20` carries into the next word (a 64-bit counter) when the first wraps.
    s.local_get(X).i32_eqz();
    s.if_(BlockType::Empty);
    le32(&mut s, ST, STB, 52);
    s.i32_const(1).i32_add().local_set(X);
    put_le32(&mut s, ST, STB, 52, X);
    s.end();
    s.local_get(LEN).i32_const(64).i32_ge_u();
    s.if_(BlockType::Empty);
    for k in 0..16 {
        le32(&mut s, DATA, DP, 4 * k);
        s.local_get(W + k as u32).i32_xor().local_set(X);
        put_le32(&mut s, OUT, OP, 4 * k, X);
    }
    s.local_get(DP).i32_const(64).i32_add().local_set(DP);
    s.local_get(OP).i32_const(64).i32_add().local_set(OP);
    s.local_get(LEN).i32_const(64).i32_sub().local_set(LEN);
    s.br(2);
    s.end();
    for k in 0..16 {
        put_le32(&mut s, ST, STB, 64 + 4 * k, W + k as u32);
    }
    s.i32_const(0).local_set(USED);
    s.end();
    // N = min(len, 64 - used); out[i] = data[i] ^ keystream
    s.i32_const(64).local_get(USED).i32_sub().local_tee(N);
    s.local_get(LEN).local_get(N).local_get(LEN).i32_lt_u().select().local_set(N);
    s.i32_const(0).local_set(J);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(J).local_get(N).i32_ge_u().br_if(1);
    s.local_get(OUT).local_get(OP).local_get(J).i32_add();
    s.local_get(DATA).local_get(DP).local_get(J).i32_add().array_get_u(T_BYTES);
    s.local_get(ST).local_get(STB).i32_const(64).i32_add().local_get(USED).i32_add().local_get(J).i32_add().array_get_u(T_BYTES);
    s.i32_xor().array_set(T_BYTES);
    s.local_get(J).i32_const(1).i32_add().local_set(J);
    s.br(0).end().end();
    s.local_get(DP).local_get(N).i32_add().local_set(DP);
    s.local_get(OP).local_get(N).i32_add().local_set(OP);
    s.local_get(USED).local_get(N).i32_add().local_set(USED);
    s.local_get(LEN).local_get(N).i32_sub().local_set(LEN);
    s.br(0).end().end();
    s.local_get(ST).local_get(STB).i32_const(128).i32_add().local_get(USED).array_set(T_BYTES);
    s.end();
    f
}

/// `poly1305_blocks(st, data, offset, blocks)`: Poly1305 over whole 16-byte blocks (each with its 2^128 bit set), 26-bit limbs as in
/// poly1305-donna. `st`: an `i32s` of 14 words: `r0..r4`, `5 * r1..5 * r4`, `h0..h4` (the accumulator, updated in place).
pub fn poly1305_blocks() -> Function {
    const STV: u32 = 0;
    const DATAV: u32 = 1;
    const OFF: u32 = 2;
    const BLOCKS: u32 = 3;
    const ST: u32 = 4;
    const DATA: u32 = 5;
    const P: u32 = 6;
    // i64 locals
    const R: u32 = 7; // r0..r4
    const S: u32 = 12; // s1..s4 (S + k - 1)
    const H: u32 = 16; // h0..h4
    const T: u32 = 21; // t0..t3
    const D: u32 = 25; // d0..d4
    const C: u32 = 30;
    let mut f = Function::new([(1, rt(true, T_I32S)), (1, rt(true, T_BYTES)), (1, ValType::I32), (25, ValType::I64)]);
    let mut s = f.instructions();
    i32s_of(&mut s, STV, ST);
    bytes_of(&mut s, DATAV, DATA);
    offset_of(&mut s, DATAV);
    s.local_get(OFF).i32_add().local_set(P);
    for k in 0..5u32 {
        s.local_get(ST).i32_const(k as i32).array_get(T_I32S).i64_extend_i32_u().local_set(R + k);
        s.local_get(ST).i32_const(9 + k as i32).array_get(T_I32S).i64_extend_i32_u().local_set(H + k);
    }
    for k in 1..5u32 {
        s.local_get(ST).i32_const(4 + k as i32).array_get(T_I32S).i64_extend_i32_u().local_set(S + k - 1);
    }
    let mask = 0x3ffffffi64;
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(BLOCKS).i32_eqz().br_if(1);
    for k in 0..4 {
        le32(&mut s, DATA, P, 4 * k);
        s.i64_extend_i32_u().local_set(T + k as u32);
    }
    // h += block
    s.local_get(H).local_get(T).i64_const(mask).i64_and().i64_add().local_set(H);
    s.local_get(H + 1).local_get(T).i64_const(26).i64_shr_u().local_get(T + 1).i64_const(6).i64_shl().i64_or().i64_const(mask).i64_and().i64_add().local_set(H + 1);
    s.local_get(H + 2).local_get(T + 1).i64_const(20).i64_shr_u().local_get(T + 2).i64_const(12).i64_shl().i64_or().i64_const(mask).i64_and().i64_add().local_set(H + 2);
    s.local_get(H + 3).local_get(T + 2).i64_const(14).i64_shr_u().local_get(T + 3).i64_const(18).i64_shl().i64_or().i64_const(mask).i64_and().i64_add().local_set(H + 3);
    s.local_get(H + 4).local_get(T + 3).i64_const(8).i64_shr_u().i64_const(1 << 24).i64_or().i64_add().local_set(H + 4);
    // d_k = sum h_i * (r_{k-i} or 5 * r_{k-i+5})
    for k in 0..5u32 {
        for i in 0..5u32 {
            let m = (k + 5 - i) % 5; // r index
            let coef = if i + m == k { R + m } else { S + m - 1 };
            s.local_get(H + i).local_get(coef).i64_mul();
            if i > 0 {
                s.i64_add();
            }
        }
        s.local_set(D + k);
    }
    // carries
    s.i64_const(0).local_set(C);
    for k in 0..5u32 {
        s.local_get(D + k).local_get(C).i64_add().local_set(D + k);
        s.local_get(D + k).i64_const(26).i64_shr_u().local_set(C);
        s.local_get(D + k).i64_const(mask).i64_and().local_set(H + k);
    }
    s.local_get(H).local_get(C).i64_const(5).i64_mul().i64_add().local_set(H);
    s.local_get(H).i64_const(26).i64_shr_u().local_set(C);
    s.local_get(H).i64_const(mask).i64_and().local_set(H);
    s.local_get(H + 1).local_get(C).i64_add().local_set(H + 1);
    s.local_get(P).i32_const(16).i32_add().local_set(P);
    s.local_get(BLOCKS).i32_const(1).i32_sub().local_set(BLOCKS);
    s.br(0).end().end();
    for k in 0..5u32 {
        s.local_get(ST).i32_const(9 + k as i32).local_get(H + k).i32_wrap_i64().array_set(T_I32S);
    }
    s.end();
    f
}
