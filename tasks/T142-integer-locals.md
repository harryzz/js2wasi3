# T142: integer locals (F475)

Status: in progress | Depends on: T134, T140, T141 | Local (+ a-03 for the gate and test262, after asking)

## Goal
T138/T140: integers are `f64` everywhere, so fannkuchredux and crypto convert `f64 ↔ i32 ↔ i31` on every index and
bit operation. The probe (`probes/fannkuch-*`) shows `i32` counters with an `i32`-valued element path 27% faster
than today's shape on wasmtime (before the typed receiver, which T141 did). Find the cheapest design that gets most
of it without a deoptimizing second version of the function.

## Read first
- `tasks/T140-int-locals-probe.md`, `tasks/T141-typed-array-locals.md`, `tasks/T134-typed-function-versions.md`
  (split number slots), `docs/design.md` "Speed work".
- `crates/lower/src/lib.rs` (`num_vars`, `numeric_use`, `Place::Split`), `crates/types/src/lib.rs` (no integer facts yet).

## Plan
- [x] Attribute the probe's gain: which part of "integer locals" pays (key conversion, value conversion, boxing call,
  `i32` arithmetic with overflow checks) with more probe models on the T141 baseline.
- [ ] Design (overflow without deoptimization: exact `i32` variables fed only by bit operations; bounded counters;
  an integer-valued `f64` invariant) and a count of the variables each rule selects in the 18 programs.
- [ ] Build, fixtures, laptop counts/size; gate on a-03 (ask); test262 (ask).

## Notes / Result
(in progress; probe attribution below)

## Progress
- 2026-10-20: started (user: "push 72e4a99, integer locals (F475) the next one"). Pushed 72e4a99. Tools unchanged.
- 2026-10-20: attribution probe (`probes/fannkuch-gen.py` models 4-6, `probes/fannkuch-run.sh`, laptop under load so
  the ratios count: M0 4,430 ms): wasmtime ms (vs M0), V8 ms:

  | model | what | wasmtime | vs M0 | V8 |
  |---|---|---|---|---|
  | M0 | today before T141 | 4,430 | | 2,512 |
  | M4 | M0 + receiver typed once (the compiler after T141) | 4,054 | −8.5% | 2,401 |
  | M5 | M4 + keys known to be integer-valued `f64` (`trunc_sat` only, no `convert`/`ne` check) | 3,635 | −18% | 2,283 |
  | M6 | M5 + the written number boxed inline (`i31` when it fits) instead of a `box_number` call | 2,904 | **−34%** | 2,205 |
  | M3 | `i32` counters + typed receiver (generic store) | 1,788 | −60% | 755 |
  | M2 | M3 + `i32` element store | 1,436 | −68% | 498 |

  Reading: from M4 (where the compiler is now) the inline box is the biggest single step available without a new
  representation (−21% of M4), known-integer keys −10%, `i32` locals and values the rest (M6 → M3 −38%).
- 2026-10-20: **correction of T139's conclusion.** T139 was judged on instructions (fannkuchredux −3.0%) and dropped
  without an a-03 gate; the probe says the `box_number` call costs far more time than its instructions (M5 → M6
  −20% of the time). T139's patch (`tasks/T139-files/split-get-inline-box.patch`) is not measured on a-03: to do first.
- 2026-10-20: user: yes. Pushed 40f0052. T139's patch applied to the working tree; `scripts/gate.sh -o out/gate-t142a 40f0052` started on a-03 (`~/gate-t142a.log`) to measure inline boxing on top of T141.
- 2026-10-20: a-03 gate of T139's patch on top of T141 (base 40f0052; `out/gate-t142a-a03.log`, `out/gate-t142a/gate.tsv`):
  geomean **0.9904** (minimum times); fannkuchredux 3,280 → 3,193 ms (**−2.7%**, instructions −3.77%); crypto −3.4%
  (instructions −1.28%); mandelbrot −3.3% and pidigits −3.2% at 0.00% instructions (placement); navier-stokes
  instructions −0.92% (time −0.6%); the other 12 within ±1.7%; `.wasm` +0.01…+0.93% (crypto +0.93%, navier-stokes
  +0.40%, splay +0.37%, fannkuchredux +0.15%). Micros: 4 "slower" (`instanceof`, `getPrototypeOf` +1.5…2 ns) and 7 "faster"
  (`v == null` −2 ns): placement, none of those paths changed.
- 2026-10-20: **the correction above is itself wrong.** Inlining the call removed fannkuchredux's 64.2M `box_number`
  calls (302 left) and gave −2.7% time, not the probe's −21%: the probe's `$box` call is costlier than the real helper
  (a model, not a measurement; the same caveat applies to its other steps). T139's original judgement (a small gain, a
  few percent on two programs) stands; what changes is the cost/benefit on the current base: geomean 0.9904 of which
  ~0.5–1.0% is placement. Decision for the user: keep the patch or leave T139 dropped.
- 2026-10-20: user: option 2 (leave T139's patch dropped). Patch reverted from the working tree (still in `tasks/T139-files/`); the gate result is recorded above.
