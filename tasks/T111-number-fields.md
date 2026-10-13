# T111: number fields of layout objects kept unboxed (F406)

Status: done | Depends on: T110 (a52761d, paused; its F409 is resolved by T113), T113, T115 | Local (+ a-03: benchmarks, test262, only after asking)

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

## Design note (2026-10-13, for the user's decision)
**What proven engines ship** (checked in source this time, v8/v8 `main`, 2026-10-13; JSC from the 2026-10-12 entry):
- JSC, SpiderMonkey, Hermes: NaN-boxing, a double is a 64-bit value in the slot: nothing to copy (not open to
  WasmGC: references are opaque, `i31` is 31 bits).
- **V8** (the only one that boxes doubles, as we do): a field's representation (`Representation::kDouble`,
  `property-details.h`) is part of the map; the field holds a **box owned by the object**. A write stores the bits
  into that box in place (`JSObject::WriteToField`, `js-objects-inl.h`: `set_value_as_bits`, no allocation); a
  generic read **copies** it into a fresh HeapNumber (`Object::WrapForRead`, `objects.cc`); optimized code reads
  the `f64` straight from the box. A non-double store turns the field to `kTagged` in place
  (`MightCauseMapDeprecation`: "Boxed double to tagged transition is always done in-place"); Smi → Double needs a
  box (map deprecation).

**Proposal: V8's owned box, as a cell type** (the 2026-10-12 option B, fixed by V8's rule that the box is never
handed out):
1. A final type `$numcell` = `(struct (mut f64))`. A slot of an object's `vals` may hold one: the property's value
   is the number in it.
2. Cells are made by the stores that know them (`layout_define`, `layout_set`, `set_named`, `define`): a
   non-integer number stored into a data slot gets a cell (or, if the slot has one, its bits are written in
   place). Integers stay `i31` until the first non-integer (V8: Smi → Double); a slot with a cell keeps it for
   integers too (no flip-flop).
3. A non-number store replaces the cell (the slot is tagged again). So a writer that doesn't know cells stays
   correct: it overwrites.
4. Every reader that hands out a slot's value copies a cell out (`box_inline`: an `i31` or a new `$number`, as
   `WrapForRead`): the ~25 runtime readers of `F_VALS` (listed in Progress 2026-10-12), anything that copies `vals`
   between objects, and T110's inline read (a third test on a hit, `ref.test $numcell`, next to the `$accessor` /
   `G_BUILTIN` tests; a shared non-final supertype would bring back `is_subtype`, R1).
5. The fast paths that make it pay (backend peepholes + one lowering case, as T107 did for arrays):
   - a numeric read `jsval->f64(GetProp(o, "k"))` at a layout key: inline slot hit with a cell → `struct.get`,
     no allocation, no `to_number`; else as today + `to_number`;
   - `o.k = <f64>` at a layout key: a hit on a writable data slot with a cell → `struct.set`; else box + `set_named`
     (which makes the cell);
   - the statement `o.k += <number>` (as T107's `elem_add_stmt`): read as number-or-HOLE, `f64.add`, the `f64`
     write; a non-number → `js_add` as today.

**Rejected:** a parallel `nums` array per object (option A: +1 field on every object and subtype, two stores per
write, and every reader still checks a sentinel: the same reader work plus memory); a shared box written in place
(`p.y = o.x` would alias); NaN-boxing (impossible in WasmGC).

**Sizing (existing data, no new runs).** Only nbody stores non-integers in object fields on a hot path (`Body`:
`x y z vx vy vz mass`); the other 17 programs' `box_number` from fields is ≤ 0.5%. nbody at `fd1b5e7` (a-03
profile): `box_number` 17.1% + `to_number` 15.4% + `js_add` 14.7% with callees (field `+=`), ~47%, a larger share
after T115. Memory: the same (a cell is the size of a `$number`; one allocation at the first non-integer store
instead of one per store).

