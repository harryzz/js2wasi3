# T06: Backend v0: IR to core Wasm

Status: **done** (2026-09-25) | Depends on: T04 (T05 for real inputs)

## Goal
Emit a valid core module from IR with `wasm-encoder` 0.259: functions, f64/i32 locals, structured control, direct calls, exports.

## Read first
`crates/backend/src/lib.rs`, `probes/p.wat` (target feature shapes), wasm-encoder docs.

## In scope
- Map the IR's labelled break/continue onto Wasm `block`/`loop`/`br` depths.
- Validate the output with `wasmparser` inside the unit tests (all features on).
- `js2wasi3 build --core` writes the core module only.

## Out of scope
The component (T07), GC types (T08), exceptions (T12).

## Acceptance
- `fib.js` → core module → `wasmtime run --invoke 'fib' out.wasm 20` prints `6765`. For core modules, pass the args after the file.
- `wasm-tools validate` passes.

## Progress
- 2026-09-25: started. Plan: (1) F1 IR validator, (2) emitter for every T05 node kind with wasm-encoder 0.259, validated by wasmparser in tests, (3) JS `ToInt32` and JS `%` helpers, (4) `build --core`, then run `fib.js` in wasmtime.
- 2026-09-25: F1 done: `crates/ir/src/validate.rs` (`ir::validate(&Module) -> Result<(), Vec<String>>`) checks id ranges, label scoping and types.
  2 unit tests pass, and the lower snapshot test now validates the IR of add/fib/loops (passes). Next: emitter.
- 2026-09-25: decisions for the emitter: `Loop` → `block $brk (loop $cont ...)`; functions with a result end in `unreachable` (the lowering guarantees every path returns);
  JS `%` → helper `$js_rem` (exact fmod, port of musl's bit algorithm, since the `x - trunc(x/y)*y` shortcut is inexact); `Convert F64→I32` → helper `$to_int32`
  (fast `i64.trunc_sat` path when |x| < 2^63, otherwise `fmod(x, 2^32)` first, so NaN/±inf → 0 and big values wrap like JS). Helpers are added only when used.
- 2026-09-25: emitter written (`crates/backend/src/lib.rs`: validate → check_supported → types/functions/helpers/globals/exports/name section) and compiling.
  CLI `build --core`. **Acceptance: `fib.js` → `wasm-tools validate` ok → `wasmtime run --invoke fib fib.wasm 20` = `6765`.** Next: differential check of loops.js vs node, fmod/ToInt32 tests.
- 2026-09-25: loops.js vs node: 8/8 calls identical (sumOdd, firstPair incl. labelled break, mix incl. do-while/bitwise/&&/||, count).
- 2026-09-25: `crates/backend/tests/emit.rs`: wasmparser validation of all fixtures, fib run, **400 `%` cases** and **30 `| 0` (ToInt32) cases** compared bit-exactly with Rust
  (includes -0, NaN, ±inf, subnormals, 2^53+1, 2^63, 2^84, max double) → all pass. The harness catches mismatches (checked by hand with a deliberately wrong case).
  Found on the way: node `1e17 % 3 = 1`, while the naive `x - trunc(x/y)*y` gives 0, so the exact fmod helper matters. Workspace fmt/clippy clean, all tests pass. Task closed.

## Checklist
- [x] F1 IR validator (runs first in `emit`)
- [x] Emit functions, f64/i32 locals, globals, structured control, direct calls, exports (+ name section)
- [x] Labelled break/continue → `block`/`loop`/`br` depths
- [x] Output validated with `wasmparser` in unit tests
- [x] `js2wasi3 build --core`
- [x] `fib.js` → `wasmtime run --invoke fib out.wasm 20` prints `6765`; `wasm-tools validate` passes

## Notes / Result
**Changed**
- `crates/ir/src/validate.rs`: new `ir::validate` (F1) + 2 tests. The lower snapshot test now validates its IR too.
- `crates/backend/src/lib.rs`: the emitter (wasm-encoder 0.259) + helpers `$js_rem` (exact fmod, a musl port) and `$to_int32` (JS ToInt32), emitted only when used.
  Unsupported IR (`Try`/`Throw`/`Suspend`/`jsval`/async) is rejected as ``unsupported IR `Try` in function g at 3:1 (planned: T12)``.
- `crates/backend/tests/emit.rs`: 5 tests (valid modules, fib run, fmod 400 cases, ToInt32 30 cases, unsupported-IR message).
- CLI: `js2wasi3 build --core <file> -o out.wasm`. Without `--core`, it stops at the component stub (T07).

**Sizes:** fib.wasm 107 B, loops.wasm 1337 B (the latter includes both helpers and the name section).

**Commands run**
- `js2wasi3 build --core tests/fixtures/fib.js -o fib.wasm && wasm-tools validate fib.wasm && wasmtime run --invoke fib fib.wasm 20` → `6765`
- the differential script loops.js vs node (8 calls, all equal)
- `cargo test` (workspace): all pass. `cargo clippy --all-targets`: clean.

**Notes for later tasks**
- The backend tests call the `wasmtime` CLI (pinned in TOOLS.md). It must be on PATH for `cargo test`.
- `wasmtime --invoke` on a core module prints an "experimental" warning to stderr. Harmless.

**Next smallest step:** T07, packaging as a component with a WIT world generated from the exports (`add: func(a: f64, b: f64) -> f64`).
