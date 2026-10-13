//! T114: the regex interpreter. Regex literals in the compiled subset are compiled at build time into programs of
//! 16-bit words (`lower/src/regex.rs` has the format and the compiler), passed to the prelude as string constants;
//! `re_exec(prog, s, from, mode, caps) -> i32` runs one (`Intrinsic::ReExec`): mode 0 searches from `from`, mode 1
//! tries `from` only; the result is the match end or -1, with `caps` as regexp.js's `$$reRun` leaves it (`[0]`,
//! `[1]` the match, then the program's slots: captures, then loop registers).
//!
//! As Hermes (build-time regex bytecode and an interpreter, no native code per regex). The search first skips
//! starts no match can have (the program's header: `^` without `m`, a literal every match contains, the units at
//! fixed offsets: Rust regex's prefilters, irregexp's lookahead). The matcher backtracks with an explicit stack of
//! three-word entries (`G_RE_STACK`, reused): a choice `[pc << 2 | tag, pos, x]` (tag 0 resume at `pc`, 1 a greedy
//! loop at `pc` gives back a unit down to `x`, 2 a lazy loop at `pc` takes one more, `x` its count) or an undo
//! `[-1 - slot, old, 0]` (logged only while a choice is on the stack), as regexp.js's VM.

use crate::runtime::{G_RE_STACK, T_I32S, T_STRING};
use wasm_encoder::{BlockType, Function, HeapType, Instruction as I, RefType, ValType};

// Opcode count and header words (`lower/src/regex.rs`).
const NOPS: u32 = 17;
const H_NCAP: i32 = 1;
const H_FLAGS: i32 = 2;
const H_CODE: i32 = 3;
const H_NEED: i32 = 4;
const H_NSETS: i32 = 5;
const NONE: i32 = 0xffff;

// Parameters.
const P: u32 = 0;
const S: u32 = 1;
const FROM: u32 = 2;
const MODE: u32 = 3;
const CAPS: u32 = 4;
// Locals.
const LEN: u32 = 5;
const START: u32 = 6;
const POS: u32 = 7;
const SP: u32 = 8;
const PC: u32 = 9;
const C: u32 = 10;
const X: u32 = 11;
const Y: u32 = 12;
const T: u32 = 13;
const Q: u32 = 14;
const I_: u32 = 15;
const K: u32 = 16;
const N: u32 = 17;
const R: u32 = 18;
/// The search's sets: offsets and classes (`O0 + j`, `K0 + j`).
const O0: u32 = 19;
const K0: u32 = 23;
const STACK: u32 = 27;
/// The last start the sets leave room for; set 0's ASCII part as a 128-bit mask (`M0` units 0–63, `M1` 64–127).
const LIMIT: u32 = 28;
const M0: u32 = 29;
const M1: u32 = 30;

/// `re_grow(stack, need) -> stack`: a backtrack stack with room for `need` words.
pub fn grow() -> Function {
    let i32s = HeapType::Concrete(T_I32S);
    let mut f = Function::new([(1, ValType::Ref(RefType { nullable: false, heap_type: i32s }))]);
    let mut s = f.instructions();
    s.local_get(1).i32_const(1).i32_shl().i32_const(96).local_get(1).i32_const(1).i32_shl();
    s.i32_const(96).i32_gt_u().select();
    s.array_new_default(T_I32S).local_set(2);
    s.local_get(0).ref_is_null().i32_eqz();
    s.if_(BlockType::Empty);
    s.local_get(2).i32_const(0).local_get(0).i32_const(0).local_get(0).array_len();
    s.array_copy(T_I32S, T_I32S);
    s.end();
    s.local_get(2);
    s.end();
    f
}

/// `re_class_wide(prog, cls, c) -> i32`: unit `c` (≥ 128) in class `cls`'s ranges (a binary search).
pub fn class_wide() -> Function {
    // Locals: 3 lo, 4 hi, 5 mid, 6 the first range.
    let mut f = Function::new([(4, ValType::I32)]);
    let mut s = f.instructions();
    s.local_get(1).i32_const(9).i32_add().local_set(6);
    s.local_get(0).local_get(1).array_get_u(T_STRING).local_set(4);
    s.block(BlockType::Empty).loop_(BlockType::Empty);
    s.local_get(3).local_get(4).i32_ge_u().br_if(1);
    s.local_get(3).local_get(4).i32_add().i32_const(1).i32_shr_u().local_set(5);
    s.local_get(2).local_get(0).local_get(6).local_get(5).i32_const(1).i32_shl().i32_add();
    s.array_get_u(T_STRING).i32_lt_u();
    s.if_(BlockType::Empty).local_get(5).local_set(4).br(1).end();
    s.local_get(2).local_get(0).local_get(6).local_get(5).i32_const(1).i32_shl().i32_add();
    s.i32_const(1).i32_add().array_get_u(T_STRING).i32_gt_u();
    s.if_(BlockType::Empty).local_get(5).i32_const(1).i32_add().local_set(3).br(1).end();
    s.i32_const(1).return_();
    s.end().end();
    s.i32_const(0);
    s.end();
    f
}

