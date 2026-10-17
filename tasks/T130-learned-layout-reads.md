# T130: inline reads of learned layouts (F441)

Status: dropped (2026-10-17): measured on a-03, a regression; code reverted | Depends on: T110, T128, T129 | Local (+ a-03 after asking)

## Goal
T128 gives objects built by `initialize.apply`-style or `superConstructor.call`-style code a layout at run time, but
reads of their fields still go through a call: T110's inline reads try only the slots a key has in the
**build-time** layouts (`layout_slots`, `crates/backend/src/lib.rs:235`; at most `LAYOUT_GUESSES` = 3 slots per key),
and raytrace's keys (`x`, `y`, `z`, `red`, …) are in no build-time layout. So every `v.x` on a `Vector` is a call to
`get_named`, which hits its per-site entry guess (raytrace: `get_named.guess_hit` 41,011,739 per run, T124/T128
counts) but pays the call. User, 2026-10-17: keep T128; T128 + T130 together are judged by the **geometric mean of
the 18 programs** (new/base) and instructions, not by single rows.

The inline read (`lib.rs` ~2525) already works on any plain object: `count > slot && keys[slot] == k` then the value.
It doesn't need a layout id, only a good guess of the slot. What's missing is the guess for keys that only learned
layouts (or dictionaries) hold.

## Evidence so far
- T128 Progress: raytrace `inline_read.hit` 4,162,059 vs `get_named.guess_hit` 41.0M and `proto_or_absent` 11.3M
  (methods, not this task); deltablue `inline_read.hit` 22.8M, `miss_plain` 5.1M, `get_named.guess_hit` 20.7M.
- Profiles (T128, laptop): raytrace `get_named` ~13–14% of instructions.
- Geometric mean after T128 on a-03: 0.995 / 0.998 (two gates).

## Read first
- `tasks/T110-layout-reads.md`, `tasks/T115-inline-element-reads.md` (the inline-read shape and its miss rules),
  `tasks/T128-layouts-other-constructors.md` (Notes / Result, F441–F444), `tasks/T129-gate-fixes.md` (how the gate
  reads now).
- `crates/backend/src/lib.rs`: `layout_slots` (~235), the constant-key `GetProp` path (~2510–2580), `count_miss`.
- `docs/design.md` "Speed work" (rules, incl. T129's paragraph).

## Plan (suggested; ask at each step marked "ask")
- [ ] Count first (rule): for each constant-key read site that calls `get_named` today, how often the entry it hits
  (`get_named.guess_hit`) is at a slot that a build-time guess could have named. Candidates to count, not to assume:
  (a) build time: slots from every function's ordered `this.k =` writes (e.g. `initialize`'s `x, y, z`), not only
  from constructors named by `new` (read guesses only, no allocation layout); (b) run time: the inline read takes its
  slot from the site's existing `G_IC` guess (one load) instead of a constant. A `--count` build or a one-off trace.
