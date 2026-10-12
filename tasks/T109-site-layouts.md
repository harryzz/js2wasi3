# T109: object layouts decided at compile time per allocation site (F392)

Status: done | Depends on: T108 | Local (+ a-03: benchmarks, test262, zigbee2mqtt replay)

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

Unlike T83 (shapes, reverted 2026-10-05: replay 22.18 s vs 22.59 s without, short of the user's ≤ 21.5 s bar
for ~2,000 lines) there's **no transition search on the common path**: T84's replay profile had `shape_child` at
5.7% self (F362: objects built by the runtime and literal defines whose site missed). Layouts here are fixed at
build time; nothing is searched at run time. (Cross-check with JSC, Progress 2026-10-11: JSC itself decides only a
property *count* statically; its layouts are run-time structures with per-site caches.)

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
- [x] Measure (existing data, no new runs): from the benchmark profiles (`~/t102-prof`, `~/t108-prof`) and
  `docs/performance.md`, list which programs' property time is on objects from literals/constructors (layout
  candidates) vs. runtime-built objects, dictionaries, prototype hits. Record the expected ceiling per benchmark.
- [x] Design note (in this file): layout representation, site analysis, the escape rule, the fallbacks; which
  generic paths need no change.
- [x] Step 1: object literals with constant keys (defines by slot; reads unchanged, F405). Fixture.
- [x] Step 2: constructors (`this.x = …` in a class/function constructor before `this` escapes). Fixture.
- [ ] Step 3 (if step 1–2 pay): unboxed number slots (F323): not done, left to F406.
- [x] Fixtures `tests/fixtures/objects/site-layouts*.js` = node: delete/re-add, `defineProperty` on a layout
  object, getters/setters in literals, `__proto__` in a literal, spread, computed keys, `Object.freeze`/`seal`,
  prototype objects, the same key list from two sites, `for-in`/`Object.keys` order, JSON.stringify.
- [x] `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`;
  test262 full on a-03 (ask); a-03 benchmarks T108 vs T109 (ask).
- [x] **Acceptance check: one zigbee2mqtt replay on a-03** (ask first): HEAD (T108) vs T109 on the same recorded
  session, reported as ms CPU per event vs node (`tasks/T79-files/run-replay.sh`; last numbers: T97 20.28 s, 37.01
  ms CPU/event; node 8.66 s). Output must be identical.

## Acceptance
- Every fixture = node; test262 0 regressions; benchmarks: property-heavy ones (nbody, richards, deltablue,
  raytrace, earley-boyer) faster, nothing slower beyond noise (F399).
- zigbee2mqtt replay: **not slower than T108** beyond replay noise (~1%), CPU/event reported vs node. Slower = the
  task isn't kept (T83/T99 precedent); a tie = ask the user whether the code is worth keeping.

