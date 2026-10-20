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
