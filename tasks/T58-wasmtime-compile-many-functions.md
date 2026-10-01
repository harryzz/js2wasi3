# T58: wasmtime compile time with many functions (F249)

Status: done (2026-10-01) | Depends on: T57

## Goal
`wasmtime compile` of a program with many top-level functions spends time outside any function's compile, growing
superlinearly (T57: 200 functions each returning a 100-element array literal → 3.7 s, 400 → 22 s; a 20 s gap after
the last function in the cranelift log). Find the wasmtime phase and what in js2wasi3's module drives it; make the
compile time linear in the program's size.

## Read first
- `tasks/T57-lazy-strings.md` (Notes / Result: the reproducer `gen3.mjs`, the measuring method).
- `crates/backend/src/lib.rs` (module assembly: globals, shared closures, element segment, start function).

## Tests
The reproducer at a few sizes (times recorded here), `cargo test`, `scripts/diff.sh` on touched fixture dirs (all
of `tests/fixtures` if every program's bytes change), `wasm-tools validate`, one `wasmtime` smoke run.

## Acceptance
- [x] The phase and its cause identified (recorded below).
- [x] Compile time linear in the number of functions (or the cause is wasmtime's and recorded with a workaround or
      an upstream report draft).
- [x] All fixtures still equal node; `cargo test` passes; no run-time slowdown.

## Notes / Result
**Cause:** not functions but `for-of`s (and `try/finally`): their catch local (`exception`) is set only on the catch
path and read after the `try` by the rethrow. That read can see the function entry's implicit default, so wasmtime's
SSA construction (cranelift-frontend) follows it back through every earlier statement and adds the variable as a
block parameter along the way: with N loops in a function, translating it to CLIF was O(N³) (400 `for-of`s: 26 s).
The cranelift log's "compiled in" excludes translation, which is why it looked like a gap after the last function.

**Fix** (`crates/backend/src/lib.rs`, `Stmt::Try`): the catch local is set to `null` before the `try_table`.
Every producer (`lower`: for-of/destructuring/`yield*` closing, `finally`, generators) uses a fresh local read only
after the catch set it, so this changes nothing observable.

**Measurements:** 200/400/800 `for (const x of ["a"])` in one function: `main` translated in 59/140/328 ms (was
3.3 s / 26 s / –); T57's reproducer (400 functions, 40k array elements) `wasmtime compile` 22.3 s → 0.83 s. Run
time: a 1M-iteration loop with a `for-of` and a `try/finally` 325–383 ms before and after. Tests: `cargo test`;
`diff.sh` all fixtures 194 pass + `async/timers-more.js` flaky under load (F250, passes alone); test262
`language/statements/for-of`, `try`, `for-await-of`, `language/expressions/class/dstr`: 4,016 pass, 99.7%, no
changes since the baseline.

**Not done:** other generated locals could have the same shape (read where the entry value is visible); none found
by these measurements. If a large program translates slowly again, look for that first: the WAT probe
(`probe4.mjs`, below) reproduces the pattern.
```js
// N for-of-shaped sections, each with its own eqref locals; the exception is set only in the catch
const [N] = process.argv.slice(2).map(Number);
let o = '(module (tag $e (param eqref)) (func $f (param eqref) (result i32) i32.const 0) (func $g (param eqref) (result eqref) local.get 0) (func $c (param eqref i32))\n(func (export "main") (param $p eqref)\n';
for (let i = 0; i < N; i++) o += `(local $it${i} eqref) (local $d${i} i32) (local $x${i} eqref) (local $ex${i} eqref) (local $cm${i} i32)`;
for (let k = 0; k < N; k++) {
  const inner = `(block (loop (local.set $d${k} (i32.const 1)) (br_if 1 (i32.eqz (call $f (local.get $it${k})))) (local.set $d${k} (i32.const 0)) (local.set $x${k} (call $g (local.get $it${k}))) (drop (call $g (local.get $x${k}))) (br 0)))`;
  o += `(local.set $it${k} (call $g (local.get $p))) (local.set $d${k} (i32.const 1)) (local.set $cm${k} (i32.const 0))
  (block (block (block (result eqref) (try_table (catch $e 0) ${inner}) (br 2)) (local.set $ex${k}) (local.set $cm${k} (i32.const 1))))
  (if (i32.eqz (local.get $d${k})) (then (call $c (local.get $it${k}) (local.get $cm${k}))))
  (if (local.get $cm${k}) (then (throw $e (local.get $ex${k}))))\n`;
}
process.stdout.write(o + '))\n');
```

**Next smallest step:** `stream`/`fs` (F238): write their task file.

## Checklist
- [x] diagnosis
- [x] fix
- [x] fixtures, measurements
- [x] docs, follow-ups

## Progress
- 2026-10-01: task written after T57 (`main` at `e66373c`).
- 2026-10-01: diagnosis. The 20 s is `main`'s Wasm→CLIF translation (the cranelift log's "compiled in" excludes it; `WASMTIME_LOG=debug` shows `translated to CLIF in 20.9s`). JS variants (400 each): plain calls 7 ms, function declarations 0.1 ms, `try/catch` 21 ms, `for (const x of ["a"])` 26 s (200: 3.3 s): cubic. Bisecting the core module's `main`: replacing the rethrow `throw 0` after each loop with `drop` → 3.0 s → 0.17 s. WAT probe (`probe4.mjs`: for-of-shaped sections, each with its own `eqref` locals; the catch's exception local is set only in the catch and read by the rethrow after the loop): 100/200/400 sections → 0.28/2.0/16.5 s; the same with one set of locals shared by all sections → 19/42 ms; with `(local.set $ex (ref.null eq))` before each `try_table` → 24/54/204 ms (200/400/800). Cause: the read of a local defined only on the catch path reaches the function entry's implicit default through every earlier section, so wasmtime's SSA construction (cranelift-frontend `use_var`) adds that variable as a block parameter along the whole path: N variables × N sections of blocks, then the GC-ref liveness on top.
- 2026-10-01: fix: the backend sets a `Try`'s catch local to `null` before its `try_table` (every producer uses a fresh local that's only read once the catch set it). 200/400/800 `for-of`s: main translated in 59/140/328 ms (was 3.3 s / 26 s / –), `wasmtime compile` 0.2/0.46/1.0 s; the T57 reproducer (`gen3.mjs` 40000: 400 functions) 22.3 s → 0.83 s.
- 2026-10-01: `cargo test` passes; `diff.sh` all fixtures: 194 pass, 1 fail (`async/timers-more.js`: a 10 ms timer fired after the immediates under the parallel load; alone it passes 5 of 5: likely T57's unreproduced failure too → follow-up).
- 2026-10-01: test262 for-of/try/for-await-of/class dstr: 4,016 pass (99.7%), no changes since the baseline; run time unchanged (hot2: 325–383 ms both). Done.
