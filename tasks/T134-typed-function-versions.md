# T134: typed function versions (step 3 of the new design)

Status: not started | Depends on: T000, T131, T132, T133 | Local (+ a-03 for the gate and test262, after asking)

## Goal
The last part of T000's design: a function the type analysis (T131) sees called with numbers gets a typed version
(unboxed `f64`/`i32` parameters, locals and result) next to today's generic one; callers that can prove or cheaply
check the argument types call the typed version, everything else the generic one. Today only directly called
functions whose every call stores numbers get `f64` parameters (the lowering's optimistic inference); methods
(T132's direct calls), closures and functions with one non-number call site take and return boxed values.
Targets (V8 run / node after T133, `out/v8bench-t133.md`): fannkuchredux 19.0×, crypto 18.7×, richards 13.9×,
deltablue 12.3×; geomean 6.43×.

## Read first
- `tasks/T000-general-speed-analysis.md` (the design, "Correction" and the probes), `tasks/T131-type-analysis.md`,
  `tasks/T132-method-calls.md`, `tasks/T133-shaped-objects-layout.md` (Notes / Result).
- `crates/lower/src/lib.rs` module docs (optimistic `f64` inference) and the call lowering (typed parameters).
- Follow-ups F446 (analysis misses), F451 (typed parameters/results for directly called methods).

## Plan
- [ ] Step 0, measure (before any design): profile fannkuchredux and crypto (V8's `--prof` on the jco component
  and `perf` on wasmtime, `build --count`): how much is boxing at calls/returns, untyped locals, `ToInt32` and
  `f64` ↔ `i32` conversions, array element access, method dispatch. If boxing isn't the main cost, report and
  re-plan before building.
- [ ] Step 1, design: which functions get a typed version (analysis: parameter types per function from all call
  sites; `i32` where values stay integers, as V8's Smi/int32 representation), how a call site picks a version
  (types known at build time → direct; else a guard on the arguments), the result type, recursion, and the size
  cost stated up front (typed versions duplicate code).
- [ ] Step 2, build; fixtures with typed and generic callers of one function, a non-number reaching a typed site.
- [ ] Gate on a-03 (ask), test262 (ask), `scripts/v8bench.sh` for the gap to node.

## Progress
