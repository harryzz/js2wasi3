#!/usr/bin/env python3
"""T140 probe: fannkuchredux's `fannkuch(n)` in three models of our generated code, as WAT.
Usage: python3 probes/fannkuch-gen.py   (writes probes/fannkuch-m{0,1,2}.wat; build and run: probes/fannkuch-run.sh)
 M0: today's shape (T138): `f64` counters, the key converted f64 -> i32 -> f64 and compared per access, the receiver
     tested and cast per access, the generic `i31` store, a write's nine tests, `box_number` called per written number.
 M1: `i32` counters (checked arithmetic), the same generic store and tests, the value written as an inline `i31`.
 M2: M1 plus the receiver typed once (no per-access test/cast) and an `i32` element store: a read is a null test and
     a bounds test, a write is the flags test, the null test and the bounds test.
 M3: M1 plus only the receiver typed once (the generic `i31` store and the write's tests kept).
 M4: M0 plus the receiver typed once: what the compiler is after T141.
 M5: M4 plus keys known to be integer-valued `f64`s: `trunc_sat` alone, no `convert`/`ne` check.
 M6: M5 plus the written number boxed inline (an `i31` when it fits) instead of calling `box_number`."""
import sys
F64_MODELS = (0, 4, 5, 6)
HOLE = 9219994337134247937  # the NaN our f64 slots use for "not a number"

TYPES = """
  (type $vals (array (mut eqref)))
  (type $ints (array (mut i32)))
  (type $num (struct (field f64)))
  (type $arr (struct (field $flags (mut i32)) (field $vals (mut (ref null $vals))) (field $len (mut i32))
    (field $f9 (mut eqref)) (field $nums (mut eqref)) (field $f11 (mut i32)) (field $ints (mut (ref null $ints)))))
"""

