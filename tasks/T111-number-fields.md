# T111: number fields of layout objects kept unboxed (F406)

Status: not started | Depends on: T109 (T110 paused, uncommitted) | Local (+ a-03: benchmarks, test262, zigbee2mqtt replay)

## Goal
A number stored in an object property is a `jsval`: a non-integer becomes a new `$number` box on every write
(`box_number`), and every numeric read unboxes it. nbody's `box_number` is 15.6% self (T108 profile, a-03
`~/t108-prof`), 11.9% of it from `advance` writing `Body` fields (`b.vx -= dx * b2.mass * mag`). T107 did this
for arrays (`$nums`, an unboxed `f64` array with a HOLE NaN): this task does it for object fields of layouts
(T109).

Measured before choosing (T110 Progress, 2026-10-12, existing profiles): `box_number` from object fields is
nbody's only (crypto's 21.9% is `am3`'s locals and parameters, F404; navier-stokes' arrays/locals, F403;
raytrace 1.1% has no layouts, F407; the rest ≤ 0.5%). So the expected gain is nbody (≤ ~15–20%); the rest should
not change. The user chose F406 now (2026-10-12: "make F406").

## Read first
- `tasks/T107-number-arrays.md` (Notes / Result: number mode, HOLE, the generic-path conversions), `docs/design.md`
  (T107, T109 paragraphs), `tasks/T109-site-layouts.md` (design note "design for A"), `tasks/T110-layout-reads.md`
  (inline checks, F409: what broke it), F323, F406, F409 in `tasks/README.md`.
- `crates/backend/src/object.rs` (`$object` fields, `layout_set`, `layout_define`, `define`, `get_named`,
  `set_named`), `crates/backend/src/array.rs` (`$nums`, `get_num`/`set_num`), every reader of `F_VALS`
  (`object.rs` 19, `props.rs` 12, `func.rs` 1 uses).

## Plan
- [ ] Measure first (no new runs where existing data answers it): which layout keys are written only with numbers
  in nbody and the other benchmarks; how many `F_VALS` readers would need to know about number slots.
- [ ] Design note (in this file): where the `f64`s live (a parallel `(array (mut f64))` in the object, or another
  representation), how a generic reader of `F_VALS` sees them (every path must stay correct), when a slot stops
  being a number slot, interplay with T110 (whose inline reads are paused, F409). Ask the user before
  implementing.
- [ ] Implement; fixture `tests/fixtures/objects/number-fields.js` = node (non-number writes into a number slot,
  `delete`, `defineProperty`, getters, `Object.keys`/`JSON.stringify`/spread/`for-in`, `-0`, `NaN`, the HOLE bits).
- [ ] `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`;
  a-03 benchmarks (ask); test262 (ask); zigbee2mqtt replay (ask).

## Acceptance
- Every fixture = node; test262 0 regressions; nbody faster; nothing slower beyond noise.
- zigbee2mqtt replay not slower beyond ~1%; slower = not kept; a tie = ask the user.

## Notes / Result

## Progress
- 2026-10-12: task created (user: "make F406"), after T110 was paused (F409).
