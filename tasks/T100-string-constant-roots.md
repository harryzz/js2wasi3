# T100: string constants as one GC root (F322)

Status: done, no code kept (2026-10-10) | Depends on: T98 | Local

## Goal
Every string constant in a module is its own mutable `(ref $string)` global (T57): ~600–2,000 in the benchmarks,
~60,000 in zigbee2mqtt. wasmtime walks every GC-ref global on **each** collection (`trace_vmctx_roots`,
`RegisteredType::root`, `RefType::from_wasm_type`), so root scanning costs time in proportion to the number of
constants times the number of collections. Keep the constants in one array (one root) and decide by measurement
whether that pays. **Measure first; implement only if step 1 says so** (F322's own note, T98).

## Why (measured already; don't re-measure these)
- T98 (cloud, default heap, before its 16 MiB reservation), roots vs copying share of time: RayTrace 19.8% /
  13.2%, DeltaBlue 7.9% / 11.7%, EarleyBoyer 2.9% / 29.0%, knucleotide 1.2% / 7.2%, binarytrees 0.3% / 46.9%,
  Splay 0.3% / 59.1%. Roots matter where collections are frequent and the live set small.
- zigbee2mqtt (512 MiB heap, F347): root scanning ~1% (T98), ~1.7% at 256 MiB (T74).
- T94's `tasks/T94-files/keys.js` (`built` case) on the default heap: 5,800 ms vs 315 ms with 256 MiB.
- T98 since then reserves 16 MiB at start (`build --gc-reserve`, default 16), so collections are rarer: RayTrace
  +73%, EarleyBoyer +28%, DeltaBlue +25%. **Not measured: the root share with the reservation in place.** That's
  step 1.
- T90's lesson: anything live in every collection costs on the default heap (a 1,024-entry ref array made a
  GC-bound micro-benchmark ~2× slower). One array of N constants is still N fields to trace per collection, so
  the gain is the per-*global* overhead (type lookups, `RegisteredType::root`), not the field count. Step 1 must
  show which of the two dominates.

## Read first
- `docs/design.md`: T09/T57 (string constants as globals filled by the start function), T74 (constant keys are
  interned per module; the inline caches compare key **identity**), T98's note (the reservation).
- `crates/backend/src/runtime.rs`: `string_global`, `Cx::string`, `Cx::str`; `crates/backend/src/lib.rs`: `fn
  string` (interning) and the `consts$init<k>` start functions.
- `tasks/T98-gc-heap.md` (measurements, how they were taken).

## Plan
1. **Measure (decision gate).** On the laptop with wasmtime 49.0.2, default flags (so the 16 MiB reservation
   applies): perf of RayTrace and DeltaBlue (`scripts/bench.sh` builds) for the root share
   (`trace_vmctx_roots` + `RegisteredType::root` + `from_wasm_type` + its `Drop`). Then a probe: one GC-heavy
   loop in modules with 0 / 1,000 / 10,000 / 60,000 extra unused string-constant globals vs the same constants in
   one array, to get the cost per global per collection and per array field. Write both in Progress and **ask the
   user** whether to implement (expected gain on the default heap, and on zigbee2mqtt at 512 MiB).
2. If yes: one global `$strs` holding `(array (mut (ref $string)))` (non-null elements: `array.new` with the
   placeholder, then filled by `consts$init<k>`); a use is `global.get $strs; i32.const i; array.get` (no null
   check). Identity stays (each constant made once). The 421 use sites (`cx.str(…)` / `Cx::string` then
   `global_get`, 18 files) need one emitting helper (e.g. `Cx::push_str(s, units)`) instead of a global index:
   change the trait, not every site by hand where avoidable. BigInt constants: the same, if many.