class Gen:
    def __init__(s, model):
        s.m = model; s.n = 0; s.out = []
        s.f64 = model in F64_MODELS
        s.typed = model in (2, 3, 4, 5, 6)
        s.knownint = model in (5, 6)
        s.inbox = model == 6
        s.T = 'f64' if s.f64 else 'i32'
    def id(s):
        s.n += 1; return s.n
    def w(s, line): s.out.append(line)
    # numbers
    def c(s, v): return f"({s.T}.const {v})"
    def L(s, name): return f"(local.get ${name})"
    def op(s, o, a, b):
        if s.f64: return f"(f64.{ {'add':'add','sub':'sub','mul':'mul'}[o] } {a} {b})"
        i = s.id()
        return (f"(block (result i32) (local.set $t64 (i64.{o} (i64.extend_i32_s {a}) (i64.extend_i32_s {b})))"
                f" (if (i64.ne (local.get $t64) (i64.extend_i32_s (i32.wrap_i64 (local.get $t64)))) (then (unreachable)))"
                f" (i32.wrap_i64 (local.get $t64)))")
    def add(s, a, b): return s.op('add', a, b)
    def sub(s, a, b): return s.op('sub', a, b)
    def mul(s, a, b): return s.op('mul', a, b)
    def cmp(s, o, a, b):
        if s.f64: return f"(f64.{o} {a} {b})"
        return f"(i32.{ {'lt':'lt_s','gt':'gt_s','eq':'eq','ne':'ne'}[o] } {a} {b})"
    def bit1(s, x):
        if s.f64: return f"(i32.and (i32.trunc_sat_f64_s {x}) (i32.const 1))"
        return f"(i32.and {x} (i32.const 1))"
    def set(s, name, e): s.w(f"(local.set ${name} {e})")
    def while_(s, cond, body):
        i = s.id()
        s.w(f"(block $b{i} (loop $l{i} (br_if $b{i} (i32.eqz {cond}))")
        body(); s.w(f"(br $l{i})))")
    def if_(s, cond, then, els=None):
        s.w(f"(if {cond} (then"); then()
        if els: s.w(") (else"); els()
        s.w("))")
    # arrays
    def get(s, arr, idx):
        i = s.id(); A = f"(local.get ${arr})"
        if s.m == 2:
            return (f"(block (result i32) (local.set $gk {idx}) (local.set $gi (struct.get $arr $ints {A}))"
                    f" (if (ref.is_null (local.get $gi)) (then (unreachable)))"
                    f" (if (i32.ge_u (local.get $gk) (array.len (local.get $gi))) (then (unreachable)))"
                    f" (array.get $ints (local.get $gi) (local.get $gk)))")
        if s.f64 and s.knownint:
            key = f"(local.set $gk (i32.trunc_sat_f64_s {idx}))"
        elif s.f64:
            key = (f"(local.set $gf {idx}) (local.set $gk (i32.trunc_sat_f64_s (local.get $gf)))"
                   f" (br_if $miss{i} (f64.ne (f64.convert_i32_s (local.get $gk)) (local.get $gf)))")
        else:
            key = f"(local.set $gk {idx})"
        val = ("(f64.convert_i32_s (i31.get_s (ref.cast (ref i31) (local.get $ge))))" if s.f64
               else "(i31.get_s (ref.cast (ref i31) (local.get $ge)))")
        recv = (f" (br_if $miss{i} (i32.eqz (ref.test (ref $arr) {A})))"
                f" (local.set $ga (ref.cast (ref $arr) {A}))") if not s.typed else f" (local.set $ga {A})"
        body = (f"(block $ok{i} (result {s.T}) (block $miss{i}" + recv + f" {key}"
                f" (local.set $gv (struct.get $arr $vals (local.get $ga)))"
                f" (if (i32.lt_u (local.get $gk) (array.len (local.get $gv))) (then"
                f"  (local.set $ge (array.get $vals (local.get $gv) (local.get $gk)))"
                f"  (if (ref.test (ref i31) (local.get $ge)) (then (br $ok{i} {val})))))"
                f" (br $miss{i}))")
        if s.f64:
            return (f"(block (result f64) (local.set $gx (block (result f64) {body[:-0] if False else body}"
                    f" (f64.reinterpret_i64 (i64.const {HOLE})))))"
                    f" (if (i64.eq (i64.reinterpret_f64 (local.get $gx)) (i64.const {HOLE})) (then (unreachable)))"
                    f" (local.get $gx))")
        return f"(block (result i32) (block $ok2{i} (result i32) {body[len('(block $ok'+str(i)+' (result i32) '):]} (unreachable)))" if False else \
               f"{body} (unreachable))"
    def put(s, arr, idx, val):
        i = s.id(); A = f"(local.get ${arr})"
        if s.m == 2:
            s.w(f"(local.set $sk {idx}) (local.set $si {val})")
            s.w(f"(local.set $gi (struct.get $arr $ints {A}))"
                f" (if (ref.is_null (local.get $gi)) (then (unreachable)))"
                f" (if (i32.and (struct.get $arr $flags {A}) (i32.const 1536)) (then (unreachable)))"
                f" (if (i32.ge_u (local.get $sk) (array.len (local.get $gi))) (then (unreachable)))"
                f" (array.set $ints (local.get $gi) (local.get $sk) (local.get $si))")
            return
        if s.f64:
            if s.inbox:
                s.w(f"(local.set $bf {val}) (local.set $bi (i32.trunc_sat_f64_s (local.get $bf)))"
                    f" (local.set $sv (if (result eqref) (i32.and (f64.eq (f64.convert_i32_s (local.get $bi)) (local.get $bf))"
                    f" (i32.lt_u (i32.add (local.get $bi) (i32.const 0x40000000)) (i32.const 0x80000000)))"
                    f" (then (ref.i31 (local.get $bi))) (else (struct.new $num (local.get $bf)))))")
            else:
                s.w(f"(local.set $sv (call $box {val}))")
            s.w(f"(local.set $sf {idx})")
            if s.knownint:
                key = "(local.set $sk (i32.trunc_sat_f64_s (local.get $sf)))"
            else:
                key = (f"(local.set $sk (i32.trunc_sat_f64_s (local.get $sf)))"
                       f" (br_if $miss{i} (f64.ne (f64.convert_i32_s (local.get $sk)) (local.get $sf)))")
        else:
            s.w(f"(local.set $si {val}) (local.set $sk {idx})")
            s.w(f"(if (i32.ge_u (i32.add (local.get $si) (i32.const 0x40000000)) (i32.const 0x80000000)) (then (unreachable)))"
                f" (local.set $sv (ref.i31 (local.get $si)))")
            key = ""
        S = "(local.get $sa)"
        recv = (f" (br_if $miss{i} (i32.eqz (ref.test (ref $arr) {A})))"
                f" (local.set $sa (ref.cast (ref $arr) {A}))") if not s.typed else f" (local.set $sa {A})"
        s.w(f"(block $done{i} (block $miss{i} {key}" + recv +
            f" (br_if $miss{i} (i32.ge_u (local.get $sk) (struct.get $arr $len {S})))"
            f" (br_if $miss{i} (i32.eqz (ref.is_null (struct.get $arr $nums {S}))))"
            f" (br_if $miss{i} (i32.and (struct.get $arr $f11 {S}) (i32.eqz (ref.test (ref i31) (local.get $sv)))))"
            f" (br_if $miss{i} (i32.eqz (ref.is_null (struct.get $arr $f9 {S}))))"
            f" (br_if $miss{i} (i32.and (struct.get $arr $flags {S}) (i32.const 1536)))"
            f" (br_if $miss{i} (ref.is_null (array.get $vals (struct.get $arr $vals {S}) (local.get $sk))))"
            f" (array.set $vals (struct.get $arr $vals {S}) (local.get $sk) (local.get $sv))"
            f" (br $done{i})) (unreachable))")

