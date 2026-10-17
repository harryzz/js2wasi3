# T130: inline reads of learned layouts (F441)

Status: not started (handoff, 2026-10-17) | Depends on: T110, T128, T129 | Local (+ a-03 after asking)

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

## Notes / Result
