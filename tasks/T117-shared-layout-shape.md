# T117: layout objects share their keys and attributes (V8's shared descriptor arrays)

Status: done | Depends on: T109, T113, T116 | Local (+ a-03 only after asking)

## Goal
Every object costs 4 GC allocations: the `$plain` struct and its `keys`, `vals`, `attrs` arrays
(`object::alloc`/`alloc_sized`). For a layout object (T109) `keys` and `attrs` are the same in every object of the
layout. T116 measured wasmtime's copying GC at 4–8× V8's GC time (binarytrees 51% GC, splay 73%, earley-boyer 26%);
the engine isn't ours to change, so fewer and smaller objects are worth more on wasmtime than on V8.

Existing counts (`out/count/counts.tsv`, 2026-10-12): binarytrees 30.0M `layout_define.fast`, earley-boyer 22.7M
`layout_set.add`, splay 1.7M + 115k, deltablue 718k, richards 40k; `define.shared_dropped` 0 everywhere: hot objects
keep their layout.

## Research (2026-10-13)
- **V8** (source, v8/v8 `main`, `src/objects/map.h`): a map's `instance_descriptors` (keys, attributes, field
  locations) is a `DescriptorArray` **shared** by the maps of a transition chain; each map uses its first
  `number_of_own_descriptors` entries. Objects hold only values (in-object or a backing store). An object that
  leaves the fast shape is **normalized** to its own dictionary (`JSObject::NormalizeProperties`, `js-objects.cc`).
- **JSC** (known design, not re-read): `Structure` holds the property table, shared by all objects of the
  structure; objects hold values (inline / butterfly); a deviating object gets its own dictionary structure.
- Ours already has half of this: T109's shared, pre-built hash index in `F_INDEX` with the invariant "index == L's
  ⇒ `keys[0..count]` = L's first keys" (= V8's own-descriptor count over a shared array).

