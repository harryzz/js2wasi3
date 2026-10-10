# T100: string constants as one GC root (F322)

Status: todo (handoff written 2026-10-10) | Depends on: T98 | Local

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
- [ ] Step 1's numbers in Progress, and the user's decision.
- [ ] If implemented: benchmarks before/after in Notes; full test262 0 regressions; one replay; docs (design.md
  T57 note, HISTORY.md, feasibility doc), follow-ups; F322 ticked. If not: F322 closed with the numbers.

## Notes / Result
(to be written)

## Progress
- 2026-10-10: handoff written (user: F322 next, in a new session).
