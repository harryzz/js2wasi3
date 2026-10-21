# T142: integer locals (F475)

Status: done (2026-10-21): integer-valued keys only; `i32` locals not built | Depends on: T134, T140, T141 | Local (+ a-03 for the gate and test262, after asking)

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
- [x] Design (overflow without deoptimization: exact `i32` variables fed only by bit operations; bounded counters;
  an integer-valued `f64` invariant) and a count of the variables each rule selects in the 18 programs.
- [x] Build, fixtures, laptop counts/size; gate on a-03 (ask); test262 (ask).

## Notes / Result
- **What changed (the smallest real piece of F475):** `int_locals` (backend): an `f64` local, not a parameter, whose
  every write is an integer-valued expression (an integral constant, a copy of such a local, an `i32` converted, or
  such an expression ± an integral constant of at most 2^31), by fixpoint. An element read or write whose key is such
  a local (or such an expression) skips the "is it an integer" test: `trunc_sat` alone is the index, a value out of
  range saturates and misses the bounds test, the slow path gets the exact `f64`.
- **Result:** a-03 gate vs 40dbf8c: geomean of the 18 programs **0.9827** (minimum times); spectralnorm −12.9% (instructions
  −5.47%), fannkuchredux −4.2% (−3.99%), nbody −3.7% (−3.28%); mandelbrot −4.0% and pidigits −4.2% at equal
  instructions (placement); nothing slower in work; `.wasm` **−0.13…−0.45%**. test262 on a-03 (`out/t262-t142.log`,
  1,517 s): **no changes**, 28,123 of 28,984 run pass (97.0%).
- **Probe caveat:** the attribution probe (`probes/fannkuch-gen.py` models 4–6) overstated the inline `box_number`
  (−21% predicted, −2.7% on a-03 when gated, left dropped) and the known-integer keys (−10% predicted, −4% real); its
  `i32` models (M3, M2) are therefore upper bounds, not forecasts.
- **Not done (follow-ups):** keys that are not integer locals (`perm[k - i]`: `k` comes from an element read or a
  parameter); real `i32` locals and values read from `i31` elements (the large step; needs overflow handling without a
  second function version); inline boxing (T139 patch, gated on top of T141: geomean 0.9904 but fannkuchredux −2.7%,
  crypto −3.4%, `.wasm` up to +0.93%).
- **Commands:** `build --core` + `wasm-tools print` (`f64.ne` in `fannkuch` 21 → 11), `scripts/diff.sh -j 4`,
  `scripts/gate.sh -o out/gate-t142b 40dbf8c` (a-03), `node scripts/test262.mjs subset` (a-03).

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
- 2026-10-20: user: "Yes" (push 40dbf8c; go with the smallest real piece, integer-valued keys). Pushed 40dbf8c.
  Built `int_locals` (backend): an `f64` local, not a parameter, whose every write (`Set`/`Tee`) is an integer-valued
  expression (`is_int_expr`: an integral constant, a copy of such a local, an `i32` converted, or such an expression ± an
  integral constant of at most 2^31 — constant steps keep a counter finite), by fixpoint. `elem_get`, `elem_num_key`
  and `elem_set_split` skip the `convert`/`ne` integer test for such a key (`trunc_sat` alone; a value out of range
  saturates and misses the bounds test, the slow path gets the exact `f64`). fannkuchredux `fannkuch`: `f64.ne` 21 → 11.
  Laptop: instructions 31.04 G → 29.80 G (**−4.0%**), `.wasm` 198,044 → 197,510 bytes (−0.27%); outputs = node.
- 2026-10-20: fixture `tests/fixtures/arrays/int-keys.js` (counters past the end and negative, copies, steps over 2^31,
  4294967297 not index 1, fractions/NaN/-0/strings, a counter reset to 1.5, nested loops, typed arrays/strings) = node.
- 2026-10-20: user: yes. Fixtures (scripts/diff.sh -j 4): 279 passed, 0 failed, 19 skipped. Synced; `scripts/gate.sh -o out/gate-t142b 40dbf8c` started on a-03 (`~/gate-t142b.log`).
- 2026-10-20: a-03 gate vs 40dbf8c (`out/gate-t142b-a03.log`, `out/gate-t142b/gate.tsv`): geomean **0.9827** (minimum
  times); spectralnorm 372 → 324 ms (**−12.9%**, instructions −5.47%), fannkuchredux 3,277 → 3,140 (−4.2%, −3.99%), nbody
  1,190 → 1,146 (−3.7%, −3.28%); mandelbrot −4.0% and pidigits −4.2% at 0.00% instructions (placement); fasta
  −0.47%, regexp −0.27%, revcomp −0.22%, navier-stokes −0.20% instructions; the rest within ±1% time; outputs =
  node. `.wasm` **−0.13…−0.45%** (smaller everywhere). Micros: 1 "faster" (`split` regex), no "slower". Next: full
  test262 on a-03 (ask).
- 2026-10-21: user: yes to test262. `node scripts/test262.mjs subset` started on a-03 (`~/t262-t142.log`).
- 2026-10-21: test262 on a-03 (`out/t262-t142.log`, 1,517 s): no changes (28,123). Task done.