## Notes / Result
**Done 2026-10-12.** Object layouts per allocation site (design note in the Progress log, 2026-10-11 "design for A").
- What changed: `closures::Analysis::this_props` (a constructor's ordered `this.k =` keys; classes, and `function`s
  named by some `new X(…)`); `ir::Module::layouts` and the intrinsics `NewObjectSized`, `CreateThisSized`,
  `CreateThisLayout`, `LayoutSet`, `NewObjectLayout`, `LayoutDefine`; `Lowerer::{layout, layout_for, this_layout,
  create_this_for}`, `member_assign` and `object_literal` emit them; ordinary functions with `this` stores get the
  `f$construct` adapter. Backend: one global shared hash index per layout (`object::layout_index`), helpers
  `alloc_sized`, `new_object_sized`, `layout_set`, `new_object_layout`, `layout_define`,
  `func::create_this_{sized,layout}`; `define` keeps or drops a shared index. Objects without a layout are pre-sized
  by the static count (JSC's real `StaticPropertyAnalyzer`; JSC cross-check in Progress).
- Results (a-03, T108 → T109, median of 5): splay −27.4%, binarytrees −26.1%, earley-boyer −23.7%, richards −2.8%,
  knucleotide −1.9%; the rest within ±2% (spectralnorm +2.0, fasta +1.5, deltablue +1.1, regexredux +0.8, crypto
  +0.7, raytrace +0.5); outputs same. zigbee2mqtt replay: 20.53 → 20.29 s steady CPU (−1.2%), 37.03 ms CPU/event,
  node 8.66 s (15.8 ms/event) = 2.34× node; build 252 → 269 s. test262: no changes (28,105, 97.0%; on the first
  build, a superset of the final one's layouts). Fixtures: `objects/site-layouts.js` = node; objects, classes,
  arrays, commonjs, modules, builtins, node all pass.
- Not done / unsupported: reads don't use the layout (F405); number fields still boxed (F406, F323: nbody
  unchanged); derived-class constructors, arrows, fields set through `superConstructor.call(this)` /
  `initialize.apply(this)` get no layout (F407); `layout_set`/`layout_define` are helper calls (F405);
  more than 8 keys fall back.
- Next smallest step: F405 (reads at a site whose receiver has a known layout: one `ref.eq` on `F_INDEX` + slot load,
  inlined), then F406.

## Progress
- 2026-10-11: task created after T108 (user: "write T109 with replay as acceptance check"). The benchmark profiles
  justify it without a new zigbee2mqtt measurement up front; the replay is the acceptance check at the end, because
  T83 passed the benchmarks and lost only on the replay. Not started.
- 2026-10-11: started. Measure step from existing data only: `docs/performance.md` (2026-10-10, e13d22c) puts
  property access at 69–77% of nbody/deltablue/richards/raytrace, 34–39% earley-boyer/fannkuchredux, 20–30%
  binarytrees/knucleotide/crypto. Split by helper and calling JS function (`/tmp/t109-propsplit.py`,
  `/tmp/t109-parents.py` on a-03 over `~/t102-prof` (T101 builds) and `~/t108-prof/nbody` (T107 build); names
  mapped with `~/js2wasi3-head-t101 ir`):
  - nbody (T107 build): `get_named` 32.9%, `set_named` 17.1% (no `find_own_named`: every site hits its guess),
    `get` 12.6% (`bodies[i]`), all from `advance`; fields from `class Body { constructor(…) { this.x = x; … } }`.
  - richards: `get_named` 33.7, `find_own_named` 25.9 (5.2+1.3+1.0 from `set_named`: profile predates T105's write
    guess; ~6 from `get_named` misses; ~3 via `lookup_named`), `set_named` 6.5. 27% in `Scheduler.prototype.schedule`
    (`this.currentTcb` ×8 = constructor fields; `isHeldOrSuspended`/`run` = prototype methods).
  - deltablue: `get_named` 37.6, `find_own_named` 26.9 (about half via `lookup_named`: prototype-chain method
    lookups, `this.size()`, `this.constraintAt(i)`, `c.execute()` over subclasses), `lookup_named` 3.7. Fields set
    through `X.superConstructor.call(this, …)` (inheritance by hand): not a single constructor body.
  - raytrace: `make_arguments` 9.1 + `list_from_array_like` 4.9 + `own_get` 6.6 (Prototype.js `Class.create`:
    `function () { this.initialize.apply(this, arguments) }`); the field stores (`Vector`/`Color` 10%: 
    `set_named`/`append_named`/`find_own_named` via `ordinary_set_named`) are in `initialize`, not a constructor.
  - earley-boyer: `sc_Pair` construction 11.1% (`set_named` 4.9, `append_named` 3.0, `find_own_named` 2.2);
    `.car`/`.cdr` reads ~16% (`get_named`). binarytrees: `TreeNode` construction 8.7% (`append_named` 4.8),
    `itemCheck` reads 10.9%.
  - Not layout work: fannkuchredux/crypto property time is array elements (`get`/`set`), knucleotide is
    `string_eq`/Map keys.
  - The current hit is already "per-site guessed entry, key identity": a layout id saves the `G_IC` load and bounds,
    the key compare and the attribute byte, and allows inlining the access into the caller; the larger gains are
    construction (no `append_named` search, pre-sized) and unboxed number fields (step 3: nbody `box_number` 15.6%).
  - Stale: richards/deltablue/raytrace/earley-boyer/binarytrees profiles predate T105 (write guess, richards −11%)
    and T107; nbody is current. A fresh profile of those five on HEAD is needed for real ceilings (ask).
- 2026-10-11: fresh profiles on a-03 (user OK): `~/t109-prof.sh`, the T108 builds in `~/t108-bench`, profiles in
  `~/t109-prof`. Property-helper self time, split by caller (function names shifted by one vs T101):
  - richards 68.1%: `get_named` 36.7, `find_own_named` 15.4, `set_named` 9.7 (T105 removed the write misses);
    `schedule` 24.0%; the `find_own_named` left is `get_named` own misses → `lookup_named` = method reads on the
    prototype (`currentTcb.run()`), not fields.
  - deltablue 73.7%: unchanged since T101: `find_own_named` 26.9 about half own misses, half `lookup_named`
    (methods through the hand-made prototype chains), `get_named` 37.2.
  - raytrace 57.7%: unchanged: `make_arguments` 8.5 + `own_get` 7.0 + `list_from_array_like` 5.0 (`apply`), field
    stores in `initialize` 10%.
  - earley-boyer 35.2%: `sc_Pair` construction 12.3% (`set_named` 6.4, `append_named` 3.0, `find_own_named` 2.0),
    `.car`/`.cdr` reads ~16%.
  - binarytrees 20.5%: `TreeNode` construction 9.4% (`append_named` 5.0, `define_named` 2.4, `find_own_named`
    2.1), `itemCheck` reads 10.9%.
  - Addressable by layouts (fields stored by a constructor or literal, then read/written at monomorphic sites):
    nbody ~50%, richards ~40%, earley-boyer ~28%, binarytrees ~20%, deltablue only the own-field part (fields set
    via `superConstructor.call(this)`), raytrace little (fields set in `initialize` via `apply`). Method reads
    through prototypes (richards ~4%, deltablue ~13%) are not layout work (T99 tried a prototype-hit cache: lost
    on the replay). Next: design note.
- 2026-10-11: local micro-benchmark (HEAD build, laptop, scratchpad `new.js`, 2M iterations, closures over `let`s
  so absolute numbers include cell costs; node in brackets): loop 10.5 ns [2.0], array read +4.5, `ps[i&1].x` read
  ~15 ns over that (a `get_named` hit + `js_add`) [~0], `{}` 42.5 [11.5], `{x, y}` 113.5 [10], `new E` (empty
  constructor) 93.5 [9], `new P1` 177 [10], `new P2` 206 [10], `new P4` 322 [13.5], `new F2` (function
  constructor) 201 [10.5]. So: the first `this.x =` costs ~84 ns (the keys/vals/attrs arrays made on the first add),
  each further one ~29 ns (`set_named` → T90's add path → `append_named`, growth), a literal field ~35 ns;
  `new` itself ~50 ns over `{}`. Construction is the biggest per-operation gap (20× node); a field-read hit is ~15 ns.
- 2026-10-11: cross-check with JSC (user: "can you make cross check with jsc?"). Read at oven-sh/WebKit `0c06faa`
  (the commit `docs/bun-jsc-analysis.md` used; files fetched to the session scratchpad):
  - `bytecompiler/StaticPropertyAnalyzer.h`: "flow-insensitive static analysis of the **number** of properties
    assigned to an object … understood to be lossy, and it's OK if it turns out to be wrong". It records a count per
    `op_new_object`/`op_create_this` (killed when the register is reused/escapes), nothing else. **JSC does not
    decide layouts at compile time**: F392's wording ("the properties … give its layout") overstates it.
  - `bytecode/ObjectAllocationProfileInlines.h`: the count (+ non-function properties of a non-`Object.prototype`
    prototype; 0 → the default capacity) only sizes the object's inline slots; the object starts with the **empty**
    structure for its prototype (`structureCache().emptyObjectStructureForPrototype`).
  - `llint/LowLevelInterpreter64.asm` `op_put_by_id`: the per-site metadata holds `oldStructureID`,
    `newStructureID`, `offset` and an optional `structureChain` (the prototypes' structure IDs, so no setter or
    read-only key appeared on the chain): an add is "compare structure ID, (walk chain IDs), store new ID, store
    value at offset", no search. `op_get_by_id` modes: Default (structure ID → offset), **ProtoLoad** (base's
    structure ID → a cached slot of the prototype object, validated by watchpoints), ArrayLength, Unset.
  - `runtime/StructureTransitionTable.h`: a miss looks the transition up in a single slot, else a hash map
    (`WeakGCMap` keyed by (key, attributes, kind)), not a list. `runtime/JSGlobalObject.h`: runtime-built objects
    get pre-made structures (`m_iteratorResultObjectStructure`, `m_dataPropertyDescriptorObjectStructure`, …).
  - So JSC's speed on objects = run-time structures + per-site caches (read, add-transition, prototype load) +
    pre-sized allocation from the static count + pre-made structures for runtime objects. That is T83's design
    plus exactly the things F362 found missing in T83 (runtime-built objects, literal sites missing, list search).
  - Correction (`docs/design.md`): T83–T86 didn't lose on the replay: 22.18 s vs 22.59 s without (−1.8%), reverted
    because short of the user's bar (≤ 21.5 s) for ~2,000 lines. My earlier "T83 lost" in this file is wrong.
- 2026-10-11: decision (user: "Yes" to step 0 first): JSC's real static technique first: allocation sized by the
  static property count, then decide between A (static shared layouts) and D (T83 + JSC's missing parts).
- 2026-10-11: step 0 implemented. `closures::Analysis::this_props` (per function scope, a class's scope for its
  constructor: distinct keys of `this.k = …` in it and its arrows; one pass over the nodes); IR
  `NewObjectSized(n)` / `CreateThisSized(new_target, n)`; backend `alloc_object_sized` (keys/vals/attrs of length
  `n`), `new_object_sized`, `create_this_sized`; `n` capped at 8 (`object::SMALL`: past it `grow` builds the hash
  index, a bigger pre-size would skip it). Object literals: count of non-spread, non-`__proto__` properties.
  Ordinary functions with `this` stores now get T38's `f$construct` adapter (`FnKind::OrdinaryWith`; it passes
  `new_target` only when the function takes it), so `new` doesn't go through `construct_ordinary`'s generic
  `create_this`. Classes: `class_adapters`.
  Local micro-benchmark (laptop, min of 5 alternating runs, ns per object, old → new): `{x, y}` 98 → 83, `new P1`
  131 → 113.5, `new P2` 191.5 → 176, `new P4` 307.5 → 293.5, `new F2` 203.5 → 196.5: ~15 ns per object (7–15%).
  The arrays weren't the main cost: each `this.k = v` add is still ~30–60 ns (`set_named`: the T105 guess fails,
  `find_own_named` scans for `k`, T90's prototype check, `append_named`). `scripts/diff.sh` objects/ 14/14,
  classes/ 12/12, `new-target.js`, `semantics.js` pass.
- 2026-10-11: design for A (user: "Yes"). A **layout** = a constructor's `this.k = …` keys in first-appearance
  order (1–8 keys; `closures::Analysis`), registered in `ir::Module::layouts`. Marker: the object's `F_INDEX`
  holds the layout's **shared, pre-built hash index** (a global `(ref $i32s)`, `array.new_fixed` of the keys'
  build-time hashes, the probe order `find` uses). No new field and no `$object` subtype (a subtype would send
  these objects through wasmtime's `is_subtype` at F376's ~69 `ref.cast (ref $object)` sites).
  - Invariant: `index == L.index` ⇒ `keys[0..count]` is `L.keys[0..count]` (deleted entries are null).
    `find` already checks `keys[e]` after a probe, so a shared index for keys not yet added answers "absent".
  - Shared indexes have < 64 slots, own ones ≥ 64 (`reindex` minimum), so "shared" = `array.len < 64`.
    Only `define` (adds), `remove` (delete) and `grow` mutate keys, and only `define`/`grow`/`reindex` write
    indexes: `define` of key k at entry `count` keeps the shared index iff it maps k to `count` (k is the next
    layout key), else drops it (null below `SMALL`, else `reindex`); `grow` drops it already.
  - `new`: `CreateThisLayout(L)`: `alloc` with `n` slots and `index = L.index`.
  - `this.k = v` in the constructor (not in arrows, not derived classes): `LayoutSet{L, slot}(o, k, v)` →
    helper `layout_set`: `o` exactly `$object` and `index == L.index`: `slot == count` (add: not `NO_EXTEND`,
    T90's per-site prototype check) → `keys[slot] = k`, `vals[slot] = v`, `count = slot + 1`; `slot < count` and
    `keys[slot] == k` and attrs allow it → `vals[slot] = v`; anything else → `set_named` (same site number).
  - Reads unchanged (T74/T105 guesses; misses probe the shared index). Literals: later, same marker.
- 2026-10-11: A implemented (constructors). Analysis: `this_props` now the ordered keys, only for what `new`
  can construct (a class's constructor via `ctor_class`, or a `function` that isn't a method/accessor/async/
  generator). IR: `Module::layouts`, `CreateThisLayout(l)`, `LayoutSet { layout, slot }`. Lowering:
  `Lowerer::layout` (1–8 keys, not `length`/`__proto__`), `this_layout` (a class's constructor is lowered with
  the class's scope: `cx.class`; derived classes and arrows: none), `create_this_for`; `member_assign` emits
  `LayoutSet` for `this.k = v`. Backend: one immutable `(ref $i32s)` global per layout (`object::layout_index`,
  `ConstExpr` `array.new_fixed`) between the shared globals and the constants (`const_base` moved);
  `func::create_this_layout`; `object::layout_set` (add at `slot == count`: `NO_EXTEND`, T90's per-site prototype
  check, `layout_changed`; overwrite: `keys[slot] == k`, attributes; else `set_named`); `define` keeps a shared
  index only when it already maps `k` to the new entry, else drops it (null, or `reindex` past SMALL).
  Bugs on the way: `layout_set`'s locals declared in the wrong order (validation error); class constructors
  first got no `LayoutSet` (lowered with the class's scope) but their objects had the layout, so every add paid
  `define`'s shared-index probe: `new P4` 296 → 372 ns until fixed.
  Micro-benchmark (laptop, min of 5, ns, step 0 → A; before T109 in brackets): `new P2` 177.5 → 135.5 [191.5],
  `new P4` 296.5 → 179.5 [307.5], `new F2` 168 → 135 [203.5], `new P1` 114.5 → 121 [131] (noise).
  Fixture `tests/fixtures/objects/site-layouts.js` (conditional/out-of-order stores, adds/deletes/re-adds,
  read-only and accessor keys, `this` escaping to code that adds/deletes keys, `preventExtensions`/`freeze` in
  the constructor, prototype setters/read-only keys/replaced prototypes, `call` on other objects,
  `Reflect.construct`, derived classes, > 8 keys, `length`, methods storing on `this`, computed same-content keys,
  for-in/spread/assign): = node. `scripts/diff.sh` objects 15/15, classes 12/12, arrays 8/8, commonjs 15/15,
  modules 14/14.
- 2026-10-11: literals. An object literal whose properties are all plain `key: value` (methods too) with
  distinct named keys (not computed, not `__proto__`/`length`/digits), 1–8 of them, gets a layout
  (`Lowerer::layout_for`: one per distinct key list, shared with constructors): `NewObjectLayout(l)`, and its
  `DefineProp`s become `LayoutDefine { layout, slot }` (`object::layout_define`: index and `count == slot` →
  store; else `DefineNamed`). Fixture extended (layout literals changed afterwards, duplicates, `__proto__`,
  computed, accessors, 9 keys, digit keys, a literal and a constructor with the same keys, freeze): = node.
  `builtins/` 73/73 (1 skipped) ran while the binary was rebuilt mid-run: to be rerun with the final build.
- 2026-10-11: micro-benchmark with literal layouts (laptop idle, min of 5, ns, step 0 → A): `{x, y}` 83 → 59,
  `new P1` 115.5 → 104, `new P2` 174.5 → 138.5, `new P4` 298 → 185, `new F2` 186 → 134; `new E`, reads unchanged.
  `node/` 42/42 (also during the mid-run rebuild). Full rerun of objects, classes, arrays, commonjs, modules,
  builtins, node on the final build started.
- 2026-10-11: final build: `scripts/diff.sh` objects 15/15, classes 12/12, arrays 8/8, commonjs 15/15, modules 14/14, builtins 73/73 (1 skipped), node 42/42. Next: a-03 benchmarks + test262, then the zigbee2mqtt replay (ask).
- 2026-10-11: a-03 run started (user OK): `~/t109-all.sh` → `~/t109-all.log`: build (`~/js2wasi3-t109`), `~/t109-bench.sh` (T108 vs T109 → `~/t109-bench.log`), full test262 `--update` (`~/t262-t109.log`), zigbee2mqtt builds `p-t108`/`p-t109` (.wasm/.cwasm in `~/js2wasi3-prod-run`), replays of `rec-20261004-1941` alternating t108, t109, t108, t109 (`t109-replay-<v>-<i>/result.txt`).
- 2026-10-11: a-03 benchmarks so far (T108 vs T109 first build): **binarytrees 9,317 → 6,884 ms (−26.1%)**,
  **splay 3,523 → 2,558 (−27.4%)** (GC-bound: no `grow` copies, less garbage), raytrace +3.0%, the rest −1.1…+2.0%.
  raytrace: its Prototype.js `initialize: function (x, y, z) { this.x = … }` methods counted as constructors
  (58 `layout_set` sites), but their `this` comes from `Class.create`'s `this.initialize.apply(this, arguments)`,
  so every store missed (`layout_set` → `set_named`). Fix: a `function` gets a layout only if some
  `new X(…)` names it (its own name, or the variable/property/key it's assigned to; classes always): raytrace
  6 `layout_set` left, richards 35, deltablue 21, earley-boyer 25; binarytrees uses a literal layout
  (`TreeNode` returns `{left, right}`). Fixture = node. Final build made in `~/js2wasi3-t109src` and swapped in as
  `~/js2wasi3-t109` during the chain (before earley-boyer/regexp builds and the zigbee2mqtt build; the running
  test262 uses the first build, which uses layouts in a superset of functions).
- 2026-10-12: a-03 chain done (`~/t109-all.log`). test262 (first T109 build): 28105 total, 97.0%, "No changes since
  the baseline". zigbee2mqtt (final build): js2wasi3 build 252 s (T108) vs 269 s (T109); replay `rec-20261004-1941`,
  alternating, steady CPU / ms CPU per event: T108 20.46 s / 37.34, 20.60 s / 37.59; T109 20.33 s / 37.10,
  20.25 s / 36.95. Mean 20.53 → 20.29 s (−1.2%), 37.47 → 37.03 ms/event; node 8.66 s (15.8 ms/event), so 2.34× node.
  Same events/matches in all runs (139 exact, 1 unmatched as before). Acceptance (not slower than T108) met.
- 2026-10-12: first vs final T109 `.wasm` compared on a-03: same for mandelbrot, spectralnorm, fannkuchredux, nbody,
  binarytrees, fasta, pidigits (their first-run numbers stand); different for knucleotide, regexredux, revcomp and all
  8 octane (earley-boyer/regexp had also run on the first build). Rerunning those 11 (`~/t109-bench2.sh` →
  `~/t109-bench2.log`, T108 vs final T109, 5 interleaved rounds, median).
- 2026-10-12: first rerun attempt ran nothing (`nohup` from ssh had no `wasmtime` on PATH); rerun with `bash -lc`.
  T108 vs final T109, median of 5 interleaved (ms): knucleotide 5226 → 5126 (−1.9%), regexredux 3021 → 3046 (+0.8%),
  revcomp 516 → 516, richards 4522 → 4397 (−2.8%), deltablue 3748 → 3788 (+1.1%), raytrace 7270 → 7308 (+0.5%;
  was +3.0% on the first build), navier-stokes 1521 → 1518, crypto 5295 → 5331 (+0.7%), splay 3613 → 2622 (−27.4%),
  earley-boyer 10230 → 7805 (−23.7%), regexp 21855 → 21816. Outputs same. With the unchanged first-run figures
  (binarytrees −26.1%, mandelbrot −1.1, spectralnorm +2.0, fannkuch −0.9, nbody −0.7, fasta +1.5, pidigits +0.4).