/// Instructions with named labels: `block`/`lp`/`if_` return a label, `br`/`br_if` take one.
struct B {
    v: Vec<I<'static>>,
    depth: u32,
    wide: u32,
    grow: u32,
}

type Label = u32;

impl B {
    fn i(&mut self, x: I<'static>) -> &mut Self {
        self.v.push(x);
        self
    }
    fn is(&mut self, xs: impl IntoIterator<Item = I<'static>>) -> &mut Self {
        self.v.extend(xs);
        self
    }
    fn open(&mut self, x: I<'static>) -> Label {
        self.v.push(x);
        self.depth += 1;
        self.depth - 1
    }
    fn block(&mut self) -> Label {
        self.open(I::Block(BlockType::Empty))
    }
    fn lp(&mut self) -> Label {
        self.open(I::Loop(BlockType::Empty))
    }
    fn if_(&mut self) -> Label {
        self.open(I::If(BlockType::Empty))
    }
    fn end(&mut self) {
        self.v.push(I::End);
        self.depth -= 1;
    }
    fn br(&mut self, l: Label) {
        self.v.push(I::Br(self.depth - 1 - l));
    }
    fn br_if(&mut self, l: Label) {
        self.v.push(I::BrIf(self.depth - 1 - l));
    }
    /// Push the program's word at `at`.
    fn prog(&mut self, at: impl IntoIterator<Item = I<'static>>) {
        self.i(I::LocalGet(P)).is(at).i(I::ArrayGetU(T_STRING));
    }
    /// Push the unit of the string at `at`.
    fn unit(&mut self, at: impl IntoIterator<Item = I<'static>>) {
        self.i(I::LocalGet(S)).is(at).i(I::ArrayGetU(T_STRING));
    }
    /// Push 1 if unit `C` is in the class at word `K`: the ASCII bitmap inline, `re_class_wide` above 127.
    fn class(&mut self) {
        self.is([I::LocalGet(C), I::I32Const(128), I::I32LtU, I::If(BlockType::Result(ValType::I32))]);
        self.depth += 1;
        self.prog([
            I::LocalGet(K),
            I::I32Const(1),
            I::I32Add,
            I::LocalGet(C),
            I::I32Const(4),
            I::I32ShrU,
            I::I32Add,
        ]);
        self.is([I::LocalGet(C), I::I32Const(15), I::I32And, I::I32ShrU, I::I32Const(1), I::I32And]);
        self.i(I::Else);
        self.is([I::LocalGet(P), I::LocalGet(K), I::LocalGet(C), I::Call(self.wide)]);
        self.end();
    }
    /// Push the entry `[kind, a, b]` (each given as instructions).
    fn push(&mut self, kind: Vec<I<'static>>, a: Vec<I<'static>>, b: Vec<I<'static>>) {
        self.is([I::LocalGet(SP), I::I32Const(3), I::I32Add, I::LocalGet(STACK), I::ArrayLen, I::I32GtU]);
        self.if_();
        self.is([I::LocalGet(STACK), I::LocalGet(SP), I::I32Const(3), I::I32Add, I::Call(self.grow)]);
        self.is([I::LocalTee(STACK), I::GlobalSet(G_RE_STACK)]);
        self.end();
        self.is([I::LocalGet(STACK), I::LocalGet(SP)]).is(kind).i(I::ArraySet(T_I32S));
        self.is([I::LocalGet(STACK), I::LocalGet(SP), I::I32Const(1), I::I32Add]).is(a).i(I::ArraySet(T_I32S));
        self.is([I::LocalGet(STACK), I::LocalGet(SP), I::I32Const(2), I::I32Add]).is(b).i(I::ArraySet(T_I32S));
        self.is([I::LocalGet(SP), I::I32Const(3), I::I32Add, I::LocalSet(SP)]);
    }
    /// Slot `j` (`caps[2 + j]`; `j` given as instructions) = `v`, logging the old value while a choice is on
    /// the stack. Uses `T`.
    fn set_slot(&mut self, j: Vec<I<'static>>, v: Vec<I<'static>>) {
        self.is(j).i(I::LocalSet(T));
        self.i(I::LocalGet(SP));
        self.if_();
        self.push(
            vec![I::I32Const(-1), I::LocalGet(T), I::I32Sub],
            vec![I::LocalGet(CAPS), I::LocalGet(T), I::I32Const(2), I::I32Add, I::ArrayGet(T_I32S)],
            vec![I::I32Const(0)],
        );
        self.end();
        self.is([I::LocalGet(CAPS), I::LocalGet(T), I::I32Const(2), I::I32Add]).is(v).i(I::ArraySet(T_I32S));
    }
    /// Push slot `j` (`caps[2 + j]`).
    fn slot(&mut self, j: Vec<I<'static>>) {
        self.i(I::LocalGet(CAPS)).is(j).is([I::I32Const(2), I::I32Add, I::ArrayGet(T_I32S)]);
    }
    /// `PC += n`, then on to the next op.
    fn next(&mut self, n: i32, run: Label) {
        self.is([I::LocalGet(PC), I::I32Const(n), I::I32Add, I::LocalSet(PC)]);
        self.br(run);
    }
    /// Push operand `k` of the op at `PC`.
    fn arg(&mut self, k: i32) {
        self.prog([I::LocalGet(PC), I::I32Const(k), I::I32Add]);
    }
    /// Push whether `C` is a word unit (`\w` without `u`).
    fn word(&mut self) {
        self.is([I::LocalGet(C), I::I32Const(48), I::I32Sub, I::I32Const(9), I::I32LeU]);
        self.is([I::LocalGet(C), I::I32Const(32), I::I32Or, I::I32Const(97), I::I32Sub, I::I32Const(25), I::I32LeU]);
        self.i(I::I32Or);
        self.is([I::LocalGet(C), I::I32Const(95), I::I32Eq, I::I32Or]);
    }
    /// Push whether `C` is a line terminator.
    fn line_terminator(&mut self) {
        self.is([I::LocalGet(C), I::I32Const(10), I::I32Eq, I::LocalGet(C), I::I32Const(13), I::I32Eq, I::I32Or]);
        self.is([I::LocalGet(C), I::I32Const(0x2028), I::I32Sub, I::I32Const(1), I::I32LeU, I::I32Or]);
    }
    /// Backtrack when the unit at `POS` (read into `C`) isn't in the class at operand 1; else `POS += 1`.
    fn one_of_class(&mut self, bt: Label) {
        self.is([I::LocalGet(POS), I::LocalGet(LEN), I::I32GeU]);
        self.br_if(bt);
        self.unit([I::LocalGet(POS)]);
        self.i(I::LocalSet(C));
        self.class();
        self.i(I::I32Eqz);
        self.br_if(bt);
        self.is([I::LocalGet(POS), I::I32Const(1), I::I32Add, I::LocalSet(POS)]);
    }
}