3. Measure the cost of the extra load at a hot constant-key site (EarleyBoyer, knucleotide, T90's `adds.js`) and
   the gain on RayTrace/DeltaBlue; compare with the probe.
4. Tests: `scripts/diff.sh` on `tests/fixtures/builtins`, `objects`, `classes`, `modules`, `commonjs`
   (constants everywhere), `wasm-tools validate`, smoke run, `cargo test`; then (ask first) full test262 on a-03
   and one zigbee2mqtt replay against T97's 20.28 s.

## Acceptance
- [x] Step 1's numbers in Progress, and the user's decision (implement, then measure).
- [x] Implemented and measured; every benchmark slower, so reverted (user, 2026-10-10): F322 closed with the
  numbers, F386 for the many-constants case. No test262 run or replay (no code kept).

## Notes / Result
**Result: no code kept** (user's decision, 2026-10-10). The implementation is kept as a patch,
`tasks/T100-files/one-array.patch` (applies to e267c0c; `git apply`), with the site-rewriting script
`tasks/T100-files/fix_str_sites.py` (reads rustc's JSON errors, turns `global_get(<Str>)` into `str_get`).

**Measured** (laptop, wasmtime 49.0.2, default flags, i.e. with T98's 16 MiB reservation):
- Root scanning in the benchmarks is now small: RayTrace ~0.4%, DeltaBlue ~0.2% (T98 before the reservation:
  19.8% / 7.9%), with ~1,100 constant globals each.
- The cost is per GC-ref global, not per traced field (`T100-files/gen.mjs` probe, a GC-heavy loop with n extra
  strings, min of 5): 60,000 constant globals 1,755 ms vs the same strings in one array 805 ms (none: 724);
  roots ~30% of the 60,000-globals profile (`RegisteredType::root` 15.9%, `trace_vmctx_roots` 14.2%).
- With all string constants in one `(ref $strs)` global: the probe's roots 30% → 0.16%, but each constant use is
  `global.get` + `i32.const` + a bounds-checked `array.get` instead of one `global.get`, and every benchmark got
  slower: RayTrace 6.88 (8.01) → 8.13 (8.23) s, DeltaBlue 4.11 (4.12) → 4.26 (4.27), EarleyBoyer 11.29 (11.37) →
  11.36 (12.21), knucleotide 5.62 (5.73) → 5.85 (6.10) (min (median) of 3, all outputs = node); `.wasm` +4–7%.

**What the patch did**: runtime type `$strs` (29; `$bigint` 30) and global `G_STRS`, filled by `consts$init<k>`;
`Cx::string`/`str` return `runtime::Str`, pushed by `StrGet::str_get`; `KeyConst` for key lists that mix string
constants and `G_SYM_ITERATOR`; BigInt constants kept their globals. Tests passed with it: `cargo test`, fixtures
builtins/objects/classes/modules/commonjs.

**Commands**: `scripts/bench.sh -r 1/-r 3` (RayTrace, DeltaBlue, EarleyBoyer, knucleotide), baseline from a
worktree at e267c0c; perf (`--profile=perfmap`); `node tasks/T100-files/gen.mjs <n> globals|array`;
`scripts/diff.sh` on five fixture directories; `cargo test`; `wasm-tools validate` + a hello-world run.

**Not done / next**: a hybrid (only constants used at module level, or used once, in the array) wasn't tried;
it would help only programs with tens of thousands of constants that run on the default heap (F386). Next
smallest step: none in this area; zigbee2mqtt runs with a 512 MiB heap, where roots are ~1%.

## Progress
- 2026-10-10: handoff written (user: F322 next, in a new session).
- 2026-10-10: started; tools unchanged. Step 1 (laptop, wasmtime 49.0.2, default flags, so T98's 16 MiB
  reservation applies; release compiler at e267c0c):
  - perf, `scripts/bench.sh` builds (`fixed 0.05`): RayTrace 9.98 s (node 0.38), DeltaBlue 6.15 s (node 0.19).
    Root share (`RegisteredType::root` + `trace_vmctx_roots` + `from_wasm_type` + its `Drop`): **RayTrace ~0.4%,
    DeltaBlue ~0.2%** (T98 before the reservation: 19.8% / 7.9%). Copying (`forward`, `scan_field`,
    `collect_increment`, memmove) <1% too. Globals: RayTrace 1,221 (1,134 `mut (ref …)`), DeltaBlue 1,103 (1,023).
    So the benchmarks have nothing left to gain.
  - Probe `T100-files/gen.mjs` (3M iterations allocating `{a, b: [..]}`; n extra strings as n unused string
    literals = n constant globals, or built at run time into one array = one root), min of 5 runs (noisy laptop,
    load ~1.5), ms: none 724; globals 1,000: 679, 10,000: 1,089, **60,000: 1,755**; array 1,000: 833, 10,000: 812,
    **60,000: 805**. perf of 60,000 globals: `RegisteredType::root` 15.9% + `trace_vmctx_roots` 14.2% (~30%);
    60,000 in an array: roots not in the top. The cost is per **global**, not per traced field: +1.0 s for 60,000
    globals vs +0.08 s for a 60,000-entry array in the same run (no collection count available).
  - So: matters only for programs with tens of thousands of constants that collect often: zigbee2mqtt (~60,000)
    without the 512 MiB flag; at 512 MiB its root share is ~1% (T98). Benchmarks (~1,100): ~0. Cost not yet
    measured: one `array.get` (bounds check) per constant use. Asking the user.
- 2026-10-10: user: implement, then measure. Implemented: runtime type `$strs` = `(array (mut (ref $string)))`
  (index 29; `$bigint` now 30, `TYPES` 30); runtime global `G_STRS` (immutable `(ref $strs)`, made by
  `array.new` of the empty string × the string count, filled by `consts$init<k>` with `array.set`). `Cx::string`/
  `str` return `runtime::Str` (an index), pushed by `StrGet::str_get` (`global.get G_STRS; i32.const i; array.get`);
  `KeyConst` for key lists mixing string constants and `G_SYM_ITERATOR`. BigInt constants keep their globals
  (`const_index` numbers strings and BigInts apart). The ~560 use sites were moved by the type change: a script
  (`scratchpad`, rustc JSON spans) rewrote `global_get(<Str>)` → `str_get` (528 sites), the rest by hand (helper
  parameters in promise, props, inspect, numstr, host, array, object, func). Builds without warnings; hello world
  validates and runs (154 globals left).
- 2026-10-10: tests: `cargo test` ok; `scripts/diff.sh` builtins 72 (+1 skip), objects 13, classes 12, modules 14, commonjs 15: all pass. Probe after: 60,000 literals 1,081 ms min (before 1,755; empty 896 in this round: noisy laptop), roots 30% → 0.16% of its profile. Benchmarks before (worktree at e267c0c) / after running.
- 2026-10-10: step 3, `scripts/bench.sh -r 3` per benchmark, before (worktree e267c0c) then after, laptop,
  default flags; s, min (median); all SAME as node: RayTrace 6.88 (8.01) → 8.13 (8.23); DeltaBlue 4.11 (4.12) →
  4.26 (4.27); EarleyBoyer 11.29 (11.37) → 11.36 (12.21); knucleotide 5.62 (5.73) → 5.85 (6.10). Every one slower
  (~+1–18% min, +3–7% median); `.wasm` +4–7% (394 → 411, 248 → 262, 958 → 1000, 1,382 → 1,473 KB: a use is three
  instructions instead of one). The benchmarks gain nothing (roots ~0.2–0.4% with the reservation) and pay the
  `global.get G_STRS` + bounds-checked `array.get` at every constant use. Asking the user (revert, or keep for the
  many-constants case).