**Risks / costs to gate:** every inline read hit pays one more test (`property-paths` "read hit", richards and
deltablue read through T110's sites); generic readers +1 test; a reader that misses the copy would leak a cell
(wrong `typeof`, printing, `===`): the fixture `tests/fixtures/objects/number-fields.js` covers every reader
kind (keys/entries/values, JSON, spread, `Object.assign`, `for-in`, descriptors, `defineProperty`, freeze,
getters/setters replacing a cell slot, `delete`, `-0`, `NaN`, classes, prototypes).

## Read first
- `tasks/T107-number-arrays.md` (Notes / Result: number mode, HOLE, the generic-path conversions), `docs/design.md`
  (T107, T109 paragraphs), `tasks/T109-site-layouts.md` (design note "design for A"), `tasks/T110-layout-reads.md`
  (inline checks, F409: what broke it), F323, F406, F409 in `tasks/README.md`.
- `crates/backend/src/object.rs` (`$object` fields, `layout_set`, `layout_define`, `define`, `get_named`,
  `set_named`), `crates/backend/src/array.rs` (`$nums`, `get_num`/`set_num`), every reader of `F_VALS`
  (`object.rs` 19, `props.rs` 12, `func.rs` 1 uses).

## Plan
- [x] Measure first (no new runs where existing data answers it): which layout keys are written only with numbers
  in nbody and the other benchmarks; how many `F_VALS` readers would need to know about number slots.
- [x] Design note (in this file): where the `f64`s live (a parallel `(array (mut f64))` in the object, or another
  representation), how a generic reader of `F_VALS` sees them (every path must stay correct), when a slot stops
  being a number slot, interplay with T110 (whose inline reads are paused, F409). Ask the user before
  implementing.
- [x] Implement; fixture `tests/fixtures/objects/number-fields.js` = node (non-number writes into a number slot,
  `delete`, `defineProperty`, getters, `Object.keys`/`JSON.stringify`/spread/`for-in`, `-0`, `NaN`, the HOLE bits).
- [x] `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`;
  a-03 benchmarks (ask); test262 (ask).

## Acceptance
- Every fixture = node; test262 0 regressions; nbody faster; nothing slower beyond noise.

## Notes / Result
- **What changed** (V8's double fields, adapted; the design note above, with two changes the gates forced):
  - A **number cell** is an `$accessor` (new third field `num: mut f64`, `runtime::F_ACCESSOR_NUM`) whose `get`
    is the `i31` 0 (`runtime::NUMCELL_MARK`; a getter is a function, `undefined` or `G_BUILTIN`), held in a data
    slot (attributes: data) of a `$plain` object. Sharing `$accessor` keeps the read paths' slot tests as they
    were (an accessor already goes to `property_value`); a separate `$numcell` type cost +2.5% instructions in
    richards/deltablue (one more test per read hit).
  - **Made** only by the backend's numeric write at a layout key (`num_slot_write`, on `SetProp(o, "k",
    f64->jsval(x))` with a `$plain` receiver, a writable data slot): a cell is written in place; a slot without
    one gets an `i31` for an integer (V8's Smi fields: integer cells cost richards +8.8% instructions, its generic
    reads missed inline) and a new cell otherwise. A writer that doesn't know cells overwrites them (correct).
  - **Read**: `num_slot_read` (`+o.k` = `jsval->f64(GetProp(o, "k"))`, and `ElemNum(o, "k")`) takes the `f64`
    from a cell, an `i31` or a `$number` without a call; anything else as before. Every reader that hands a slot
    out copies a cell (`runtime::numcell_out`: `property_value`, `own_get`, new `cell_out` in `defineProperty`'s
    same-value check and `getOwnPropertyDescriptor`). `own_get_raw` (no copy) serves `lookup` and `has_own`,
    whose callers test for null or call `property_value` (the copy in `own_get` cost `o[k]`/`k in o` 9
    instructions per call).
  - Lowering: the statement `o.k += x` goes through T107's `elem_add_stmt` (read by `ElemNum`, `f64.add`, the
    numeric write).
  - `build --count`: `num_read.*`, `num_write.*` counters.
- **Result** (a-03, `out/gate-t111b-a03.log`, vs `40ba0c5`): nbody 2,739 → 2,168 ms (−20.8%, instructions
  −24.1%); mandelbrot +3.2%, pidigits +3.0% at equal instructions (placement, F414); the other 15 the same;
  outputs = node. Micro: `o[k]` plain/array +2/+3 ns and `k in o` +2…+5 ns at equal instructions (the loops count
  exactly the base's instructions on the laptop): accepted by the user as placement. Laptop: nbody −23.6%.
- **test262** (a-03, `out/t262-t111.log`, 1,478 s): no changes since the baseline (28,105 pass, 97.0%).
- **Tests**: `tests/fixtures/objects/number-fields.js` = node (every reader kind, copies, in-place writes,
  -0/NaN/Infinity, non-number writes, read-only/frozen/`defineProperty` same value, accessors replacing a cell
  slot, `delete`, prototypes, classes, literals, array/function receivers; `--count`: 30 cells made, 1,013
  in-place writes); fixtures `objects/` `classes/` `builtins/` `arrays/` `commonjs/` `modules/` `node/` pass
  (`node/fetch.js`, `fs-streams.js` time out only under back-to-back load).
- **Commands**: `scripts/gate.sh -r 3 … 40ba0c5` (laptop, three times), `scripts/gate.sh -r 5 -o out/gate-t111b
  40ba0c5` (a-03), `build --count` on the fixture, nbody, richards; `perf stat -e instructions:u` on scratch loops
  and the micro programs; `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups)**: cells only at layout keys of `$plain` objects written by a numeric expression (F423);
  a generic read of a non-integer cell allocates its copy (F424); placement swings (F414).