def program(m):
    g = Gen(m); T = g.T; L = g.L; c = g.c
    arrt = "(ref null $arr)" if m in (2, 3, 4, 5, 6) else "eqref"
    g.w(f"(func $fannkuch (param $n {T}) (result {T} {T})")
    g.w(f"(local $perm1 {arrt}) (local $perm {arrt}) (local $count {arrt})")
    for v in "ijkrf flips nperm checksum p0 t".replace("ijkrf", "i j k r f").split():
        g.w(f"(local ${v} {T})")
    g.w("(local $more i32) (local $ga (ref null $arr)) (local $sa (ref null $arr)) (local $gv (ref null $vals))"
        " (local $gi (ref null $ints)) (local $ge eqref) (local $sv eqref) (local $gk i32) (local $sk i32) (local $si i32)"
        " (local $gf f64) (local $sf f64) (local $gx f64) (local $t64 i64) (local $bf f64) (local $bi i32)")
    nn = "(i32.trunc_sat_f64_s (local.get $n))" if g.f64 else "(local.get $n)"
    for a in ("perm1", "perm", "count"):
        g.w(f"(local.set ${a} (call $mk {nn}))")
    g.set("i", c(0))
    g.while_(g.cmp('lt', L('i'), L('n')), lambda: (g.put('perm1', L('i'), L('i')), g.set('i', g.add(L('i'), c(1)))))
    g.set("r", L('n'))
    def outer():
        g.set("i", c(0))
        g.while_(g.cmp('ne', L('r'), c(1)), lambda: (g.put('count', g.sub(L('r'), c(1)), L('r')), g.set('r', g.sub(L('r'), c(1)))))
        g.while_(g.cmp('lt', L('i'), L('n')), lambda: (g.put('perm', L('i'), g.get('perm1', L('i'))), g.set('i', g.add(L('i'), c(1)))))
        g.set("f", c(0)); g.set("k", g.get('perm', c(0)))
        def flip():
            g.set("i", c(0))
            def swap():
                g.set("t", g.get('perm', L('i')))
                g.put('perm', L('i'), g.get('perm', g.sub(L('k'), L('i'))))
                g.put('perm', g.sub(L('k'), L('i')), L('t'))
                g.set('i', g.add(L('i'), c(1)))
            g.while_(g.cmp('lt', g.mul(c(2), L('i')), L('k')), swap)
            g.set("k", g.get('perm', c(0))); g.set("f", g.add(L('f'), c(1)))
        g.while_(g.cmp('ne', L('k'), c(0)), flip)
        g.if_(g.cmp('gt', L('f'), L('flips')), lambda: g.set('flips', L('f')))
        g.if_(f"(i32.eqz {g.bit1(L('nperm'))})", lambda: g.set('checksum', g.add(L('checksum'), L('f'))),
              lambda: g.set('checksum', g.sub(L('checksum'), L('f'))))
        g.w("(local.set $more (i32.const 1))")
        def more():
            g.if_(g.cmp('eq', L('r'), L('n')), lambda: g.w(f"(return (local.get $checksum) (local.get $flips))"))
            g.set("p0", g.get('perm1', c(0))); g.set("i", c(0))
            def shift():
                g.set("j", g.add(L('i'), c(1)))
                g.put('perm1', L('i'), g.get('perm1', L('j')))
                g.set('i', L('j'))
            g.while_(g.cmp('lt', L('i'), L('r')), shift)
            g.put('perm1', L('r'), L('p0'))
            g.put('count', L('r'), g.sub(g.get('count', L('r')), c(1)))
            g.if_(g.cmp('gt', g.get('count', L('r')), c(0)), lambda: g.w("(local.set $more (i32.const 0))"),
                  lambda: g.set('r', g.add(L('r'), c(1))))
        g.while_("(local.get $more)", more)
        g.set("nperm", g.add(L('nperm'), c(1)))
    g.while_(g.cmp('gt', L('r'), c(0)), outer)
    g.w(f"(return (local.get $checksum) (local.get $flips)))" if False else f"({T}.const 0) ({T}.const 0))")
    return "\n".join(g.out)

