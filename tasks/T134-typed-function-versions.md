# T134: typed function versions (step 3 of the new design)

Status: done (2026-10-19; re-planned after step 0: split number slots) | Depends on: T000, T131, T132, T133 | Local (+ a-03 for the gate and test262, after asking)

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
- [x] Step 0, measure (before any design): profile fannkuchredux and crypto (V8's `--prof` on the jco component
  and `perf` on wasmtime, `build --count`): how much is boxing at calls/returns, untyped locals, `ToInt32` and
  `f64` ↔ `i32` conversions, array element access, method dispatch. If boxing isn't the main cost, report and
  re-plan before building.
- [x] Step 1, design (re-planned with the user: split number slots instead of function versions): which functions get a typed version (analysis: parameter types per function from all call
  sites; `i32` where values stay integers, as V8's Smi/int32 representation), how a call site picks a version
  (types known at build time → direct; else a guard on the arguments), the result type, recursion, and the size
  cost stated up front (typed versions duplicate code).
- [x] Step 2, build; fixtures with typed and generic callers of one function, a non-number reaching a typed site.
- [x] Gate on a-03 (ask), test262 (ask), `scripts/v8bench.sh` for the gap to node (a-03, after the commit).

## Notes / Result
- **Re-plan.** Step 0 (wasmtime `perf`) showed boxing at calls/returns isn't the cost: fannkuchredux's hot function
  is called once; crypto's `am3` spends ~45% boxing/unboxing its parameters and locals *inside* the function
  (values from element reads and parameters of a function used as a value, which the lowering's own inference
  can't type but the analysis can). The user chose **split number slots** over typed function versions.
- **What changed.** `crates/types`: `Result::num_vars` (variables whose every stored value is a number). Lowering:
  `Place::Split(o, f)` for uncaptured locals/simple parameters of plain functions that the lowering makes `jsval`,
  the analysis types as numbers, and that have a numeric use (`numeric_use`); `o` is null while the value is the
  number in `f`, else holds the value (a box kept) with `f` its number or HOLE. Writes go through T108's
  `sum_operand` (element reads and `+` trees unboxed); parameters keep the argument and unbox it at entry.
  IR: `SplitGet`, `GetElemCall` (an unboxed read's miss as a `get` call), `JsAddParts` (a `+`'s generic path as one
  call, runtime `js_add_parts`). Backend: split reads as numbers/values, split keys and values in the inline
  element reads/writes, `split == number`, `--x`; `ElemNum` with a computed key reads numbers from number mode
  and from a generic array's store (`i31`/`$number`), inline in loops, else `elem_num` with an `f64` key; `+o[k]`
  with an `f64`/split key the same; `key_temp` keeps non-constant `f64` keys unboxed. Always correct: anything
  the analysis gets wrong still fits in `o` (fixture `tests/fixtures/arrays/split-slots.js`).
- **Result** (a-03 gate vs 671e315, `out/gate-t134-v2-a03.log`): geomean of the 18 programs **0.9117**; crypto
  −31.3% (instructions −38.5%), navier-stokes −27.0% (−29.5%), fannkuchredux −20.5% (−24.8%), spectralnorm −13.8%
  (−23.5%), mandelbrot −12.8% (−1.5%), fasta −6.1% (−7.8%); nothing slower (deltablue +0.3% at +0.08%
  instructions). `.wasm` +0.7…+5.2% (crypto +5.17%, navier-stokes +3.07%; mean ~1.6%). Micros: 1 slower
  (`typeof o [array]` +1.5 ns at equal instructions: placement), 5 faster. test262: no changes (28,123,
  `out/t262-t134.log`). Fixtures: all pass.
- **Found on the way:** dropping a parameter's box made splay +3.8% instructions (re-boxing per use: the box is
  kept now); splitting variables never used as numbers (`this.x = x`) cost `new P` +4.2 ns (`numeric_use` now);
  a constant typed-array index kept as `f64` boxed per miss (mandelbrot +3.2%; constants stay boxed).
- **Tried and dropped:** the unboxed element read as a call everywhere (spectralnorm −15.7% instead of −23.5%
  instructions); no split slots in the prelude (−0.1…0.2% size only); a NaN test in numeric reads (no smaller).
- **Commands:** `perf record`/`perf stat -e instructions:u` (laptop), `scripts/bench.sh -r 2 -b <base>` (laptop
  counts), `scripts/diff.sh`, `scripts/gate.sh -o out/gate-t134-v2 671e315` (a-03), `node scripts/test262.mjs
  subset` (a-03).
- **Not done (follow-ups):** booleans (F461), element-to-element copies (F462), typed arrays in the unboxed reads
  (F463), typed parameters/results at calls, the original T134 idea (F464), the size of hole tests (F465). The V8
  gap (F466) was measured after the commit: 5.19× node (see Progress).
- **Next smallest step:** F462 (fannkuchredux: `perm[i] = perm1[i]` still boxes through `get`, ~6% of its time).

## Progress
- 2026-10-18: started; tools unchanged (check-tools ok). Step 0: profiling fannkuchredux and crypto.
- 2026-10-18: step 0, wasmtime `perf` (laptop, 999 Hz, `--profile=perfmap`; fannkuchredux 10, crypto `fixed 0.05`;
  builds of the patched/concatenated sources at 671e315). Leaf samples by direct caller:
  - fannkuchredux (4.30 s): one function called once, so **no call boundary at all**. `fannkuch` itself 59.5%;
    helpers 40.3%: `box_number` 16.7%, `to_number` 10.8%, `loose_eq` 7.2%, `get_num` 2.7%, `is_nullish` 2.0%,
    `to_boolean` 0.9%. Cause (IR): the locals `k`, `t`, `p0` are filled from element reads and `more` is a hoisted
    `var`, so the lowering's own inference makes them `jsval`; every element read boxes the `f64` element, every use
    unboxes it, `k != 0` is `loose_ne`. The analysis (T131) types all of them as numbers (fannkuchredux 100% typed and right).
  - crypto (4.03 s): `am3` and its helpers ≈ 79%: `am3` 31.5%, `box_number` 21.1%, `get_num` 9.8%, `to_number`
    8.6%, `js_add` 4.0%, `arg_get` 1.5%; `get_named` ~6.5% (montReduce, bnpSquareTo, am3). `am3` is reached only as
    `x.am(…)` through its `$code` adapter (argument array; T132's direct call gives up, F450), so all its parameters
    are `jsval`; `i`, `j`, `n`, `c` and the local `l` (`… + w_array[j] + c`) re-box on every iteration (F404).
  - **Verdict: boxing at calls/returns is not the main cost.** The cost is numbers boxed in parameters and locals
    *inside* the hot function, from values whose type the lowering can't see (element/property reads, parameters
    of functions used as values) but the analysis can (root R2; F403, F404). Typed versions with unboxed
    parameters at direct calls would not touch fannkuchredux and only part of `am3`. Re-plan before building
    (asked the user).
- 2026-10-18: user chose the re-plan: **split number slots** (a parameter/local the analysis types as a number gets an `f64` local plus a `jsval` fallback local, null while the value is a number; no copies, no deoptimization). Step 1 (design) next.
- 2026-10-18: step 1, design (split number slots):
  - Analysis (`crates/types`): `Result::num_vars`, the variables/parameters whose set over every copy of their
    function is exactly `{Num}`.
  - Which slots: uncaptured locals and simple parameters of plain (not generator/async) functions that the
    lowering's own inference makes `jsval` (kind `Any`) and the analysis types as numbers. `f64` slots unchanged.
  - Representation `Place::Split(o, f)`: `o: jsval` is the Wasm null (`Intrinsic::Hole`) exactly when the value
    is the number in `f: f64`, else it holds the value. Always correct, whatever the analysis says.
  - Reads: new `Intrinsic::SplitGet(o, f) -> jsval` (`o` null ? box(f) : o). The backend recognises it where it
    now recognises a boxed `f64` (`Convert(F64→JsVal)`): a numeric use (`Convert(JsVal→F64)`) is
    `o` null ? f : ToNumber(o); element keys and values take their `f64` paths; `==`/`!=` against a number.
  - Writes: a boxed `f64` → f = x, o = null; another split slot → both copied; an element read `a[k]` → f =
    `ElemNum`, the generic read only on a miss; anything else → f = `NumOrHole(v)`, o = null or v.
  - Parameters: at entry f = `NumOrHole(p)`, and p (used as `o`) set to null if it was a number.
  - Left out (count first, later): `+` of two `jsval`s the analysis types as numbers (`js_add`), booleans
    (fannkuchredux's `more`), results, cells, globals.
  - Size: each read adds a branch (~10 bytes), each write ~15–25 bytes; measured with the gate.
- 2026-10-18: step 2, built (laptop, uncommitted): `num_vars` in `crates/types` (+ unit test); `Place::Split` and
  `Intrinsic::SplitGet` (lowering: locals via `local_for`/`declare_store`/`write_var`, parameters unboxed at
  entry; stores through T108's `sum_operand`); backend: `SplitGet` as a value and as a number, `--x` of a split
  slot, split keys/values in the inline element reads/writes, `split == number`; `ElemNum` with a computed key
  inline (also numbers in a generic array's store, as `get_num`); `+o[k]` with an `f64`/split key inline;
  `key_temp` keeps `f64` keys unboxed. `wasm-tools validate` ok, outputs as before.
  Laptop, instructions (`perf stat -e instructions:u`) / min of 3 times / `.wasm`:
  fannkuchredux 51.20G → 38.44G (−24.9%), 5.10 → 4.23 s; crypto 50.58G → 31.34G (−38.0%), 4.05 → 2.80 s;
  `.wasm` fannkuchredux +2.1%, crypto +7.0% (size to look at before the gate). Next: fixtures, size, the 18.
- 2026-10-19: fixtures: `scripts/diff.sh -j 8` all 275 pass (commonjs/ws timed out once under load, passes alone); new fixture `tests/fixtures/arrays/split-slots.js` (holes, missing arguments, past-the-end arguments reaching split slots, split keys/values, `==` against numbers, -0/NaN) passes.
- 2026-10-19: laptop run of the 18 vs 671e315 (`scripts/bench.sh -r 2 -b <base>`, `out/gate-t134-laptop.md`; laptop
  times advisory): geomean 0.9007; instructions: crypto −37.9%, navier-stokes −27.5%, fannkuchredux −24.9%,
  spectralnorm −23.6%, fasta −7.2%, nbody −1.3%; **splay +3.78%, raytrace +1.79%, deltablue +1.04%** (added work,
  to find before the a-03 gate); binarytrees +15.1% time at −0.00% instructions (laptop GC noise/placement).
  `.wasm` +0.9…+6.7% (crypto +6.7%, navier-stokes +5.5%; mean ~2.3%). All outputs as node.
- 2026-10-19: user: find the splay/raytrace/deltablue extra work and cut the size before the a-03 gate.
- 2026-10-19: splay's +3.78% instructions: its `key` parameters were split, unboxed at entry and the box dropped,
  so each use as a JS value (`key < node.key`, stores, calls) boxed again (more garbage; splay is ~90% GC). Fix:
  `o` keeps the box (`f` is then its number or HOLE); numeric uses test `f`, value uses test `o`; split locals'
  `f` start as HOLE at entry. After: splay −0.52%, raytrace +0.47%, deltablue +0.24% instructions (laptop).
- 2026-10-19: size: element reads under a split store/`+` were ~100 instructions each (inline `ElemNum`, an
  inline `get` on its miss, `NumOrHole`, an inline `js_add` miss per `+`). Changes: the miss after `ElemNum` is
  `Intrinsic::GetElemCall` (`get` as a call); a `+`'s generic path one call (`Intrinsic::JsAddParts`,
  `js_add_parts`); `ElemNum` with a computed key inline only inside loops, else the `elem_num` helper with an
  `f64` key (all-call measured: spectralnorm −15.7% instead of −23.6% instructions). Tried and dropped: no split
  slots in the prelude (−0.1…0.2% size only); a NaN test (`f != f`) in numeric reads (no smaller, fannkuchredux
  +1.2% instructions). Now (laptop, vs 671e315): crypto `.wasm` +5.35% / −38.5% instructions, navier-stokes
  +3.34% / −29.5%, fannkuchredux +1.85% / −24.8%, spectralnorm +1.80% / −23.6%.
- 2026-10-19: mandelbrot +3.24% instructions: `key_temp` kept a constant key (`byte_acc[0] <<= 1`, a typed array)
  as an `f64`, boxed by a call on every typed-array miss; constants stay boxed now (mandelbrot −1.5%).
- 2026-10-19: laptop run of the 18 vs 671e315 (`out/gate-t134-laptop.md`, replaced): geomean 0.8758 (advisory);
  instructions crypto −38.47%, navier-stokes −29.49%, fannkuchredux −24.84%, spectralnorm −23.55%, fasta −7.79%,
  mandelbrot −1.50%, nbody −1.14%, splay −0.52%; raytrace +0.48%, deltablue +0.24%, revcomp +0.18%, the rest
  ±0.1%. `.wasm` +0.69…+5.33%, mean +1.72%. Fixtures: all pass (http-server timed out once under `-j 4`,
  passes alone). Next: the a-03 gate (asked the user).
- 2026-10-19: user: run the a-03 gate, then test262 if it passes. Synced; `scripts/gate.sh -o out/gate-t134 671e315` started on a-03.
- 2026-10-19: a-03 gate vs 671e315 (`out/gate-t134-a03.log`, `out/gate-t134/`): geomean **0.9135**, no program
  slower (crypto −31.8%, navier-stokes −27.0%, fannkuchredux −19.6%, spectralnorm −14.4%, mandelbrot −11.0%, fasta
  −5.3%), outputs = node; micros: 10 slower (property-paths: `new P` +4.2 ns, write hit +1.8 ns, …). Cause: `P`'s
  parameters were split though only stored (`this.x = x`): entry unboxing and split reads for nothing (+0.21%
  instructions in the micro, all of it). Fix: split only a variable with a numeric use (`numeric_use`:
  arithmetic/bitwise/comparison operand, unary `-`/`+`/`~`, `++`, `op=`, element key, the value of an element
  write). After (laptop): micro instructions = base exactly; raytrace −0.26%, deltablue +0.08%, the gains the same.
- 2026-10-19: fixtures all pass (fetch timed out once under `-j 4`, passes alone). Re-running the a-03 gate.
- 2026-10-19: a-03 gate vs 671e315 after the `numeric_use` fix (`out/gate-t134-v2-a03.log`, `out/gate-t134-v2/`):
  geomean **0.9117**, no program slower (crypto −31.3%, navier-stokes −27.0%, fannkuchredux −20.5%, spectralnorm
  −13.8%, mandelbrot −12.8%, fasta −6.1%); outputs = node; micros: 1 slower (`typeof o [array]` +1.5 ns) at equal
  instructions (object-kinds micro 158.6177G both, laptop: placement), 5 faster. test262 on a-03 next.
- 2026-10-19: test262 on a-03 (`out/t262-t134.log`, 1,490 s): no changes since the baseline (28,123). Task done: Notes / Result, tasks/README.md (status, F403/F404 ticked, F461–F466), design.md, root-causes.md, test262 HISTORY.md, CLAUDE.md.
- 2026-10-19: committed (93e893f; user). `scripts/v8bench.sh` on a-03 (user; `out/v8bench-t134-a03.md`,
  `out/v8bench-t134/v8bench.tsv`): our components on node's V8 run at **5.19× node** (geomean of the 18 V8 run
  times / node), on wasmtime 7.69×; T133's 6.43× / 9.58× were measured on the laptop, so the machines differ.
  Worst now raytrace 16.0×, fannkuchredux 15.7×, richards 11.1×, deltablue 10.8×, crypto 10.4× (was 18.7×),
  revcomp 9.7×; best pidigits 1.1×, mandelbrot 1.8×. All outputs SAME. F466 closed.
