# T136: element-to-element copies unboxed (F462)

Status: done (2026-10-19): measured and dropped, no code kept | Depends on: T134 | Local (+ a-03 for the gate and test262, after asking)

## Goal
`a[i] = b[j]` reads the element through `get` (boxing a number from an array in number mode, `box_number`) and
writes it back through the generic store path (unboxing it again). After T134 this is fannkuchredux's largest
remaining helper cost (`box_number` ~6%). Read the value as T134's element reads do (`ElemNum`, `get` only on a
miss) and write it with the number path when it is a number (T134's split-value write).

## Read first
- `tasks/T134-typed-function-versions.md` (Notes / Result), `docs/design.md` "Speed work" (split number slots).
- `crates/lower/src/lib.rs`: `member_assign`, `sum_operand`; `crates/backend/src/lib.rs`: `elem_set_split`.

## Plan
- [x] Count: element copies in the 18 programs (sites, executions), the boxing they cost.
- [x] Design and build; fixtures (copies between number-mode, generic and typed arrays, holes, objects).
- [x] Laptop counts and size; gate on a-03 (ask). test262 not run (dropped before it).

## Notes / Result
- **Dropped** (user, 2026-10-19). a-03 gate vs 38b93a6: geomean **0.9992**; navier-stokes −1.8% time at −5.14%
  instructions, fannkuchredux +3.8% at −0.29% (placement), the rest ±2%; `.wasm` mean +0.11% (navier-stokes +2.04%).
- **Why so little:** fannkuchredux's copies were never boxed (its arrays keep `i31`s in the generic store: 177M
  generic write hits), so T134's note blaming them for its `box_number` was a guess; only navier-stokes copies
  doubles in number mode.
- **Kept:** `tests/fixtures/arrays/element-copies.js` (passes before and after), the code as
  `tasks/T136-files/element-copies.patch` (`num_elems` in the analysis, `Intrinsic::ElemCopy` fusing the read into
  the write path, a dropped `seq`'s pure value not computed, two-path split-value writes).
- **Also from this task:** the geomean correction (T134 0.9327, T135 1.25) and `scripts/gate.sh` printing the
  geomean of minimum times.
- **Learned:** count whether the guessed cost happens (`build --count`) before building, not only the sites.

## Progress
- 2026-10-19: started (user: "start the element copies follow-up"); tools unchanged.
- 2026-10-19: count: copy sites `a[i] = b[j]` in the 18 (user code): earley-boyer 18, navier-stokes 11, crypto 4,
  fannkuchredux 3, mandelbrot 2, raytrace 1, the rest 0. **Premise wrong for fannkuchredux** (T134's note):
  `build --count`: all 177M of its element writes hit the generic store with `i31`s, so its copies never boxed
  (its `box_number` ~4.8% is elsewhere). navier-stokes copies doubles in number mode (boxed through `get`).
- 2026-10-19: design: the type analysis records each computed read's values (`Result::num_elems`: only numbers,
  + unit test); at such a copy the lowering emits one `Intrinsic::ElemCopy` (dst, key, src, key, v, t): the read
  (as `get`'s array fast paths: a generic-store element as it is, a number-mode element unboxed, `get` for holes
  and non-arrays) goes straight into its own `elem_set` write path (generic / number / generic); `v`/`t` keep the
  value as a split slot's parts for the assignment's value. A dropped `seq` with a pure final value (a local, a
  constant, a split read) no longer computes it (a split read would box). A split-value write with pure operands
  takes the number path when `o` is null, else the generic inline path.
  Tried first (laptop instructions vs T134): read as `ElemSplit` + split-value write: fannkuchredux +11.1% (the
  number round trip of `i31`s), then +3.7%, +1.3% (the null test choosing the write path) → fused `ElemCopy`.
- 2026-10-19: `ElemCopy` (laptop, instructions / `.wasm` vs T134, outputs same): navier-stokes **−5.14%** / +1.78%,
  fannkuchredux −0.29% / +0.82%, raytrace −0.73%, mandelbrot +0.26% / +0.50%, crypto +0.02%, earley-boyer,
  spectralnorm, nbody 0%.
- 2026-10-19: fixtures: all pass (new `tests/fixtures/arrays/element-copies.js`: number-mode/generic/typed arrays,
  holes, past the end, an inherited element, a frozen destination in strict code; node/fetch timed out once under
  `-j 4`, passes alone). A sloppy-mode write to a frozen array throws in js2wasi3 (already so at 671e315:
  CommonJS is compiled as strict code, F232), so the fixture's frozen case is strict.
- 2026-10-19: laptop run of the 18 vs T134 (`out/gate-t136-laptop.md`; times advisory): instructions navier-stokes
  −5.15%, fannkuchredux −0.29%, mandelbrot +0.26% (its 2 copies are typed-array reads: `get` either way), crypto
  +0.01%, the rest ±0.00%; outputs = node; `.wasm` mean +0.11% (navier-stokes +2.04%, fannkuchredux +0.82%).
  Next: the a-03 gate (asked the user).
- 2026-10-19: user: a-03 gate, then test262. `scripts/gate.sh -o out/gate-t136 38b93a6` started on a-03.
- 2026-10-19: a-03 gate vs 38b93a6 (`out/gate-t136-a03.log`, `out/gate-t136/`): geomean **0.9992** (minimum
  times); navier-stokes −1.8% time at −5.14% instructions (verdict same), knucleotide −2.1% at 0 instructions;
  fannkuchredux **+3.8%** at −0.29% instructions (placement), the rest ±1.7%; outputs = node; micros: no verdicts.
- 2026-10-19: found while computing it: my geomeans from `gate.tsv` used each program's first run, not the
  minimum: T134's gate is **0.9327** (reported 0.9117), its first gate 0.9333 (0.9135), T135's wasm-opt 1.25 (1.21);
  docs corrected; `scripts/gate.sh` now prints the geomean of minimum times itself.
- 2026-10-19: user: drop (option 2). Code reverted, kept as `tasks/T136-files/element-copies.patch`; task done.
