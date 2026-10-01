# T57: string constants that compile in linear time (F245)

Status: todo (handoff written 2026-10-01, not started) | Depends on: T09 (string literals as globals), T56

## Goal
wasmtime compiles a program's global initializers as one function (`module_start`), and every string literal (and
every BigInt constant) is a global initialized by `array.new_fixed` of one `i32.const` per code unit. Register
allocation in that function grows superlinearly, so compile time explodes with the total length of the program's
strings. Make it grow linearly, so large programs (zigbee2mqtt) stay usable:
1. wasmtime's compile time of a program is dominated by its functions, not by its string constants.
2. Programs keep their behaviour; string identity/equality semantics don't change.
3. Fixtures stay byte-identical where reasonable, or the change is uniform and documented (this touches every
   program: decide and record).

## What we know (at handoff)
- Measurements (T55, T56; `WASMTIME_LOG=wasmtime_internal_cranelift=debug wasmtime compile -D logging=y` prints each
  function's compile time; `module_start[0]::Wasm` is the initializers): 1,488 globals / 75,025 `i32.const` → 9.2 s;
  1,828 globals / 174,755 consts (one 66 KB string) → 29.1 s; ~1,000 globals / 11,700 consts → ~1 s. A Buffer
  program today (~360 KB) takes ~2.5 s, most of it the initializers. F246 (T56) removed the biggest strings
  (function source text of generated code), not the scaling.
- Code: `crates/backend/src/lib.rs`: `ConstGlobal::{Str, BigInt}`, `Cx::string` → `const_global`, emitted at
  ~line 451 by `runtime::string_global`; `Expr::Str` lowering ~line 1474; a data section already exists
  (`tables_data`, ~line 567: Unicode tables, T27). BigInt constants: `bigint.rs` ~line 214.
- Wasm constraints: `array.new_data` / `array.init_data` are **not** constant expressions, so data-backed strings
  must be made in code: e.g. globals initialized to `ref.null` and filled on first use (a null check per use), or
  all filled by one generated init function called at start (that's just `module_start` again, unless split into
  chunks), or one big `(array i16)` from a data segment plus `(offset, length)` per string sliced on first use.
- Strings must stay `(ref $string)` where the runtime expects non-null (`string_ref()`); uses: `Expr::Str`,
  `cx.str(..)` in helpers (`global_get(index)` everywhere in the backend: hundreds of sites).

## Design questions (decide first, record in docs/design.md D1/D5)
1. Lazy per string (nullable global + `ref.as_non_null` after an `if null then make` helper call), vs. a chunked
   init (N small init functions run from the start function), vs. keeping `array.new_fixed` for short strings
   (e.g. ≤ 8 units) and data segments for long ones. Measure each on the T56 fixtures before choosing.
2. How backend helpers that `global_get` a string constant keep working (a `Cx::str` that returns an expression
   emitter instead of a global index?), with the least churn.
3. BigInt constants: same treatment or not (they're few).
4. Gating: a uniform change to every program is acceptable only with a measurement showing no run-time cost that
   matters (a null check per string use in hot loops?).

## Plan
1. Measurement harness (scratchpad): compile times of a few fixtures (`node/buffer.js`, `node/util.js`, a large
   synthetic one with 10k strings) per option; pick.
2. Implement; `wasm-tools validate`, fixtures, `cargo test`.
3. Gating/size comparison, targeted test262 (strings, RegExp, JSON), docs, follow-ups.

## Read first
- `tasks/T56-buffer.md` (Progress: the 32 s case), `tasks/T55-node-builtins.md` (Progress: the assert timeout).
- `crates/backend/src/lib.rs` (const globals, data section), `crates/backend/src/runtime.rs` (`string_global`, `Cx::str`).
- `docs/design.md` D1 (heap), T09 notes.

## Tests
`scripts/diff.sh` on the touched fixture dirs (all of `tests/fixtures` once at the end: the change touches every
program), `cargo test`, `wasm-tools validate`, wasmtime compile times before/after (record them).

## Acceptance
- [ ] Compile time of the global initializers is linear in the strings' total length (numbers in the task file).
- [ ] All fixtures still equal node; `cargo test` passes.
- [ ] No measurable run-time slowdown on the existing benchmarks/fixtures (record what was measured).

## Notes / Result
(to be written)

## Checklist
- [ ] design questions decided and recorded
- [ ] implementation
- [ ] fixtures, measurements, gating
- [ ] docs (design.md, keywords.md if needed, README, HISTORY.md if run), follow-ups

## Progress
- 2026-10-01: handoff written after T56 (`main` at `ff2fc12`; test262 subset 27,826 of 28,799 run pass, 96.6%).