## Design note (for the user's decision)
1. Per layout L, two immutable globals next to its shared index: `keys` (L's n keys) and `attrs` (n default bytes).
   `CreateThisLayout` and literal allocation give the object these instead of new arrays: **2 allocations per
   object instead of 4** (struct + `vals`), and less live data for the copying collector.
2. One invariant, as today: `F_INDEX` is L's shared index ⇒ `keys` and `attrs` are L's shared arrays.
3. The layout fast paths need no shape write: `layout_set`'s add at `slot == count` with L's key and
   `layout_define` only bump `count` and write `vals` (the shared `keys[slot]` is already the key, `attrs` the
   default).
4. **Every other write to `keys`/`attrs` first unshares** (V8's normalization): `own_shape(o)` copies `keys`/`attrs`
   (and drops the shared index, as `define` already does when it can't keep it). The writers (10 functions):
   `define`, `define_named`, `layout_set`/`layout_define` fallbacks, `remove`, `grow`, `define_property`,
   `integrity` (freeze/seal), `set_attrs`, `watch_mark`. One test before each (F_INDEX shared?), on paths that
   are rare for layout objects.
5. Readers don't change (they read `keys[0..count]` and `attrs[e]`); T110's inline slot checks still compare
   `keys[slot]`.

Risks: a writer that misses `own_shape` would change every object of the layout (all share the array): the audit
above plus a fixture (`tests/fixtures/objects/shared-shape.js`: add, delete, defineProperty, freeze/seal,
preventExtensions, accessors, `__proto__`, many keys/grow, watchpoint keys on prototypes, two objects of one
layout diverging). Gate: binarytrees, splay, earley-boyer faster; nothing slower (`scripts/gate.sh`).

## Plan
- [x] User decision on the design note.
- [x] Implement; fixture = node; `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`.
- [x] Gate (laptop, then a-03 after asking); test262 (ask).

## Notes / Result
- **What changed**: per layout two more globals next to its shared index: `keys` (`(ref null $keys)`, built at the
  end of the start function from the key constants) and `attrs` (immutable, all default). `alloc_object_layout`
  allocates only the struct and `vals`; `new_object_layout`/`create_this_layout` pass the shared arrays;
  `layout_set`/`layout_define` no longer write keys. Invariant (as T109's): shared index ⇒ shared keys/attrs.
  `own_shape` + `object::unshare` copy them before a write (`define` — which grows a full object directly —,
  `remove`, `watch_mark`, `define_property`, `integrity`); `find_own`'s index path requires `e < count` (the
  shared keys are prefilled past `count`).
- **Result** (a-03, `out/gate-t117-a03.log`, vs `e726771`): earley-boyer −22.7% (instructions −20.8%), binarytrees
  −5.1%, object creation −9…−20% (`new P` 152 → 138.5 ns, literal 86.5 → 69.5 ns); outputs = node. **splay +65%**:
  not added work (all counters equal; 5% fewer instructions without collections) but wasmtime's grow-or-collect
  rule (`should_collect_first`: collect while live < one semi-space): the smaller live set sits just under a
  semi-space and collects constantly (F304). Laptop over all 18 at the 16 MiB reserve: time 0.998× base, mean peak
  RSS 72 vs 87 MB. Larger reserves measured (64/128/256 MiB: 0.978/0.992/0.962×, 142/243/447 MB; splay leaves the
  trap only at 256); user: keep 16 MiB.
- **test262** (a-03, `out/t262-t117.log`, 1,489 s): no changes since the baseline (28,105 pass, 97.0%).
- **Tests**: `tests/fixtures/objects/shared-shape.js` = node; fixtures `objects/` `classes/` `builtins/` `arrays/`
  `commonjs/` `modules/` `node/` pass.
- **Not done**: keys added right after construction (splay's `node.left`/`right`) aren't in the layout, so such
  objects grow at their first extra key (F427); the `e < count` check costs index lookups ~9 instructions
  (`hasOwnProperty` on a function +0.4%).
- **Next smallest step**: F427, or F404/F420/F422.

## Progress
- 2026-10-13: created (user chose it after T116); research (V8 `map.h` descriptor sharing, normalization) and
  design note written; existing counts reused. Next: the user's decision.
- 2026-10-13: user: build it. Reading the layout code.
- 2026-10-13: implemented: per layout two more globals (`keys`: `(ref null $keys)` filled at the end of the start
  function from the key constants; `attrs`: immutable `array.new_default`), `alloc_object_layout` (struct + `vals`
  only), `new_object_layout`/`create_this_layout` take them; `layout_set`/`layout_define` no longer write keys;
  `own_shape` + `object::unshare` before writes in `define`, `remove`, `watch_mark`, `define_property`,
  `integrity` (`define_named`'s attribute write on an existing entry stores the same 0; its appends go through
  `define`). First run: 13 fixtures failed: `find_own`'s index path matched a key past `count` (the shared keys
  are prefilled; own arrays had null there), so the first `layout_set` fallback "found" the key and never bumped
  `count`. Fix: index hits need `e < count`. Then `objects/` 17, `classes/` 12, `builtins/` 73 (1 skipped),
  `arrays/` 8 pass; new `tests/fixtures/objects/shared-shape.js` (every shape change on one object, its sibling
  and literal sibling checked; objects seen mid-constructor; out-of-order keys; prototypes; copies) = node.
- 2026-10-13: laptop gate vs `e726771` (`out/gate-t117-laptop.log`, 6 programs): earley-boyer −25.5% (instr −20.8%),
  binarytrees −6.6% (−3.8%), richards/raytrace same, deltablue +2.1% (instr +0.3%); **splay +53.9% (instr +75.8%)**.
  `define` now goes to `grow` directly when a shared-shape object is full (no copy-then-grow); splay unchanged.
- 2026-10-13: splay's cause is wasmtime's grow-or-collect policy, not our code (F304). Evidence: every `--count`
  counter equal; `-C collector=null` (no GC): new 3.18G vs base 3.34G instructions; GC heap 1 GiB (no collections):
  3.17G vs 3.33G; 256 MiB: 33.7G vs 15.7G (base 4.7× its no-GC cost too), 64 MiB: 40.1G vs 23.2G. wasmtime 49
  `store/gc.rs` `should_collect_first`: collect instead of grow while the last post-GC usage is < capacity/2 = one
  whole semi-space, so a steady live set just under a semi-space collects constantly and never grows; T117's
  smaller live set moved splay into that zone. Small tests flip both ways (payload trees with array leaves: base
  339G vs new 35G; with array+string leaves: 27.7G vs 45.2G; flat 500k live: new −17%; allocation loops −14…−17%).
  Reported to the user; options asked.
- 2026-10-13: user: keep T117, measure larger GC reservations (T98 `--gc-reserve`) on all 18.
- 2026-10-13: GC reservation measured (laptop, `out/reserve-t117.tsv`, harness in the session scratchpad: 18 programs
  × base/T117 at 16 MiB, T117 at 64/128/256 MiB, min of 3, peak RSS, outputs = node). Geomean time vs base 16:
  T117 16 → 0.998 (mean RSS 72 MB vs base 87), 64 → 0.978 (142 MB), 128 → 0.992 (243), 256 → 0.962 (447). splay
  leaves the trap only at 256 (838 ms vs base 2,830; 532 MB); small programs get slower with big reserves
  (pidigits +33%, fannkuchredux +18% at 256). binarytrees/knucleotide jump between configurations (trap + noise).
  User: keep T117 with 16 MiB; splay's trap recorded under F304.
- 2026-10-13: fixtures `commonjs/` 15, `modules/` 14, `node/` 42 pass (`fetch.js` timed out under load, passes alone). User: a-03 gate + test262.
- 2026-10-13: a-03 gate vs `e726771` (`out/gate-t117-a03.log`): earley-boyer 6,286 → 4,856 ms (−22.7%, instr −20.8%),
  binarytrees −5.1% (−3.7%), fasta/mandelbrot/pidigits −2.3…−2.9% (equal instr), deltablue +3.3% (instr +0.28%),
  splay +65.3% (instr +80%: the F304 trap, accepted); outputs = node. Micro: `new P` 152 → 138.5 ns, literal
  `{x, y}` 86.5 → 69.5 ns; other cases ±2…5 ns both ways (primitives too), `hasOwnProperty` on a function +15 ns.
  Next: test262 (`out/t262-t117.log`).
- 2026-10-13: `hasOwnProperty` on a function (+15 ns on a-03): laptop scratch loop +9 instructions per call (+0.4%): the `e < count` check on index hits (`Function.prototype`/`Object.prototype` lookups use their index); kept (correctness of shared keys); the rest of the 15 ns is noise.
- 2026-10-13: full test262 on a-03 (`out/t262-t117.log`, 1,489 s): no changes since the baseline. Done; docs updated.
