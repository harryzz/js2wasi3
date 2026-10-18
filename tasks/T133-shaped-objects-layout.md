# T133: objects as one WasmGC struct per shape, small header (step 2 of the new design)

Status: done (2026-10-18) | Depends on: T000, T131, T132 | Local (+ a-03 for the gate, after asking)

## Goal
Objects whose shape the type analysis knows become a WasmGC struct per shape with a small header (flags +
overflow dictionary, as `probes/layout2.wat`), not under today's 7-field `$object` header (`probes/layout3.wat`:
that header costs 2.0× V8 / 1.7× wasmtime); every generic path reads them. Then `f64` fields (step 3).
Probe numbers: struct instead of keys/vals 1.7×, `f64` fields 3.2× (V8; `tasks/T132-method-calls.md` Progress).

## Read first
- `tasks/T000-general-speed-analysis.md`, `tasks/T131-type-analysis.md`, `tasks/T132-method-calls.md`.

## Design options (2026-10-18)
- **(a) a struct per shape that is a final subtype of `$object`** (today's 7-field header, typed fields after it). Every
  generic path keeps working on the header (`keys` = the shape's shared keys, `proto`, `count`, attributes); only the
  ~40 places that read or write an object's `vals[e]` (`object.rs`, `props.rs`, `lib.rs`, `func.rs`) go through
  `slot_get`/`slot_set`, which read a shape's own fields through a per-shape function. Delete or attribute change of a
  shape key copies the fields into `vals` and sets a flag (fast paths then fail their guard). Added keys go to
  `keys`/`vals` as today.
- **(b) the small header of probe 2** (flags + overflow dictionary): not an `$object`, so every generic path (hundreds of
  reads of `class`/`proto`/`keys`/`attrs`/`count`/`index`, every object test chain) must learn a second object kind.
- Probe (`probes/layout3.wat` `run_hdr` = (a), `run_small` = (b), same kernel, f64 fields; laptop, best of 3,
  n = 20M): V8 534 vs 447 ms, wasmtime 754 vs 536 ms; today's layout (`run_dict`) 1,551 ms on V8. So (b) is 1.19× (V8) /
  1.41× (wasmtime) faster than (a) on this allocation-heavy kernel; (a) is ~2.9× faster than today.

## Plan (option (a))
Shapes are the build-time layouts (`Module::layouts`, T109: a constructor's `this.k = …` keys, a literal's keys; 1–8
keys). Run-time learned layouts (T128) stay as today.
- [x] Step 1, backend: per field count a final struct type `$shapeN` (`$object`'s 7 fields + N `eqref` fields; f64 in
  step 3), in the object test chains after `$plain`/`$array`/`$function`. `CreateThisLayout`/`NewObjectLayout` make
  one: the layout's shared keys/attrs/index, `vals` = a shared empty array. Invariant: an object whose `vals` is
  shorter than its `count` keeps entries `0..count` in its fields (`keys.len == N`). Every `vals[e]` read/write goes
  through `slot_get`/`slot_set` (inline: `e < vals.len` → the array, else `field_get`/`field_set`, a chain over the
  shape types); `grow` copies the fields out into a real `vals` (from then the fields are dead). `$plain`-only fast
  paths (method cache, layout set/define, T111 number slots) learn the shape types or fall back to generic.
- [x] Step 2, lowering: reads/writes `o.k` where the analysis gives one constructor/literal with a layout: guard
  (exact shape type, `index` = the layout's, `count > slot`) + `struct.get/set`, else the generic path.
- [x] Step 3: `f64` fields where the analysis says a key only ever holds numbers.
- [x] Gate on a-03 (ask), test262 (ask).

## Progress
- 2026-10-18: started (user: "Yes"). Read the object model: `$object` header fields are read in hundreds of places,
  `vals` in ~40. Added `run_small` (option (b)) to `probes/layout3.wat` and measured (a) vs (b) (above).
- 2026-10-18: user chose (a) (today's header kept; the small header can be a later step of its own).

- 2026-10-18: step 1 built: `$shapeN` types (one per field count of constructors' layouts, in `runtime::types`'
  group; `runtime::set_shape_types`/`object_kinds`: the object chains include them), `G_NOVALS`, helpers
  `alloc_object_shape`, `field_get`/`field_set`; `slot_get`/`slot_set` at every `vals[e]` access (`object.rs`,
  `props.rs`); `own_shape` and `grow` move a shape object's fields into a real `vals` (so fields are live exactly
  while the shared index is kept); `create_this_layout` makes shape objects; the method cache (T132) accepts shape
  receivers and holders. Outputs = node on a scratch test (adds, delete, defineProperty, freeze, for-in, spread,
  classes) and fixture dirs objects, classes, closures, builtins, arrays, exceptions, commonjs.
- 2026-10-18: step 1 alone, laptop instructions vs b935ef8 (`scripts/bench.sh -r 2 -b`): richards +122%, nbody
  +156%, deltablue +31%, splay +4.6%, binarytrees +0.3%: the `$plain`-only fast paths (T110 inline layout reads,
  T111 number cells, T105 guesses read through `slot_get`) now miss on shape objects; step 2's typed paths replace
  them. Analysis: `Result::fields` (member span → the one constructor of its receivers) and `num_fields`
  (constructor, key) only numbers stored; `js2wasi3 types` prints their counts (richards 210 field sites / 14
  number-only keys, deltablue 204/8, nbody 51/7, splay 145/3, crypto 428/13).
- 2026-10-18: step 2 built: `Intrinsic::ShapeGet`/`ShapeSet { layout, slot }` at `o.k` sites the analysis gives one
  constructor with a layout (`Lowerer::field_slot`; function and class nodes mapped to their scopes): inline guard
  (exact shape type, `index` = the layout's, `count > slot`) + `struct.get/set`, else `get_named`/`set_named`; the
  compound assignments' reads too. T110's inline layout reads/writes accept shape objects (`plain_or_shape`,
  `slot_read`/`slot_write`); the runtime's `slot_get`/`slot_set` reach fields inline (a test per shape type +
  `br_table`, up to 4 shape types). Laptop instructions vs b935ef8: richards −5.9%, deltablue +0.6%, earley-boyer
  +12.8% (its `car`/`cdr` reads aren't typed; 74M of them went through `get_named` before T110 took shapes),
  nbody +38%.
- 2026-10-18: step 3 built: `Module::layout_nums` (per layout, the keys only numbers are stored at on every
  constructor's objects, from `Result::num_fields`); shape types per (field count, `f64` fields); `f64` fields read
  boxed by generic paths, unboxed in numeric reads (`Convert(JsVal→F64, ShapeGet)`) and written unboxed from `f64`
  values; a non-number written to an `f64` field moves the object's entries into `vals` (`field_set` → `own_shape`,
  keeping the key of an entry being added). Bug found and fixed on the way: that move during a constructor's add
  lost the added key (the analysis said `this.early = this.peek()` stores a number; it reads a prototype's
  string: an analysis miss, the guard keeps it right). Fixture `objects/shape-fields.js` (guards' misses: added
  keys, delete, freeze, accessors, read-only, prototype setters, other receivers, `f64` fields given a string).
  Fixture dirs objects, classes, closures, builtins, arrays, exceptions, commonjs pass (`commonjs/ws` failed once
  under load at 82 s, passes alone).
- 2026-10-18: steps 1–3, laptop vs b935ef8 (`scripts/bench.sh -r 2 -b`, `out/t133-laptop-steps123.log`; times noisy,
  instructions decide): all 18 outputs = node. Instructions: richards −1.2%, deltablue +1.7%, raytrace +1.4%, crypto
  +2.5%, splay +4.9%, knucleotide +5.4%, earley-boyer +12.1%, nbody +15.1%; the rest ±0.4%. `.wasm` total +10.4%
  (11.22 → 12.39 MB). Not a win yet; reported to the user before any change.
- 2026-10-18: a-03 gate vs f9d6de0 (the compatibility page's commit; user asked; `out/gate-t133-page-a03.log`,
  `out/gate-t133-page/`), outputs = node: geomean of the 18 0.9933; richards −16.9%, deltablue −21.2% (mostly
  T132's); slower: nbody +18.8% (instructions +15.1%), earley-boyer +12.4% (+12.2%), knucleotide +5.2% (+5.4%),
  crypto +2.8% (+2.6%); the rest ±2%. Micros: 20 slower (write hit at a layout slot 11.8 → 37.5 ns, `new P` with
  layout adds 136 → 182 ns, prototype read miss 44.5 → 52.5 ns, `typeof`/`getPrototypeOf` +1.5–2 ns), 2 faster.
  `.wasm` +11.4% in total. Page not updated (user).
- 2026-10-18: profiles (laptop; user: "profile nbody and earley-boyer first"). nbody (`build --count`, IR): its `+=`
  statements (`bodies[j].vx += dx * mi`, `b.x += b.vx * dt`) are lowered by T111's `ElemNum` path (`+` may
  concatenate), not `member_assign`'s `ShapeSet`: `elem_num` misses on shape objects (HOLE), then `get` + `js_add` +
  `set` box the number: 22.5M times (`num_read.miss_not_object`); base: number cells (75M `num_read.cell`). The `-=`
  statements take `ShapeGet`/`ShapeSet`. earley-boyer (`perf record -e instructions:u` on `.cwasm`s, fixed 0.05,
  `out/t133-prof/`): +8.5% instructions; `layout_set` +48% (constructor adds: the object chain reaches shape types
  after `$plain`/`$array`/`$function`, then `slot_set`'s per-shape test + `br_table`), its hot user functions +28%
  / +31% (T110 inline reads: `plain_or_shape` chain, `vals.len` check, per-shape test), `instanceof_value` +26%,
  `strict_eq` +30% (longer object chains); `alloc_object_shape` 138 vs `alloc_object_layout` 218 (one allocation:
  the only gain).
- 2026-10-18: fix 1 (user: "yes"): `o.k += x` with a number `x` at a typed field: `Intrinsic::ShapeNum` (the `f64`
  field's number, else HOLE, as `ElemNum`) and `ShapeSet` in `elem_add_stmt`. Fixture `objects/shape-fields.js`
  (+= cases, a string there afterwards) passes. nbody, laptop vs b935ef8: instructions −50.4% (was +15.1%),
  time 2,585 → 1,311 ms.
- 2026-10-18: fix 2 (user: "yes"): shape types right after `$plain` in the object chains (`object_kinds`); a
  constructor's `this.k = v` (`LayoutSet`) on a shape object inline: the layout's next entry, not `NO_EXTEND`/
  `IS_PROTO`, the site's add cache (`G_SET_IC`: same prototype, tag = `G_EPOCH`), the value into the field (an `f64`
  field takes a number), `count + 1`; else `layout_set`. Laptop instructions vs b935ef8: earley-boyer −0.06% (was
  +12.1%), richards −3.3%, deltablue +0.7%, raytrace +1.7%, crypto +1.4%, splay +4.6%, knucleotide +5.4%
  (unchanged since step 1), binarytrees +0.1%; nbody −50.4% (fix 1). Fixtures pass.
- 2026-10-18: knucleotide (+5.4%, user: "yes"): its 5 shape types came from constructors in node's modules (no typed
  sites; `user_end` doesn't separate them): every object test of a Map/array went through them first, and with more
  than 4 the runtime called `field_get`. Now only layouts of constructors with typed `o.k` sites are shapes
  (`Module::layout_shapes`); `slot_get`/`slot_set` emit plain `vals[e]` without shape types. Laptop instructions vs
  b935ef8: knucleotide −0.00%, earley-boyer −4.3%, richards −3.3%, nbody −50.4%, deltablue +0.7%, splay +4.6%.
- 2026-10-18: fix 3, `.wasm` size (user: "yes"): inline shape tests in T110's reads/writes only for shape types whose
  layouts have the key (at that slot); the runtime reaches fields inline only in its hot helpers (`get_named`,
  `set_named`, `layout_set`, `layout_define`, `lookup_named`, `define`'s layout add; `slot_get_hot`), elsewhere
  `field_get`/`field_set`; the inline constructor add checks the add cache in a helper (`add_ok`); shape scratch
  locals only in functions with shape paths (they were declared in every function: most of the growth). `.wasm`
  vs b935ef8: total +1.31% (was +11.4%), largest navier-stokes +5.1%, deltablue +4.1%, richards +3.7%. Fixture dirs
  objects, classes, closures, builtins, arrays, exceptions pass.
- 2026-10-18: after fixes 1–3, laptop vs b935ef8 (`out/t133-laptop-fixes.log`; instructions decide), outputs =
  node, commonjs 15/15: nbody −50.4%, richards −3.3%, earley-boyer −3.2%, deltablue +0.3%, regexp +0.7%, crypto
  +1.4%, raytrace +1.6%, splay +4.3%; the other 10 ±0.00%. `.wasm` +1.31% in total.
- 2026-10-18: a-03 gate vs b935ef8 after fixes 1–3 (`out/gate-t133-a03.log`, `out/gate-t133/`), outputs = node:
  geomean of the 18 **0.9665**; nbody −46.8% (instructions −50.4%), earley-boyer −5.8% (−3.2%), richards −2.7%
  (−3.3%); slower: raytrace +3.1% (+1.6%), fasta +2.3% (equal instructions: placement); crypto +1.2% (+1.4%), splay
  −1.4% (−2.6%), the rest ±2%. Micros: 10 slower (write hit at a plain layout slot 12.0 → 33.5 ns, read miss to a
  prototype 46.5 → 52.0, adds over prototype defaults +9.5/+10.5, add absent key +5.8, `instanceof Object` +1.5,
  read hit at a layout slot +2.5), 5 faster (`new P` 140.8 → 135.0, read miss other slot −3.5). `.wasm` +1.31%.
- 2026-10-18: the gate's slow micro and raytrace (user: "yes"). `write hit (plain, layout slot)` (`plain.y = i` in a
  closure the analysis doesn't reach: untyped) went through T111's number write (`$plain` only) → `set_named` →
  `field_set` call: T111's numeric reads/writes now try the key's `f64` fields first (`shape_num_slots`), T110's
  write unboxes numbers into `f64` fields inline, its read boxes inline (`box_inline`); laptop: write hit 53 →
  10.5–11.5 ns (T132 14.5), read hit ~30 ns (T132 25.5). raytrace (`perf stat`, fixed 0.05): T132 65.654G
  instructions, T133 66.689G (+1.58%), with shapes switched off 65.696G (+0.06%): the cost is its 2 shape types (the
  benchmark harness's constructors) in the generic paths; shape types after `$array`/`$function` in the object
  chains: 66.500G (+1.29%; earley-boyer at that size −2.3% vs T132); `get_named` with out-of-line fields: no gain
  (reverted).
- 2026-10-18: a-03 gate vs b935ef8 again after the write-hit and raytrace fixes (user: "re-run the a-03 gate"):
  `out/gate-t133-v2-a03.log`, `out/gate-t133/`, outputs = node: geomean of the 18 0.9647 (was 0.9665); nbody
  −46.1% (−50.2% instr), earley-boyer −4.7%, richards −3.7% (−6.8% instr); raytrace +2.7% (+1.29% instr),
  navier-stokes +2.5% at +0.04% instr (placement); others within 2%. `.wasm` 11,221,257 → 11,371,113 B (+1.34%).
  Micros: 12 slower (read hit 16.8 → 20.0 ns, read miss proto 45.8 → 52.5, adds +5.5–7.2, write miss +3.8, Map
  read/write +1.2/+4.5, literal +3.8, getPrototypeOf array +1.5, one regex case), 4 faster (write hit 12.0 → 9.8,
  new P 139 → 118).
- 2026-10-18: flag bit in `class` (user: "yes, do the flag bit in class"): `FIELDS_LIVE = 0x2000` set by
  `alloc_shape`, cleared by `own_shape`/`grow`; `slot_get`/`slot_set`(`_with`), T110's `slot_read`/`slot_write` and
  the T111 field path test it instead of `e < vals.len`; Map/Set's identity hash moved up to bit 14. Fixtures: all
  directories pass (node/ 4–8 timing failures under load, each passes alone; unsupported/ is rejection tests).
  Laptop instructions vs T132 (cwasm, small inputs), gate build → flag build: raytrace +1.29% → +1.31%,
  earley-boyer −2.27% → −2.74%, deltablue +0.09% → +0.21%, regexp +0.10% → +0.19%, splay −2.32% → +0.67% (GC
  spread), others equal. Laptop micros, gate build vs flag build (6 pairs, advisory): read miss proto +4.2 ns, read
  string .x +5.2, read array own x +3.0, read miss plain +2.8, read guess hit +3.5, Map read/write +1.8/+4.0, adds
  +7–12; faster: `o[k]` −4.0, `k in o` −4.0. The bit costs a `class` load on top of the `vals` load the hit path
  needs anyway (the length test shared it). Not kept unless the user decides otherwise.
- 2026-10-18: flag bit reverted (the user left the choice to me): the code is the second gate's again. Cleanup of
  `out/` (user: yes): old gate worktrees, base builds and `.wasm`/`.cwasm` removed (8.5 GB → 918 MB); `find -delete`
  ignored `-prune`, so `out/gate-t133*/` lost their `.wasm`/`.cwasm` too (logs, outputs, counts kept). test262 on
  a-03 (`out/t262-t133.log`, 1,480 s, -j 36): **no changes since the baseline**, 28,123 of 28,984 run pass (97.0%),
  same as T132.

## Notes / Result
- **What changed.** Objects made by a constructor with a build-time layout (T109) and typed `o.k` sites (T131) are
  a final struct per (field count, `f64` fields) after today's 7-field `$object` header (option (a)): entries live
  in the struct's fields while `vals` is the shared empty `G_NOVALS` (`vals.len < count`); `own_shape`/`grow` copy
  them out (the shared index goes with them, so typed guards miss from then on). Typed sites: `ShapeGet`/`ShapeSet`/
  `ShapeNum` (guard: exact type, the layout's index, `count > slot`; else the generic path), `f64` fields where the
  analysis sees only numbers stored (a non-number moves the entries out). The runtime's ~40 `vals[e]` accesses go
  through `slot_get`/`slot_set` (fields inline in the hot helpers, up to 4 shape types); T110/T111's inline paths,
  the inline constructor add and T132's method cache take shape objects.
- **Result** (a-03 gate vs b935ef8, `out/gate-t133-v2-a03.log`): geomean of the 18 programs **0.9647**; nbody
  −46.1% (instructions −50.2%), earley-boyer −4.7%, richards −3.7% (−6.8%); raytrace +2.7% (+1.29%: its two shape
  types, from the harness's constructors, lengthen the generic paths; accepted), navier-stokes
  +2.5% at +0.04% instructions (placement); the rest within 2%. `.wasm` +1.34%. Micros: write hit 12.0 → 9.8 ns,
  `new P` 139 → 118 ns; generic reads/adds/misses +1.2…+7 ns (12 slower). test262: no changes (28,123).
- **Tried and dropped:** `get_named` with out-of-line fields (no gain); a `FIELDS_LIVE` flag bit in `class`
  instead of the `vals` length test (a `class` load on top of the `vals` load: generic reads +2…+5 ns, raytrace
  unchanged).
- **Commands:** `scripts/gate.sh -o out/gate-t133 b935ef8` (a-03), `node scripts/test262.mjs subset` (a-03),
  `scripts/diff.sh tests/fixtures/<dir>`, `perf stat -e instructions:u` on `.cwasm`s.
- **Not covered:** run-time learned layouts (T128) and object literals stay `$plain`; derived classes; keys added
  after the constructor (splay's `left`/`right`); the small header (option (b)). Follow-ups F455–F460.
- **Next smallest step:** literals as shapes (F457), or the generic paths' cost of shape types (F455).