def module(m):
    fill = ("(array.new $ints (i32.const 0) (local.get $n))" if m == 2 else "(array.new $vals (ref.i31 (i32.const 0)) (local.get $n))")
    mk = (f"(func $mk (param $n i32) (result (ref $arr)) (struct.new $arr (i32.const 0) "
          + ("(ref.null $vals) (local.get $n) (ref.null eq) (ref.null eq) (i32.const 0) " + fill + "))"
             if m == 2 else fill + " (local.get $n) (ref.null eq) (ref.null eq) (i32.const 0) (ref.null $ints)))"))
    box = """(func $box (param f64) (result eqref) (local $i i32)
    (local.set $i (i32.trunc_sat_f64_s (local.get 0)))
    (if (result eqref) (i32.and (f64.eq (f64.convert_i32_s (local.get $i)) (local.get 0))
          (i32.and (i32.ge_s (local.get $i) (i32.const -1073741824)) (i32.lt_s (local.get $i) (i32.const 1073741824))))
      (then (ref.i31 (local.get $i))) (else (struct.new $num (local.get 0)))))""" if m in F64_MODELS else ""
    # struct.new args order: flags vals len f9 nums f11 ints
    if m != 2:
        mk = (f"(func $mk (param $n i32) (result (ref $arr)) (struct.new $arr (i32.const 0) {fill} (local.get $n)"
              " (ref.null eq) (ref.null eq) (i32.const 0) (ref.null $ints)))")
    else:
        mk = (f"(func $mk (param $n i32) (result (ref $arr)) (struct.new $arr (i32.const 0) (ref.null $vals) (local.get $n)"
              f" (ref.null eq) (ref.null eq) (i32.const 0) {fill}))")
    run = ("(func (export \"run\") (param $n i32) (result i32 i32) (local $a i32) (local $b i32)"
           + (" (call $fannkuch (f64.convert_i32_s (local.get $n))) (local.set $b (i32.trunc_sat_f64_s)) (local.set $a (i32.trunc_sat_f64_s))"
              if m in F64_MODELS else " (call $fannkuch (local.get $n)) (local.set $b) (local.set $a)")
           + " (local.get $a) (local.get $b))")
    return f"(module\n(rec{TYPES})\n{box}\n{mk}\n{program(m)}\n{run}\n)\n".replace("(rec\n", "(rec\n")

if __name__ == "__main__":
    for m in range(7):
        open(f"probes/fannkuch-m{m}.wat", "w").write(module(m))
        print(f"probes/fannkuch-m{m}.wat")
