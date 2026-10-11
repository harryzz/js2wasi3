# T109: object layouts decided at compile time per allocation site (F392)

Status: todo | Depends on: T108 | Local (+ a-03: benchmarks, test262, zigbee2mqtt replay)

## Goal
Property access is 31% of js2wasi3's time on the benchmarks (node 1.6%; `docs/performance.md`) and 22.5% of
scriptc's typed workloads. T108's nbody profile (a-03, `~/t108-prof`): `get_named` 32.9%, `set_named` 17.1%,
`box_number` 15.6% (all from `advance`'s object properties). Today an object is T74's dictionary (`F_KEYS`/`F_VALS`
arrays, a hash index past a size) with a per-site entry guess (`G_IC`, T74; writes T105); every read compares the
guessed entry's key, and every number field is a boxed `$number`.

JSC's `StaticPropertyAnalyzer` (`docs/bun-jsc-analysis.md` #1): for each object literal and each constructor, the
properties stored into the new object before it escapes give its layout at compile time. The object is allocated
with that layout; a read/write whose receiver comes from that site checks one constant layout id and loads/stores a
slot. Dictionary mode stays the fallback (a later add, delete, attribute change, `defineProperty`…).

Unlike T83 (shapes, reverted 2026-10-05) there's **no transition search on the common path**: T83's loss on the
zigbee2mqtt replay was `shape_child` (F362: 5.7% self, objects built by the runtime and literal defines whose site
missed). Layouts here are fixed at build time; nothing is searched at run time.

## Idea (to check first, not decided)
- Analysis in `lower` (like `closures.rs`): per allocation site (object literal; `new C` / class constructor body
  `this.x = …` before `this` escapes), the ordered key list. Layout id = site id (or shared by equal key lists).
- Representation: the layout id in the object (a field, or `F_INDEX` holding a `$layout`), the keys still in
  `F_KEYS` so every generic path keeps working unchanged. A layout object stays a valid dictionary object.
- Access: a constant-key site whose layout is known statically or per site at run time (the site remembers the last
  layout id + slot: one `i32` compare, no key compare) → `F_VALS[slot]`. Leaving the layout (add past the list,
  delete, attribute change) clears the id: dictionary from then on, no transitions.
- Later step (decide after the first): unboxed `f64` slots for fields only ever written with numbers (F323), as T107
  did for arrays: nbody's `box_number`.
- Read T83's code (branch `shapes`, 8532edb) for what to reuse (site numbering, `IS_PROTO`, `G_EPOCH` interplay)
  and what made it lose.

## Read first
- `docs/bun-jsc-analysis.md` (#1, `StaticPropertyAnalyzer`), `docs/design.md` (T74, T83–T86, T90, T105 paragraphs),
  `tasks/T83-shapes.md` (Notes / Result), F362, F357 in `tasks/README.md`, `docs/zigbee2mqtt-feasibility.md`
  (T83–T86 replay numbers).
- `crates/backend/src/object.rs`: `get_named`, `set_named`, `append_named`, `define`, `find_own_named`, `G_IC`.
- `crates/lower/src/lib.rs`: `object_literal` (~6875), class constructors (`classes.rs`), `closures.rs` (pattern
  for a per-function analysis).
- Replay tooling: `tasks/T79-files/run-replay.sh` (a-03; see `tasks/T99-*.md` for the last replay comparison).

## Plan
- [ ] Measure (existing data, no new runs): from the benchmark profiles (`~/t102-prof`, `~/t108-prof`) and
  `docs/performance.md`, list which programs' property time is on objects from literals/constructors (layout
  candidates) vs. runtime-built objects, dictionaries, prototype hits. Record the expected ceiling per benchmark.
- [ ] Design note (in this file): layout representation, site analysis, the escape rule, the fallbacks; which
  generic paths need no change.
- [ ] Step 1: object literals with constant keys; constant-key reads/writes check the layout id. Fixture.
- [ ] Step 2: constructors (`this.x = …` in a class/function constructor before `this` escapes). Fixture.
- [ ] Step 3 (if step 1–2 pay): unboxed number slots (F323).
- [ ] Fixtures `tests/fixtures/objects/site-layouts*.js` = node: delete/re-add, `defineProperty` on a layout
  object, getters/setters in literals, `__proto__` in a literal, spread, computed keys, `Object.freeze`/`seal`,
  prototype objects, the same key list from two sites, `for-in`/`Object.keys` order, JSON.stringify.
- [ ] `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`;
  test262 full on a-03 (ask); a-03 benchmarks T108 vs T109 (ask).
- [ ] **Acceptance check: one zigbee2mqtt replay on a-03** (ask first): HEAD (T108) vs T109 on the same recorded
  session, reported as ms CPU per event vs node (`tasks/T79-files/run-replay.sh`; last numbers: T97 20.28 s, 37.01
  ms CPU/event; node 8.66 s). Output must be identical.

## Acceptance
- Every fixture = node; test262 0 regressions; benchmarks: property-heavy ones (nbody, richards, deltablue,
  raytrace, earley-boyer) faster, nothing slower beyond noise (F399).
- zigbee2mqtt replay: **not slower than T108** beyond replay noise (~1%), CPU/event reported vs node. Slower = the
  task isn't kept (T83/T99 precedent); a tie = ask the user whether the code is worth keeping.

## Notes / Result

## Progress
- 2026-10-11: task created after T108 (user: "write T109 with replay as acceptance check"). The benchmark profiles
  justify it without a new zigbee2mqtt measurement up front; the replay is the acceptance check at the end, because
  T83 passed the benchmarks and lost only on the replay. Not started.
