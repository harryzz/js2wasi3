# T57: string constants that compile in linear time (F245)

Status: done (2026-10-01) | Depends on: T09 (string literals as globals), T56

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
- [x] Compile time of the global initializers is linear in the strings' total length (numbers in the task file).
- [x] All fixtures still equal node; `cargo test` passes.
- [x] No measurable run-time slowdown on the existing benchmarks/fixtures (record what was measured).

## Notes / Result
**What changed** (`crates/backend/src/lib.rs` `Emitter::const_inits`/`const_base`, `runtime::const_placeholders`,
`runtime::string_global`, `bigint::const_global`): string and BigInt constants are mutable non-null globals
initialized to an immutable placeholder (`global.get` of an empty `$string` / a zero `$bigint`, the first one or two
constant globals), filled by the module's start function. Their contents are one passive data segment (index 1 when
Ryu's tables are segment 0, else 0): UTF-16 units / BigInt words, little-endian. Each constant is
`i32.const offset; i32.const len; call consts$string|consts$bigint; global.set`; the makers are the only
`array.new_data`. 1,024 constants per function `consts$init<k>`; the last is the Wasm start function and calls the
others. No use site changed (`global.get`, still `(ref $string)`), and identity is as before (each constant made once
at instantiation).

**Design questions, decided:** (1) lazy per string was not needed: eager chunked init is linear once
`array.new_data` is out of line; keeping `array.new_fixed` for short strings fails because the initializer function
is superlinear in the *number* of globals too (10k strings ≤ 8 units: 11 s). (2) No churn: globals keep their type.
(3) BigInts get the same treatment (one code path). (4) Uniform for every program; measured no run-time cost.

**Measurements** (debug compiler, `wasmtime compile` wall time; see Progress): `node/buffer.js` 2,525 → 315 ms,
`node/util.js` 1,903 → 245 ms, `node/assert.js` 2,113 → 257 ms, `hello.js` 277 → 51 ms; 1,000 strings of ≤ 400 units
20.4 s → 0.35 s; 10k strings 19.2 s → 0.56 s; constants cost ~20 µs each (40k: 0.75 s CPU). Sizes shrink slightly
(buffer 361,333 → 355,772 B). Run time equal (buffer 13.6/13.0 ms, a 3M-iteration loop on string constants 817–826 ms
both). Tests: `cargo test`, `scripts/diff.sh` on all fixtures (195 pass), test262 String/JSON/BigInt/RegExp
prototype/template-literal: no changes since the baseline.

**Unsupported / found:** F249: `wasmtime compile` time outside function compiles grows superlinearly with many
functions (pre-existing). Reproducer (`node gen3.mjs 40000 > h.js`, 400 functions → 22 s; 20000 → 3.7 s):
```js
const [n] = process.argv.slice(2).map(Number);
let out = "let t = 0;\n";
for (let i = 0; i < n; i += 100) {
  out += `function f${i}() { return [`;
  for (let k = i; k < Math.min(n, i + 100); k++) out += '"same",';
  out += `]; }\nfor (const x of f${i}()) t += x.length;\n`;
}
process.stdout.write(out + "console.log(t);\n");
```

**Next smallest step:** F249 with the reproducer above (which wasmtime phase), then `stream`/`fs` (F238).

## Checklist
- [x] design questions decided and recorded
- [x] implementation
- [x] fixtures, measurements, gating
- [x] docs (design.md, keywords.md if needed, README, HISTORY.md if run), follow-ups

## Progress
- 2026-10-01: handoff written after T56 (`main` at `ff2fc12`; test262 subset 27,826 of 28,799 run pass, 96.6%).
- 2026-10-01: started. Harness in the scratchpad (`gen.mjs`: N distinct strings in one array literal; `measure.sh`:
  build + `wasmtime compile` with cranelift debug logging). Before: 2,000 strings ≤ 40 units → 5.4 s total
  (`module_start` 4.5 s); 10,000 strings ≤ 8 units (~60k consts) → 12.9 s (`module_start` 11.0 s); 1,000 strings
  ≤ 400 units (~200k consts) → 20.4 s (`module_start` 18.3 s). So the initializer grows superlinearly in the number
  of globals too, not only in their length: "short strings stay `array.new_fixed`" is out.
- 2026-10-01: design: constants are mutable globals initialized to an immutable placeholder (`global.get`: an empty
  `$string`, a zero `$bigint`), filled by a start function from one passive data segment. First try (inline
  `array.new_data` per constant) was linear-ish but heavy and still superlinear per function (10k strings: 1,024
  per function 5.2 s CPU, all in one 7.0 s). Second: one out-of-line maker per type, `(offset, len) -> (ref $T)`
  (`array.new_data`), each constant `i32.const; i32.const; call; global.set`: 10k strings 111–184 ms CPU whatever
  the chunk size; 40k strings 743–780 ms (linear, ~20 µs per constant). Kept chunks of 1,024 (bounded functions,
  parallel compile). A 40k-placeholder-global probe (WAT) compiles in 0.25 s (40k `array.new_fixed` globals: 103 s).
  The Wasm start section works inside the component (wasmtime 49, `wit-component`).
- 2026-10-01: found an unrelated superlinearity: 400 top-level functions returning 100-element array literals
  (`gen3.mjs`, one distinct string) → 22 s in `wasmtime compile`, outside any logged function compile (a 20 s gap
  after the last function); 200 → 3.7 s; same before T57's change → follow-up.
- 2026-10-01: implemented (makers `consts$string`/`consts$bigint`, chunks `consts$init<k>`, start section); `cargo test` passes.
- 2026-10-01: `scripts/diff.sh` (all of `tests/fixtures`): 195 pass, 0 fail, 16 skipped (a first run had 1 failure that
  didn't reproduce: a timeout under the parallel load, the run before showed 189 "slow"). Before/after (debug
  compiler, `wasmtime compile`; run = precompiled, best of 15): `node/buffer.js` 361,333 → 355,772 B, compile
  2,525 → 315 ms, run 13.6 → 13.0 ms avg; `node/util.js` 1,903 → 245 ms; `node/assert.js` 2,113 → 257 ms;
  `hello.js` 277 → 51 ms, run equal; 10k strings (`gen2.mjs`) 19.2 s → 0.56 s, run 21 → 20 ms; a hot loop reading
  string constants (3M iterations) 817–826 ms both: no run-time cost.
- 2026-10-01: test262 `built-ins/String`, `JSON`, `BigInt`, `RegExp/prototype`, `language/expressions/template-literal`: 1,860 pass of 2,009, no changes since the baseline (565 s, -j 4).
- 2026-10-01: done. Reverted `cargo fmt`'s changes to unrelated code (the repo isn't rustfmt-clean); rebuilt; `diff.sh` builtins + node 71 pass; hello validates and runs. Docs: design.md D1 (T57 note), tasks/README.md (status, next up, F245 ticked, F249).