- **Next smallest step**: F404 (crypto `am3`'s boxed parameters/locals) or F420 (typed-array element reads inline).

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
- 2026-10-13: resumed in a new session after T115 (`40ba0c5`). Step 1: design note (research + measure), then ask.
- 2026-10-13: research: V8's double fields checked in source (owned box, written in place, copied on read, `WriteToField`/`WrapForRead`); design note written (cell type `$numcell`, V8's rule). Next: ask the user.
- 2026-10-13: user: build the cell design (`$numcell`, V8's owned box).
- 2026-10-13: implemented: `$numcell` (type 34; `$bigint` 35); copies in `property_value`, `own_get`, new `cell_out` (`defineProperty` same-value check, `getOwnPropertyDescriptor`); `get_named` guess hit and T110's inline read route a cell to `property_value`; backend: numeric read `jsval->f64(GetProp(o, k))`, write `SetProp(o, k, f64->jsval(x))` and `ElemNum(o, "k")` inline at layout keys (`num_slot_read`/`num_slot_write`); lowering: `o.k += x` statements through `elem_add_stmt`. Cells are made only by the inline numeric write (on `$plain` receivers). Builds.
- 2026-10-13: `tests/fixtures/objects/number-fields.js` (reads of every kind, copies, in-place writes, -0/NaN/Infinity,
  non-number writes, read-only/frozen/defineProperty same value, accessors replacing a slot, delete, prototypes,
  classes, literals, array/function receivers) = node; `--count`: 32 cells made, 1,013 in-place writes, 1,017 cell
  reads (counters `num_read.*`/`num_write.*` added to `COUNTS`). Fixtures: `objects/` 17, `classes/` 12,
  `builtins/` 73 (1 skipped), `arrays/` 8, `commonjs/` 15, `modules/` 14, `node/` 42 pass (`fs-streams.js` failed
  once at 94 s under the back-to-back load, passes alone in 28.8 s). Patched nbody 5M: output = node. Laptop gate
  vs `40ba0c5` running (`out/gate-t111-laptop.log`).
- 2026-10-13: laptop gate vs `40ba0c5` (`out/gate-t111-laptop.log`): nbody −25.9% (instr −27.5%); added work: richards +8.8% instr, deltablue +2.7%, earley-boyer +1.9%, raytrace +1.8% (micro times drifted again: `typeof v` on primitives slower). Cause (`--count`, richards): integer fields written numerically got cells (657k), then generic reads missed inline (3.7M). Fix (V8's Smi fields): a slot without a cell gets an `i31` for an integer, a cell only for a non-integer: richards 0 cells, inline misses 617k; nbody still 30 cells, 7.5M in-place writes, 0 inline misses. Fixture passes. Re-gate (laptop) on nbody, richards, deltablue, raytrace, earley-boyer, binarytrees, knucleotide.
- 2026-10-13: laptop re-gate (`out/gate-t111b-laptop.log`): nbody −16.4% (instr −27.6%); richards +2.9% instr, deltablue +2.5%, earley-boyer +1.9%, raytrace +1.8%, binarytrees/knucleotide +0.7%: the extra `ref.test $numcell` on every read hit. Changed the representation: a number cell is now an `$accessor` (new field `num: f64`) whose `get` is the `i31` 0 (`NUMCELL_MARK`; getters are functions/undefined/`G_BUILTIN`); `$numcell` removed. Read paths already send an `$accessor` to `property_value`, so hits test nothing new; `property_value`/`own_get`/`cell_out` copy a cell out; the numeric write knows a data slot (attributes) holding an `$accessor` is a cell. Fixture = node, validates, counts as before; `objects/` `classes/` `builtins/` `arrays/` pass. Re-gate (laptop) on the same 7.
- 2026-10-13: laptop re-gate (`out/gate-t111c-laptop.log`): nbody −23.6% (instr −24.1%); richards/deltablue/earley-boyer/binarytrees/knucleotide instr ±0.17%, raytrace +0.30%; time deltablue +3.1%, knucleotide +2.4% at equal instructions (placement/noise). Fixtures `commonjs/` 15, `modules/` 14, `node/` 42 pass (`fetch.js`/`fs-streams.js` timed out under load, pass alone). Next: a-03 gate + test262 (ask).
- 2026-10-13: user: a-03 gate, then test262 if it passes (`out/gate-t111-a03.log`).
- 2026-10-13: a-03 gate vs `40ba0c5` (`out/gate-t111-a03.log`): nbody 2,740 → 2,162 ms (−21.1%, instr −24.1%),
  pidigits +2.8% at equal instructions (placement, F414), the other 16 the same, outputs = node. Micro slower:
  `o[k]` computed key +3…+4 ns (all kinds), `k in o` +3…+6, `hasOwnProperty` array +5: `own_get`'s cell test
  (9 instructions per call, measured with a scratch loop + `perf stat`). Fix: `own_get_raw` (no copy) for
  `lookup` and `has_own`, whose callers test for null or call `property_value`. Now the scratch loops count
  exactly the base's instructions; whole micro programs: object-kinds equal, property-paths −0.16%, regex
  +0.08%, primitives +0.13%. Fixtures `objects/` `classes/` `builtins/` `arrays/` pass.
- 2026-10-13: user: re-gate on a-03 (`out/gate-t111b-a03.log`), then test262 if it passes.
- 2026-10-13: the re-gate stopped at once: `out/gate-t115/base-src` refused the checkout ("local changes"): `rsync --delete` replaces a-03's `.git/worktrees` metadata with the laptop's, so a reused worktree's index no longer matches (probably also the "uncommitted edits" seen in `out/gate/base-src` during T115). Restarted with a fresh `-o out/gate-t111b`.
- 2026-10-13: a-03 re-gate vs `40ba0c5` (`out/gate-t111b-a03.log`): nbody 2,739 → 2,168 ms (−20.8%, instr −24.1%);
  mandelbrot +3.2%, pidigits +3.0% at equal instructions (placement, F414); the other 15 the same; outputs = node.
  Micro marked slower: `o[k]` plain/array +2/+3 ns, `k in o` plain/array/Map +3/+5/+2 (their loops count exactly
  the base's instructions on the laptop: placement); regex ±3% both ways; `set_named` write misses −3…−3.5 ns.
  User: accepted as placement; full test262 on a-03 (`out/t262-t111.log`).
- 2026-10-13: full test262 on a-03 (`out/t262-t111.log`, 1,478 s): no changes since the baseline. Done; docs updated.
