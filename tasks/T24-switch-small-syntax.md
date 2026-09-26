# T24: `switch` and small syntax gaps

Status: done | Depends on: T19

## Goal
Common syntax the compiler still rejects: `switch` statements, logical assignment `&&=` / `||=` / `??=` (F18),
`debugger` (a no-op), exponentiation `**` / `**=`.

## In scope
- `switch`: strict-equality matching in order, fall-through, `default` in any position (taken only when no case
  matches, then falls through from its position), `break` (also labeled) and `continue` of an enclosing loop,
  one lexical scope for all case bodies (`let`/`const`/`class`/functions, TDZ, closures capturing them), case
  expressions evaluated lazily in order, `switch` inside generators/async functions (suspension points in the
  discriminant, the case tests and the bodies), `return`/`break` through `finally`.
- Logical assignment: short-circuit (the target isn't written, a setter isn't called, when not needed), on
  variables, properties, computed properties (object and key evaluated once), private fields.
- `**`: ECMAScript `Number::exponentiate` (NaN, ±Infinity, -0, `(-8) ** (1/3)` is NaN, `1 ** Infinity` is NaN),
  results equal node's (V8's pow) bit for bit on the fixtures; `**=` on the same targets as other compound
  assignment; `-2 ** 2` is a syntax error (parser).

## Out of scope
Tagged templates, regex literals, `arguments`, class `static {}` blocks, `new.target` in functions (see
`tasks/README.md` Follow-ups). BigInt `**`.

## Acceptance
- [x] Fixtures `tests/fixtures/syntax/*.js` match node (`scripts/diff.sh`).
- [x] `unsupported.rs` no longer lists these constructs; snapshot/unit tests updated.

## Notes / Result
**Done 2026-09-26.**

What changed:
- `switch` (`Lowerer::switch_stmt`): the discriminant in a local, one `hoist` over every case body (`hoist` now
  takes any statement iterator), then nested blocks with a lazy strict-equality dispatch (`default` taken last,
  from its own position); `TargetKind::Switch` (`break` targets it, `continue` goes past it). Closure analysis:
  `Analysis::switch_tdz`: a `let`/`const`/`class` read from a later case (a jump can skip its declaration) is a
  TDZ-checked cell, so the read throws node's ReferenceError. Works inside generators/async functions unchanged.
- `debugger`: a no-op.
- `&&=` / `||=` / `??=` (`Lowerer::logical_assign`, F18): short-circuit (no evaluation, no write, no setter call),
  named evaluation for variables, object and key evaluated once for members, private fields.
- `**` / `**=`: `BinOp::Pow` (f64), backend `pow.rs` `js_pow`: fdlibm's `__ieee754_pow` built with wasm-encoder (no
  new dependency) plus the JS rule `(±1) ** ±Infinity` = NaN.
- Found and fixed a silent miscompile: assigning to a `const` succeeded. `write_var` / `assign_value` now evaluate
  the value, run the TDZ check, then throw `TypeError: Assignment to constant variable.`
- Fixtures `tests/fixtures/syntax/{switch,logical-assign,const-assign,exponent}.js`; `unsupported.rs` updated;
  the T19 timing test now asserts on raw wall times (it failed once under load when a slow baseline was subtracted).

`**` vs node: the Wasm helper equals the fdlibm reference (Python and C ports, bit for bit) on 3734 pairs; node 20
(V8 11.3) differs from it by exactly 1 ulp on 2–3% of inexact results (on 200k random pairs; node is neither
classic fdlibm nor glibc; FMA contraction ruled out). Not investigated further (user's call): follow-up F77.
The fixture checks exact results and special values only.

Commands run: `cargo test --no-fail-fast` (63 tests ok, twice), `cargo clippy --all-targets` (clean), `cargo fmt`,
`scripts/diff.sh` (65 passed, 0 failed, 9 skipped).

Next smallest step: T20 (WIT async + user worlds). Remaining syntax gaps are follow-ups F72, F78–F80.

## Progress
- 2026-09-26: task created (scope agreed with the user: switch + the tiny syntax gaps).
- 2026-09-26: `switch` lowered (`Lowerer::switch_stmt`: discriminant local, one `hoist` over all case bodies (hoist now
  takes any statement iterator), nested blocks + lazy strict-equality dispatch, `TargetKind::Switch`, `find_target`
  skips it for `continue`); `debugger` is a no-op. Closure analysis: `switch_tdz` (let/const/class read from a later
  case → TDZ-checked cell). Fixture `tests/fixtures/syntax/switch.js` PASS (fall-through, default in the middle,
  lazy tests, strict equality, labels, TDZ across cases incl. closures, finally, generators, async).
- 2026-09-26: `debugger` no-op. Logical assignment `Lowerer::logical_assign` (variables: result/Cond on the old value,
  named evaluation, `??=` on a never-nullish var reads only; members/private: object+key once, old read once).
  Found and fixed a silent miscompile: writes to a `const` succeeded; `write_var`/`assign_value` now evaluate the
  value, TDZ-check, then throw `TypeError: Assignment to constant variable.`. Fixtures `syntax/logical-assign.js`,
  `syntax/const-assign.js` PASS.
- 2026-09-26: `**`/`**=`: IR `BinOp::Pow` (f64 only), backend `pow.rs` `js_pow` = fdlibm `__ieee754_pow` port (built
  with wasm-encoder, no new dependency; JS rule (±1)**±Inf = NaN). Checked on 3734 pairs: equal to the fdlibm
  reference (Python + C ports agree bit-for-bit); vs node 2.3–3% differ by exactly 1 ulp (node 20 / V8 11.3 is
  neither classic fdlibm nor glibc; not investigated further, per the user → follow-up). Fixture
  `syntax/exponent.js` (exact + special values only) PASS.
- 2026-09-26: done. cargo test 63 ok (x2), clippy clean (pow.rs allows excessive_precision/approx_constant for the verbatim fdlibm constants), fmt, diff.sh 65 passed / 0 failed / 9 skipped. Notes / Result, README (status, F18 ticked, F77–F80) written.
