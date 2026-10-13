# T111: number fields of layout objects kept unboxed (F406)

Status: next (handoff written 2026-10-13) | Depends on: T110 (a52761d, paused; its F409 is resolved by T113), T113, T115 | Local (+ a-03: benchmarks, test262, only after asking)

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

## Handoff (2026-10-13, for a new session)
Start here. The state of the code and what is known:
- **Where things stand.** `main` has T109 (layouts per allocation site), T110's inline layout reads/writes
  (`a52761d`; T110 is "paused" only as a task record: its knucleotide regression F409 came from `is_subtype`, which
  T113 removed), T113 (plain objects are a final `$plain`; object tests are exact `br_on_cast` chains, no
  `is_subtype`), T114 (regexes) and T115 (array element reads inline on untyped receivers; check its status in
  `tasks/README.md`: if not done, it is in the working tree or committed after `fd1b5e7`).
- **Why T111 now.** nbody on a-03 at `fd1b5e7` (Progress below): 3.15 s, 19.4× node. `box_number` 17.1%,
  `to_number` 15.4%, `js_add` 14.7% with callees: all from number values in object fields (`b.vx -= dx * mj`,
  `bodies[i].x - bodies[j].x`, `b.x += b.vx * dt`), ~47% together. T115 targets the other 23% (`get`).
- **What a field read/write is today.** A layout object's slot holds a `jsval`; a non-integer number is a
  `$number` struct, so every write allocates and every numeric use unboxes (`to_number`), and `+=` goes through
  `js_add` because the slot might hold a string.
- **The model to copy.** T107 kept arrays of numbers unboxed (`$nums`, `f64` with a HOLE NaN, a mode switch when a
  non-number is written; `docs/design.md` T107 paragraph; `array.rs` `get_num`/`set_num`, `is_num_mode`). V8 does
  the same for object fields ("double fields": a field's representation is part of the hidden class and changes
  when a non-double is stored).
- **First step (no code):** the design note in this file, then ask the user. Questions it must answer: where the
  `f64`s live (a parallel `(array (mut f64))` per layout object, or a per-slot representation flag); how every
  generic reader of `F_VALS` sees them (19 + 12 + 1 uses listed in "Read first"); when a slot stops being a number
  slot (a non-number written: convert that object, or the layout); how T110's inline reads/writes and the
  lowering's types use it (a numeric consumer gets an `f64` straight from the slot, no `to_number`; `+=` on a
  number slot is `f64.add`, no `js_add`). Size the work before starting: which keys of which layouts in the 18
  programs are only ever written numbers (nbody's `Body` fields; others probably none: earlier profiles show
  `box_number` from fields only in nbody).
- **Rules to keep.** "Consult proven implementations first" means the research decides the design; report and ask
  before building (memory: feedback-research-deployment-tradeoffs). Every speed change goes through
  `scripts/gate.sh` on a-03 (ask first); fixtures `scripts/diff.sh`; test262 on a-03 (ask). Write progress here
  every step.

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
  a-03 benchmarks (ask); test262 (ask).

## Acceptance
- Every fixture = node; test262 0 regressions; nbody faster; nothing slower beyond noise.

## Notes / Result

## Progress
- 2026-10-12: task created (user: "make F406"), after T110 was paused (F409).
- 2026-10-12: started on top of T110 (a52761d; user: "on top"). Measure: nbody's `Body` keys (`x y z vx vy vz
  mass`) are only ever written with numbers; its hot writes (`b.vx -= …`) compute an `f64` that is boxed before
  `set`, its hot reads feed arithmetic. Readers of an object's values: `F_VALS` is read directly in ~25 runtime
  functions (`object.rs`: `get_named`, `set_named`, `define`, `define_named`, `grow`, `remove`, `own_get`,
  `lookup_named`, `layout_set`, `layout_define`, `get_length`, `regexp_plain`, `typed_length_snapshot`; `props.rs`:
  `slot`, `set`, `define_property`, `define_accessor`, `get_own_property_descriptor`, `private_access`, `super_set`,
  `ctor_name`; `func.rs` `fn_prototype`; T110's inline sites in `lib.rs`). Precedent: `G_BUILTIN` is a sentinel in
  `vals` that readers already route through `props::value`.
  Design options:
  - A: a parallel `nums: (ref null $nums)` field in `$object` (every object and subtype +1 field), `vals[slot]` a
    `G_NUMSLOT` sentinel when the slot's value is in `nums[slot]`; every reader above checks for it (or routes it
    through `props::value` like `G_BUILTIN`, which then needs the slot); writes of a non-number turn the slot back.
    Numeric reads/writes at constant-key sites (`GetNum`/`SetNum`-like, T110's inline slot) read/write the `f64`.
  - B: mutable boxes owned by the slot, written in place: rejected, a read hands the box out (`p.y = o.x`), so a
    later in-place write would change `p.y`.
  Next: ask the user (A, memory per object, ~25 readers).
- 2026-10-12: JSC cross-check (user asked; oven-sh/WebKit `0c06faa`, `runtime/JSCJSValue.h`, `runtime/IndexingType.h`,
  fetched to the session scratchpad): JSC never boxes a double. A `JSValue` is 64 bits, NaN-boxed: a double is
  stored as its bits + 2^49, an int32 under tag `0xFFFE`, a pointer under `0x0000`. A property slot (inline or
  butterfly) holds that 64-bit value, so writing a double to a field allocates nothing; there is no per-property
  number representation (no `InferredType.h` at that commit). Arrays have a raw-double indexing shape
  (`DoubleShape`, `ArrayWithDouble`: what T107 copied). WasmGC can't NaN-box (references are opaque, `i31` is 31
  bits), so JSC's way isn't open to us; the cell design (refined A) is the substitute. (V8's "double field
  representation" with a mutable box owned by the object and copied on read is the closer match: from memory,
  not checked in source.)
- 2026-10-12: paused before implementation (user: T112 first, the root-cause map and counts).
- 2026-10-12: user: no zigbee2mqtt runs for tests or measurement until asked; replay removed from the plan.
- 2026-10-13: nbody profile on a-03 at `fd1b5e7` (after T110/T113/T114; `~/nbody.data`, the bench-t114 build,
  output = node; 3.15 s, 19.4× node). Self: `advance` 36.5%, `get` 22.9% (`bodies[i]`/`bodies[j]` on the
  untyped parameter `bodies`: a generic element read, 14 per pair), `box_number` 17.1% (field writes),
  `to_number` 15.4% (field values in `-`/`*`), `js_add` 4.1% (14.7% with callees: `+=` on fields). This task
  (fields kept unboxed) would address `box_number` + `to_number` + `js_add` (~47% together, more than the old
  15–20% estimate); the 23% element reads are a separate cause (no inline path for `a[i]` on an untyped
  receiver; F92 mentions it).