- [ ] Consult how V8/JSC inline-cache a monomorphic field load (the load IC's map check + offset) and what that means
  for (b); report and propose a design (ask).
- [ ] Build (after the user's yes); micro cases for hit and every miss kind in `tests/bench/micro/property-paths.js`;
  fixtures `objects/`, `classes/`; `scripts/diff.sh`; `cargo test`.
- [ ] Gate: laptop, then a-03 (ask): report the 18 programs' geometric mean (T128+T130 vs ebe6055, and T130 vs
  T128), instruction changes, and the micro table.

## Rules that apply
- Dated Progress line after every step; status `in progress` in `tasks/README.md` when starting.
- Root cause first; count before designing; a miss may not cost more than the old path; a miss doesn't repeat the
  inline test; equal instructions = placement.
- Only the standard tests and benchmarks; no zigbee2mqtt. Ask before a-03 runs and long runs. Report numbers plainly
  (geomean of the 18, instructions), don't fix after results without asking.
- Never commit/push without asking each time; no attribution lines.
- At the end: Notes/Result, `tasks/README.md` status and follow-ups (next free F is F446), tick F441, `CLAUDE.md`,
  `docs/design.md`, `docs/root-causes.md` (R3), `tests/test262/HISTORY.md` after a full run.

## State at handoff (2026-10-17)
- `main` = `5a68ca1` + T129 (to be committed). wasmtime 49.0.3. test262 baseline 28,123. Gate base for T130: the T129
  commit (compiler = T128's).

## Progress
- 2026-10-17: handoff written (user: "Keep T128, F441 is next").
- 2026-10-17: started (user: "push and next up"); status `in progress`.

## Notes / Result
- 2026-10-17: count step from existing counts (`out/count-t128-s12`, `out/count-t124`): `get_named` calls / guess hits /
  prototype-absent: raytrace 52.6M / 41.0M (77.9%) / 11.3M; deltablue 45.5M / 20.7M (45.5%) / 24.8M; richards 16.6M /
  0 / 16.6M; earley-boyer 1.6k / 347 / 1.2k. A run-time guess (the site's `G_IC` entry) covers all 61.7M guess hits; a
  build-time guess only a subset. Cost per read (laptop, 10M reads, control subtracted; scratchpad `t130/`): inline
  hit 220 instructions, guess hit through `get_named` 385 (−165 possible). Projection: raytrace ~−10% instructions,
  deltablue ~−5…−10%.
- 2026-10-17: V8: monomorphic LoadIC feedback = (map, field offset), checked and loaded inline in optimized code; ours:
  the site's entry index, validated by `keys[e] == k` (keys unique per object). Proposed (asked): after T110's
  constant slots, inline the `G_IC` guess (`$plain`, `count > e`, `keys[e] == k`, data value); miss → a `get_named`
  variant without the guess; constant slots stay first; micro cases for hit and every miss kind; `.wasm` size
  reported; writes (`set_named`'s guess) left as a follow-up.
- 2026-10-17: user: build it.
- 2026-10-17: built: the constant-key `GetProp` site casts to `$plain`, tries T110's constant slots, then `G_IC[site]` inline (`count > e`, `keys[e] == k`, data value); a plain miss calls the new `get_named_noguess` (= `get_named(cx, false)`), a non-plain value `get_named`. Counters `inline_read.guess_hit`, `inline_read.miss_other`. Per read (laptop, 10M, control subtracted): guess hit 385 → 279 instructions, T110 constant hit 220 → 221. Fixtures objects 21/21, classes 13/13, builtins 76/76.
- 2026-10-17: count run, all 18 programs (`out/count-t130`, outputs = node): `inline_read.guess_hit` raytrace 39,036,930, deltablue 19,744,093, crypto 7,885,586, regexp 3,813,820, splay 1,064,219, regexredux 963,399; `get_named.guess_hit` left: raytrace 1.97M, deltablue 0.96M (non-plain receivers). Fixtures arrays 9/9, commonjs 15/15, modules 14/14; node: `http.js`, `http-server.js` time out (exit 124, 60+ s) with and without T130 (checked with `git stash`): the laptop's environment, not T130.
- 2026-10-17: `cargo test --release` passes; micro case `read guess hit (key in no layout)` added (`dict.r`). Laptop gate vs 7dfe4a5 started (`out/gate-t130`).
- 2026-10-17: user: test262 on a-03 after the gate (queued).
- 2026-10-17: a-03 gate vs 7dfe4a5 (`out/gate-t130-a03.log`, `.tsv`), outputs = node: geomean of the 18 programs
  **0.9969**; `.wasm` **+16.9%** in total (7–24% per program: every constant-key read site, built-ins included, grew
  ~20 instructions); 11 of 18 programs slower in ms (binarytrees +0.2, fannkuchredux +0.5, fasta +1.8, nbody +1.3,
  pidigits +2.9, spectralnorm +1.6, richards +1.9 (instructions +0.80%: its 16.6M prototype reads pay the inline
  `$plain` test), navier-stokes +0.3, splay +0.3, earley-boyer +0.5, regexp +0.4), 7 faster (deltablue −4.9% /
  instructions −4.48%, raytrace −1.5% / −4.79%, crypto −0.9% / −1.32%, mandelbrot −8.4% (0.3 s), knucleotide,
  regexredux, revcomp ≤ 0.7%). Micros: guess hit 25.5 → 17.2 ns, other-slot read 30.0 → 22.8 ns; `read Map own x`
  +1.2 ns (6/0).
- 2026-10-17: user: "regression in all aspects … is there some win?": only deltablue (−4.9%). Code reverted
  (`git checkout` of `lib.rs`, `object.rs`, `runtime.rs`); test262 on a-03 stopped (it was testing the dropped
  code). Kept: the micro case `read guess hit (key in no layout)`. T128's own size cost for comparison: +0.23%
  (`out/gate-t128-v2` on a-03).

## Notes / Result
**Dropped 2026-10-17.** Inline reads from the site's remembered entry (`G_IC[site]`) after T110's constant slots:
the work fell where intended (raytrace −4.8%, deltablue −4.5% instructions) but the code at every read site cost
+16.9% `.wasm` and made 11 of 18 programs slower; geomean 0.9969; richards +0.8% instructions. No code kept.
- Lesson: an inline sequence at every constant-key site (including the self-hosted built-ins) costs size and time
  everywhere; a per-site inline path has to be limited to the sites that need it (user code, no constant slots, not
  method calls), and judged with a `.wasm` size column.
- Possible retry (not started): the limited version above; F441 stays open with these numbers.