/// `re_exec(prog, s, from, mode, caps) -> i32`; `wide` is `re_class_wide`, `grow` `re_grow`.
pub fn exec(wide: u32, grow: u32) -> Function {
    let mut b = B { v: Vec::new(), depth: 0, wide, grow };
    let i32s = HeapType::Concrete(T_I32S);
    b.is([I::LocalGet(S), I::ArrayLen, I::LocalSet(LEN)]);
    // The backtrack stack, kept between calls (no user code runs while matching).
    b.is([I::GlobalGet(G_RE_STACK), I::LocalTee(STACK), I::RefIsNull]);
    b.if_();
    b.is([I::I32Const(96), I::ArrayNewDefault(T_I32S), I::LocalTee(STACK), I::GlobalSet(G_RE_STACK)]);
    b.end();
    b.is([I::LocalGet(FROM), I::LocalSet(START), I::I32Const(-1), I::LocalSet(Q)]);
    let search = b.lp();
    // ---- the search (mode 0): skip starts no match can have ----
    b.is([I::LocalGet(MODE), I::I32Eqz]);
    b.if_();
    {
        // `^` without `m`: position 0 only.
        b.prog([I::I32Const(H_FLAGS)]);
        b.is([I::I32Const(1), I::I32And, I::LocalGet(START), I::I32Const(0), I::I32Ne, I::I32And]);
        b.if_();
        b.is([I::I32Const(-1), I::Return]);
        b.end();
        // R: the required literal's header (after the sets): N its length, K its offset in every match.
        b.prog([I::I32Const(H_NSETS)]);
        b.is([I::I32Const(1), I::I32Shl, I::I32Const(H_NSETS + 1), I::I32Add, I::LocalSet(R)]);
        b.prog([I::LocalGet(R)]);
        b.i(I::LocalTee(N));
        b.if_();
        {
            b.prog([I::LocalGet(R), I::I32Const(1), I::I32Add]);
            b.is([I::LocalTee(K), I::I32Const(NONE), I::I32Eq]);
            b.if_();
            b.is([I::I32Const(0), I::LocalSet(K)]);
            b.end();
            // Q: its next occurrence at or after START + K; none: no match from here on.
            b.is([I::LocalGet(START), I::LocalGet(K), I::I32Add, I::LocalGet(Q), I::I32GtS]);
            b.if_();
            {
                b.is([I::LocalGet(START), I::LocalGet(K), I::I32Add, I::LocalSet(T)]);
                // Y: its first unit, scanned for alone; then the rest compared.
                b.prog([I::LocalGet(R), I::I32Const(2), I::I32Add]);
                b.i(I::LocalSet(Y));
                let found = b.block();
                let scan = b.lp();
                b.is([I::LocalGet(T), I::LocalGet(N), I::I32Add, I::LocalGet(LEN), I::I32GtU]);
                b.if_();
                b.is([I::I32Const(-1), I::Return]);
                b.end();
                b.unit([I::LocalGet(T)]);
                b.is([I::LocalGet(Y), I::I32Ne]);
                b.if_();
                b.is([I::LocalGet(T), I::I32Const(1), I::I32Add, I::LocalSet(T)]);
                b.br(scan);
                b.end();
                b.is([I::I32Const(1), I::LocalSet(I_)]);
                let miss = b.block();
                let cmp = b.lp();
                b.is([I::LocalGet(I_), I::LocalGet(N), I::I32Eq]);
                b.br_if(found);
                b.unit([I::LocalGet(T), I::LocalGet(I_), I::I32Add]);
                b.prog([I::LocalGet(R), I::I32Const(2), I::I32Add, I::LocalGet(I_), I::I32Add]);
                b.i(I::I32Ne);
                b.br_if(miss);
                b.is([I::LocalGet(I_), I::I32Const(1), I::I32Add, I::LocalSet(I_)]);
                b.br(cmp);
                b.end();
                b.end();
                b.is([I::LocalGet(T), I::I32Const(1), I::I32Add, I::LocalSet(T)]);
                b.br(scan);
                b.end();
                b.end();
                b.is([I::LocalGet(T), I::LocalSet(Q)]);
            }
            b.end();
            // At a known offset, no match starts before Q - K.
            b.prog([I::LocalGet(R), I::I32Const(1), I::I32Add]);
            b.is([I::I32Const(NONE), I::I32Ne]);
            b.if_();
            b.is([I::LocalGet(Q), I::LocalGet(K), I::I32Sub, I::LocalTee(T), I::LocalGet(START), I::I32GtS]);
            b.if_();
            b.is([I::LocalGet(T), I::LocalSet(START)]);
            b.end();
            b.end();
        }
        b.end();
        // The units at fixed offsets (flag 4): every set must hold. The (up to 4) sets in locals, a missing one
        // repeating set 0; all tested at each position without early branches (mispredicted on random text).
        b.prog([I::I32Const(H_FLAGS)]);
        b.is([I::I32Const(4), I::I32And]);
        b.if_();
        {
            for j in 0..4 {
                b.prog([I::I32Const(H_NSETS)]);
                b.is([I::I32Const(j), I::I32GtU]);
                b.if_();
                b.prog([I::I32Const(H_NSETS + 1 + 2 * j)]);
                b.i(I::LocalSet(O0 + j as u32));
                b.prog([I::I32Const(H_NSETS + 2 + 2 * j)]);
                b.i(I::LocalSet(K0 + j as u32));
                b.i(I::Else);
                b.is([I::LocalGet(O0), I::LocalSet(O0 + j as u32), I::LocalGet(K0), I::LocalSet(K0 + j as u32)]);
                b.end();
            }
            // LIMIT = LEN - need (none left: no match).
            b.i(I::LocalGet(LEN));
            b.prog([I::I32Const(H_NEED)]);
            b.is([I::I32Sub, I::LocalTee(LIMIT), I::I32Const(0), I::I32LtS]);
            b.if_();
            b.is([I::I32Const(-1), I::Return]);
            b.end();
            // Set 0's bitmap words as two i64 masks.
            for (m, base) in [(M0, 1), (M1, 5)] {
                b.is([I::I64Const(0), I::LocalSet(m)]);
                for w in 0..4 {
                    b.i(I::LocalGet(m));
                    b.prog([I::LocalGet(K0), I::I32Const(base + w), I::I32Add]);
                    b.is([I::I64ExtendI32U, I::I64Const(16 * w as i64), I::I64Shl, I::I64Or, I::LocalSet(m)]);
                }
            }
            let found = b.block();
            let scan = b.lp();
            b.is([I::LocalGet(START), I::LocalGet(LIMIT), I::I32GtS]);
            b.if_();
            b.is([I::I32Const(-1), I::Return]);
            b.end();
            // The smallest set alone first (on text it rejects most positions), then the others together.
            let next = b.block();
            for j in 0..4u32 {
                b.unit([I::LocalGet(START), I::LocalGet(O0 + j), I::I32Add]);
                b.i(I::LocalSet(C));
                if j == 0 {
                    // The mask below 128 (an i64 shift takes the amount mod 64), the ranges above.
                    b.is([I::LocalGet(C), I::I32Const(128), I::I32LtU, I::If(BlockType::Result(ValType::I32))]);
                    b.depth += 1;
                    b.is([I::LocalGet(M0), I::LocalGet(M1), I::LocalGet(C), I::I32Const(64), I::I32LtU, I::Select]);
                    b.is([I::LocalGet(C), I::I64ExtendI32U, I::I64ShrU, I::I32WrapI64, I::I32Const(1), I::I32And]);
                    b.i(I::Else);
                    b.is([I::LocalGet(P), I::LocalGet(K0), I::LocalGet(C), I::Call(b.wide)]);
                    b.end();
                    b.i(I::I32Eqz);
                    b.br_if(next);
                } else {
                    b.is([I::LocalGet(K0 + j), I::LocalSet(K)]);
                    b.class();
                    if j > 1 {
                        b.i(I::I32And);
                    }
                }
            }
            b.br_if(found);
            b.end();
            b.is([I::LocalGet(START), I::I32Const(1), I::I32Add, I::LocalSet(START)]);
            b.br(scan);
            b.end();
            b.end();
        }
        b.end();
    }
    b.end();
    // ---- try START ----
    b.is([I::LocalGet(START), I::LocalSet(POS), I::I32Const(0), I::LocalSet(SP)]);
    // The captures are unset.
    b.prog([I::I32Const(H_NCAP)]);
    b.is([I::LocalSet(N), I::I32Const(0), I::LocalSet(I_)]);
    {
        let done = b.block();
        let l = b.lp();
        b.is([I::LocalGet(I_), I::LocalGet(N), I::I32GeU]);
        b.br_if(done);
        b.is([I::LocalGet(CAPS), I::LocalGet(I_), I::I32Const(2), I::I32Add, I::I32Const(-1), I::ArraySet(T_I32S)]);
        b.is([I::LocalGet(I_), I::I32Const(1), I::I32Add, I::LocalSet(I_)]);
        b.br(l);
        b.end();
        b.end();
    }
    b.prog([I::I32Const(H_CODE)]);
    b.i(I::LocalSet(PC));
    let fail = b.block();
    let run = b.lp();
    let bt = b.block();
    // One block per op, the innermost first in the `br_table`: op k's code follows the k-th `end`.
    for _ in 0..NOPS {
        b.block();
    }
    b.prog([I::LocalGet(PC)]);
    b.i(I::BrTable((0..NOPS).collect::<Vec<_>>().into(), NOPS));
    // MATCH
    b.end();
    b.is([I::LocalGet(CAPS), I::I32Const(0), I::LocalGet(START), I::ArraySet(T_I32S)]);
    b.is([I::LocalGet(CAPS), I::I32Const(1), I::LocalGet(POS), I::ArraySet(T_I32S)]);
    b.is([I::LocalGet(POS), I::Return]);
    // CHAR c
    b.end();
    b.is([I::LocalGet(POS), I::LocalGet(LEN), I::I32GeU]);
    b.br_if(bt);
    b.unit([I::LocalGet(POS)]);
    b.arg(1);
    b.i(I::I32Ne);
    b.br_if(bt);
    b.is([I::LocalGet(POS), I::I32Const(1), I::I32Add, I::LocalSet(POS)]);
    b.next(2, run);
    // CLASS cls
    b.end();
    b.arg(1);
    b.i(I::LocalSet(K));
    b.one_of_class(bt);
    b.next(2, run);
    // STR n u…
    b.end();
    b.arg(1);
    b.i(I::LocalSet(N));
    b.is([I::LocalGet(POS), I::LocalGet(N), I::I32Add, I::LocalGet(LEN), I::I32GtU]);
    b.br_if(bt);
    b.is([I::I32Const(0), I::LocalSet(I_)]);
    {
        let done = b.block();
        let l = b.lp();
        b.is([I::LocalGet(I_), I::LocalGet(N), I::I32Eq]);
        b.br_if(done);
        b.unit([I::LocalGet(POS), I::LocalGet(I_), I::I32Add]);
        b.prog([I::LocalGet(PC), I::I32Const(2), I::I32Add, I::LocalGet(I_), I::I32Add]);
        b.i(I::I32Ne);
        b.br_if(bt);
        b.is([I::LocalGet(I_), I::I32Const(1), I::I32Add, I::LocalSet(I_)]);
        b.br(l);
        b.end();
        b.end();
    }
    b.is([I::LocalGet(POS), I::LocalGet(N), I::I32Add, I::LocalSet(POS)]);
    b.is([I::LocalGet(PC), I::LocalGet(N), I::I32Const(2), I::I32Add, I::I32Add, I::LocalSet(PC)]);
    b.br(run);
    // BOL: pos 0, or (m) a line terminator before it
    b.end();
    b.i(I::LocalGet(POS));
    b.if_();
    b.prog([I::I32Const(H_FLAGS)]);
    b.is([I::I32Const(2), I::I32And, I::I32Eqz]);
    b.br_if(bt);
    b.unit([I::LocalGet(POS), I::I32Const(1), I::I32Sub]);
    b.i(I::LocalSet(C));
    b.line_terminator();
    b.i(I::I32Eqz);
    b.br_if(bt);
    b.end();
    b.next(1, run);
    // EOL: pos == len, or (m) a line terminator at pos
    b.end();
    b.is([I::LocalGet(POS), I::LocalGet(LEN), I::I32Ne]);
    b.if_();
    b.prog([I::I32Const(H_FLAGS)]);
    b.is([I::I32Const(2), I::I32And, I::I32Eqz]);
    b.br_if(bt);
    b.unit([I::LocalGet(POS)]);
    b.i(I::LocalSet(C));
    b.line_terminator();
    b.i(I::I32Eqz);
    b.br_if(bt);
    b.end();
    b.next(1, run);
    // WORDB negate: a word unit before pos != one at pos (`\b`), == (`\B`)
    b.end();
    b.is([I::I32Const(0), I::LocalSet(X), I::LocalGet(POS)]);
    b.if_();
    b.unit([I::LocalGet(POS), I::I32Const(1), I::I32Sub]);
    b.i(I::LocalSet(C));
    b.word();
    b.i(I::LocalSet(X));
    b.end();
    b.is([I::I32Const(0), I::LocalSet(Y), I::LocalGet(POS), I::LocalGet(LEN), I::I32LtU]);
    b.if_();
    b.unit([I::LocalGet(POS)]);
    b.i(I::LocalSet(C));
    b.word();
    b.i(I::LocalSet(Y));
    b.end();
    b.is([I::LocalGet(X), I::LocalGet(Y), I::I32Ne]);
    b.arg(1);
    b.i(I::I32Eq);
    b.br_if(bt);
    b.next(2, run);
    // JMP t
    b.end();
    b.arg(1);
    b.i(I::LocalSet(PC));
    b.br(run);
    // SPLIT t: the next op now, `t` on backtrack
    b.end();
    b.push(
        vec![I::LocalGet(P), I::LocalGet(PC), I::I32Const(1), I::I32Add, I::ArrayGetU(T_STRING), I::I32Const(2), I::I32Shl],
        vec![I::LocalGet(POS)],
        vec![I::I32Const(0)],
    );
    b.next(2, run);
    // SAVE slot
    b.end();
    b.set_slot(vec![I::LocalGet(P), I::LocalGet(PC), I::I32Const(1), I::I32Add, I::ArrayGetU(T_STRING)], vec![I::LocalGet(POS)]);
    b.next(2, run);
    // CLEAR first last
    b.end();
    b.arg(1);
    b.i(I::LocalSet(I_));
    {
        let done = b.block();
        let l = b.lp();
        b.i(I::LocalGet(I_));
        b.arg(2);
        b.i(I::I32GtU);
        b.br_if(done);
        b.set_slot(vec![I::LocalGet(I_)], vec![I::I32Const(-1)]);
        b.is([I::LocalGet(I_), I::I32Const(1), I::I32Add, I::LocalSet(I_)]);
        b.br(l);
        b.end();
        b.end();
    }
    b.next(3, run);
    // STAR cls min max atomic: scan ahead (X: where it started, N: the maximum); give back on backtrack (tag 1,
    // down to X + min)
    b.end();
    b.arg(1);
    b.i(I::LocalSet(K));
    b.arg(3);
    b.is([I::LocalTee(N), I::I32Const(NONE), I::I32Eq]);
    b.if_();
    b.is([I::I32Const(i32::MAX), I::LocalSet(N)]);
    b.end();
    b.is([I::LocalGet(POS), I::LocalSet(X)]);
    {
        let done = b.block();
        let l = b.lp();
        b.is([I::LocalGet(POS), I::LocalGet(LEN), I::I32GeU]);
        b.br_if(done);
        b.is([I::LocalGet(POS), I::LocalGet(X), I::I32Sub, I::LocalGet(N), I::I32GeU]);
        b.br_if(done);
        b.unit([I::LocalGet(POS)]);
        b.i(I::LocalSet(C));
        b.class();
        b.i(I::I32Eqz);
        b.br_if(done);
        b.is([I::LocalGet(POS), I::I32Const(1), I::I32Add, I::LocalSet(POS)]);
        b.br(l);
        b.end();
        b.end();
    }
    b.is([I::LocalGet(POS), I::LocalGet(X), I::I32Sub]);
    b.arg(2);
    b.i(I::LocalTee(Y));
    b.i(I::I32LtU);
    b.br_if(bt);
    // Not atomic and of variable length: a give-back entry.
    b.arg(4);
    b.is([I::I32Eqz, I::LocalGet(N), I::LocalGet(Y), I::I32Ne, I::I32And]);
    b.if_();
    b.push(
        vec![I::LocalGet(PC), I::I32Const(2), I::I32Shl, I::I32Const(1), I::I32Or],
        vec![I::LocalGet(POS)],
        vec![I::LocalGet(X), I::LocalGet(Y), I::I32Add],
    );
    b.end();
    b.next(5, run);
    // LAZY cls min max: the minimum, then one more per backtrack (tag 2, X the count)
    b.end();
    b.arg(1);
    b.i(I::LocalSet(K));
    b.arg(2);
    b.i(I::LocalSet(N));
    b.is([I::I32Const(0), I::LocalSet(I_)]);
    {
        let done = b.block();
        let l = b.lp();
        b.is([I::LocalGet(I_), I::LocalGet(N), I::I32Eq]);
        b.br_if(done);
        b.one_of_class(bt);
        b.is([I::LocalGet(I_), I::I32Const(1), I::I32Add, I::LocalSet(I_)]);
        b.br(l);
        b.end();
        b.end();
    }
    b.arg(3);
    b.is([I::LocalGet(N), I::I32Ne]);
    b.if_();
    b.push(
        vec![I::LocalGet(PC), I::I32Const(2), I::I32Shl, I::I32Const(2), I::I32Or],
        vec![I::LocalGet(POS)],
        vec![I::LocalGet(N)],
    );
    b.end();
    b.next(4, run);
    // REP_INIT r: count = 0
    b.end();
    b.set_slot(vec![I::LocalGet(P), I::LocalGet(PC), I::I32Const(1), I::I32Add, I::ArrayGetU(T_STRING)], vec![I::I32Const(0)]);
    b.next(2, run);
    // REP_LOOP r min max greedy exit: the iteration is the next op (PC + 6)
    b.end();
    b.arg(1);
    b.i(I::LocalSet(R));
    b.slot(vec![I::LocalGet(R)]);
    b.i(I::LocalSet(X));
    // Below the minimum: iterate.
    b.i(I::LocalGet(X));
    b.arg(2);
    b.i(I::I32LtU);
    b.if_();
    b.next(6, run);
    b.end();
    // At the maximum: exit.
    b.arg(3);
    b.i(I::LocalTee(Y));
    b.is([I::I32Const(NONE), I::I32Ne, I::LocalGet(X), I::LocalGet(Y), I::I32GeU, I::I32And]);
    b.if_();
    b.arg(5);
    b.i(I::LocalSet(PC));
    b.br(run);
    b.end();
    b.arg(4);
    b.if_();
    // Greedy: iterate, the exit on backtrack.
    b.push(
        vec![I::LocalGet(P), I::LocalGet(PC), I::I32Const(5), I::I32Add, I::ArrayGetU(T_STRING), I::I32Const(2), I::I32Shl],
        vec![I::LocalGet(POS)],
        vec![I::I32Const(0)],
    );
    b.next(6, run);
    b.end();
    // Lazy: exit, the iteration on backtrack.
    b.push(
        vec![I::LocalGet(PC), I::I32Const(6), I::I32Add, I::I32Const(2), I::I32Shl],
        vec![I::LocalGet(POS)],
        vec![I::I32Const(0)],
    );
    b.arg(5);
    b.i(I::LocalSet(PC));
    b.br(run);
    // REP_ITER r: its start = pos, count + 1
    b.end();
    b.arg(1);
    b.i(I::LocalSet(R));
    b.set_slot(vec![I::LocalGet(R), I::I32Const(1), I::I32Add], vec![I::LocalGet(POS)]);
    b.set_slot(
        vec![I::LocalGet(R)],
        vec![I::LocalGet(CAPS), I::LocalGet(R), I::I32Const(2), I::I32Add, I::ArrayGet(T_I32S), I::I32Const(1), I::I32Add],
    );
    b.next(2, run);
    // REP_END r min loop: an optional iteration that matched the empty string fails
    b.end();
    b.arg(1);
    b.i(I::LocalSet(R));
    b.slot(vec![I::LocalGet(R)]);
    b.arg(2);
    b.i(I::I32GtU);
    b.i(I::LocalGet(POS));
    b.slot(vec![I::LocalGet(R), I::I32Const(1), I::I32Add]);
    b.is([I::I32Eq, I::I32And]);
    b.br_if(bt);
    b.arg(3);
    b.i(I::LocalSet(PC));
    b.br(run);
    // ---- backtrack: pop entries, undoing slots, up to a choice ----
    b.end();
    {
        let pop = b.lp();
        b.is([I::LocalGet(SP), I::I32Eqz]);
        b.br_if(fail);
        b.is([I::LocalGet(SP), I::I32Const(3), I::I32Sub, I::LocalTee(SP)]);
        b.is([I::LocalGet(STACK), I::LocalGet(SP), I::ArrayGet(T_I32S), I::LocalTee(T), I::I32Const(0), I::I32LtS]);
        b.if_();
        // An undo: slot -1 - T, at caps[1 - T].
        b.is([I::LocalGet(CAPS), I::I32Const(1), I::LocalGet(T), I::I32Sub]);
        b.is([I::LocalGet(STACK), I::LocalGet(SP), I::I32Const(1), I::I32Add, I::ArrayGet(T_I32S), I::ArraySet(T_I32S)]);
        b.br(pop);
        b.end();
        b.is([I::LocalGet(STACK), I::LocalGet(SP), I::I32Const(1), I::I32Add, I::ArrayGet(T_I32S), I::LocalSet(POS)]);
        b.is([I::LocalGet(STACK), I::LocalGet(SP), I::I32Const(2), I::I32Add, I::ArrayGet(T_I32S), I::LocalSet(X)]);
        b.is([I::LocalGet(T), I::I32Const(2), I::I32ShrU, I::LocalSet(PC)]);
        b.is([I::LocalGet(T), I::I32Const(3), I::I32And, I::LocalTee(Y), I::I32Eqz]);
        b.br_if(run);
        b.is([I::LocalGet(Y), I::I32Const(1), I::I32Eq]);
        b.if_();
        // A greedy loop gives back one unit, down to X.
        b.is([I::LocalGet(POS), I::LocalGet(X), I::I32Eq]);
        b.br_if(pop);
        b.is([I::LocalGet(POS), I::I32Const(1), I::I32Sub, I::LocalSet(POS)]);
        b.push(vec![I::LocalGet(T)], vec![I::LocalGet(POS)], vec![I::LocalGet(X)]);
        b.next(5, run);
        b.end();
        // A lazy loop takes one more unit (X: how many so far).
        b.arg(3);
        b.i(I::LocalTee(N));
        b.is([I::I32Const(NONE), I::I32Ne, I::LocalGet(X), I::LocalGet(N), I::I32GeU, I::I32And]);
        b.br_if(pop);
        b.is([I::LocalGet(POS), I::LocalGet(LEN), I::I32GeU]);
        b.br_if(pop);
        b.unit([I::LocalGet(POS)]);
        b.i(I::LocalSet(C));
        b.arg(1);
        b.i(I::LocalSet(K));
        b.class();
        b.i(I::I32Eqz);
        b.br_if(pop);
        b.is([I::LocalGet(POS), I::I32Const(1), I::I32Add, I::LocalSet(POS)]);
        b.push(vec![I::LocalGet(T)], vec![I::LocalGet(POS)], vec![I::LocalGet(X), I::I32Const(1), I::I32Add]);
        b.next(4, run);
        b.end();
    }
    b.end();
    b.end();
    // ---- no match at START ----
    b.i(I::LocalGet(MODE));
    b.if_();
    b.is([I::I32Const(-1), I::Return]);
    b.end();
    b.is([I::LocalGet(START), I::I32Const(1), I::I32Add, I::LocalTee(START), I::LocalGet(LEN), I::I32GtU]);
    b.if_();
    b.is([I::I32Const(-1), I::Return]);
    b.end();
    b.br(search);
    b.end();
    b.i(I::Unreachable);
    assert_eq!(b.depth, 0);
    let mut f = Function::new([
        (22, ValType::I32),
        (1, ValType::Ref(RefType { nullable: true, heap_type: i32s })),
        (1, ValType::I32),
        (2, ValType::I64),
    ]);
    for x in &b.v {
        f.instruction(x);
    }
    f.instruction(&I::End);
    f
}
