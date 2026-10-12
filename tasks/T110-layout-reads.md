# T110: constant-key reads and writes by layout, inline (F405)

Status: paused (F409) | Depends on: T109 | Local (+ a-03: benchmarks, test262)

## Goal
T109 gave objects from constructors and literals a **layout** (its shared hash index in `F_INDEX`; invariant:
index == L's ⇒ `keys[0..count]` are L's first keys, a deleted entry's key null), but only construction uses it.
Every constant-key read is still a call to `get_named` (`object_or`'s type chain, `G_IC` load, null and bounds
checks, the guessed entry's key compare, the accessor/built-in test) and every write outside a constructor a
call to `set_named` (T105's guess). Profiles (a-03, T108 builds, `~/t108-prof`, `~/t109-prof`): `get_named`
self 32.9% of nbody, 36.7% of richards, 37.2% of deltablue; `set_named` 17.1% of nbody, 9.7% of richards.
A hit is ~15 ns (T109 micro-benchmark; node ~0).

At a constant-key site whose key `k` is in a layout `L` at slot `s`, emit inline: the receiver is an exact
`$object`, `F_INDEX ref.eq L.index`, `s < count`, `keys[s]` not null (deleted), then `vals[s]` (a read: not an
accessor or the built-in sentinel; a write: the attribute byte allows it); anything else calls the helper as
today. No call, no `G_IC`, no key compare on a hit.

Chosen over F406 (unboxed number fields) on 2026-10-12 (user: "F405 first"): `box_number` from object fields is
only nbody's (11.9%); F406 builds on this task.

## Read first
- `tasks/T109-site-layouts.md` (Notes / Result, design note in Progress "design for A"), `docs/design.md` (T74,
  T92, T105, T109 paragraphs), F359, F376, F405–F408 in `tasks/README.md`.
- `crates/backend/src/object.rs`: `get_named`, `set_named`, `layout_set`, `layout_index`, `define`, `remove`;
  `crates/backend/src/props.rs` `set_attrs` (attribute changes keep the shared index); `runtime::object_or`.
- `crates/backend/src/lib.rs`: the `GetProp`/`SetProp` constant-key emission (T74), `layout_global`.

## Plan
- [ ] Measure (no new runs where existing data answers it): per benchmark, constant-key read/write sites whose key
  is in 0, 1, or several layouts (same slot or not); the dynamic share of `get_named`/`set_named` calls at
  those sites (a counting build on the laptop, small sizes). Decide static (key → one slot) vs a per-site
  remembered layout.
- [ ] Design note (in this file): the receiver type test without wasmtime's `is_subtype` (F376: `ref.test` on the
  non-final `$object` is inline only for an exact `$object`), the accessor/attribute checks, code size per site.
- [ ] Reads inline. Fixture `tests/fixtures/objects/layout-reads.js` = node (deleted keys, accessors and read-only
  keys defined on a layout object, frozen objects, subclass instances, arrays/functions/strings at a layout site,
  prototype keys, polymorphic sites).
- [ ] Writes inline (outside constructors).
- [ ] `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`;
  test262 full on a-03 (ask); a-03 benchmarks T109 vs T110 (ask); `.wasm` size.

## Acceptance
- Every fixture = node; test262 0 regressions; nbody, richards, deltablue faster; nothing slower beyond noise.

## Notes / Result

## Progress
- 2026-10-12: task created (user: "next up F406"; after the measurement below, "F405 first"). `box_number` self
  time split by caller (existing profiles, `boxsplit.py` in the session scratchpad, run on a-03): nbody 15.6%
  (11.9 `advance` = `Body` fields, 3.2 `js_add`); crypto 21.9% (`am3` locals/parameters, F404); navier-stokes
  24.3% (arrays/locals, pre-T108 profile, F403); raytrace 1.1% (no layouts there, F407); the rest ≤ 0.5%. So F406
  pays on nbody only; `get_named` (33–37% of nbody/richards/deltablue) is the larger target.
- 2026-10-12: static site counts (temporary `J2W_STATS` print in the backend at the T74 emission; whole program
  incl. the prelude): constant-key sites whose key is in no layout / exactly one / several at the same slot /
  several at different slots. Reads: nbody 88/65/0/7, richards 240/115/48/7, deltablue 444/128/12/22, raytrace
  716/116/48/21, earley-boyer 439/282/16/267, binarytrees 85/27/0/7, splay 235/96/15/7. Writes: nbody 52/16/0/0,
  richards 107/34/16/0, deltablue 157/35/5/2. The "several, different slots" keys are mostly the prelude's literal
  layouts (`value`, `done`, `get`, `enumerable`, …); earley-boyer's 267 are `cdr` (211; layouts `[car, cdr]`
  and `[cdr]`) and `val` (48; `[val]`, `[key, val]`, `[type, val, pos]`): user keys, hot in `sc_Pair` code. So keys at
  different slots need the guess per distinct slot (≤ 3 tests), not only the one-slot case. (Probe removed.)
  Design idea from this: the inline check need not compare layout identity at all. Keys are unique in an object,
  so "receiver is an `$object`, `s < count`, `keys[s] ref.eq k`, value not an accessor / built-in sentinel"
  proves an own data property `k` at `s` for **any** object: a static slot guess (the slot `k` has in every layout
  that contains it) with T74's key compare, without `G_IC`, without a call, without T109's invariant. Writes:
  same with the attribute byte (no `READONLY | ACCESSOR | WATCHED`). Keys at different slots in different
  layouts: the call as today (or a second guess; decide after measuring). Next: prototype reads, measure locally.
- 2026-10-12: reads implemented (prototype = the design). Backend: `Emitter::layout_slots` (key → its distinct
  slots over `Module::layouts`), two more scratch locals per function (`(ref null $object)`, `jsval`); at a T74
  constant-key read whose key has 1–3 (`LAYOUT_GUESSES`) slots: `br_on_cast_fail` to `(ref $object)` (inline for an
  exact `$object`), per slot `slot < count`, `keys[slot] ref.eq k`, value not `$accessor`/`G_BUILTIN` → the value;
  else `get_named` as before (same site number). Fixture `objects/layout-reads.js` (same keys at other slots,
  inherited, deleted/re-added, accessor/read-only defined later, frozen, arrays/functions/Maps/strings/numbers/
  null at a layout site, prototype getters, polymorphic loop, > 8 keys, iterator results, descriptors) = node.
  Micro-benchmark (laptop, 4M iterations, ns per iteration, noisy): `p.x` 40.5–43.5 → 27–39 (loop alone ~26),
  `p.x + p.y` 65–81 → 37–53, a site seeing two layouts (`x` at slots 0 and 2) 54–81 → 29–50.
  Laptop benchmarks (small sizes, min of 3 alternating, ms, T109 → reads inline): nbody 6277 → 4814 (−23.3%),
  richards 5307 → 4136 (−22.1%), earley-boyer 9832 → 8432 (−14.2%), raytrace 10014 → 9153 (−8.6%), deltablue
  4531 → 4157 (−8.3%), binarytrees 9546 → 8873 (−7.1%), splay 3554 → 3607 (+1.5%); outputs same.
  (`lb.sh` in the session scratchpad; baseline = HEAD a47ff0b... e088f40 built in a worktree.)
- 2026-10-12: writes implemented: at a T74 constant-key write whose key has 1–3 layout slots, the value is
  evaluated first, then per slot `slot < count`, `keys[slot] ref.eq k`, attribute byte without
  `ACCESSOR | READONLY | WATCHED` (T105's test) → `vals[slot] = x`; else `set_named` (same site numbers).
  Fixture extended (writes: in place, read-only, accessor, frozen, sealed, deleted and re-added, Map/array/number/
  null receivers, prototype setters, a loop) = node. Laptop (min of 3, ms, T109 → reads + writes inline): nbody
  6326 → 4015 (−36.5%), richards 5143 → 3749 (−27.1%), earley-boyer 10294 → 8704 (−15.4%), deltablue 4624 → 4117
  (−11.0%), raytrace 10504 → 9432 (−10.2%), binarytrees 10361 → 9479 (−8.5%), splay 3360 → 3395 (+1.0%; +1.5%
  with reads only: GC-bound, to check on a-03). Outputs same.
- 2026-10-12: `scripts/diff.sh`: objects 16/16, classes 12/12, arrays 8/8, commonjs 15/15, modules 14/14, builtins
  73/73 (1 skipped), node 42/42. (A first node/ run had 5 failures with `fetch.js` taking 161 s, laptop load ~8;
  the next two runs passed 42/42: network-timing flakes, not this change.) `.wasm` size T109 → T110: nbody 164,157 →
  172,858 (+5.3%), richards 285,856 → 306,804 (+7.3%), earley-boyer 986,933 → 1,059,062 (+7.3%), raytrace +5.7%:
  ~60–90 bytes per inline site. Next: a-03 benchmarks, test262, zigbee2mqtt build time + replay (ask).
- 2026-10-12: a-03 run started (user: "1 and 2 now"; zigbee2mqtt build + replay decided later): `~/t110-all.sh` → `~/t110-all.log`: build (`~/js2wasi3-t110`), `~/t110-bench.sh` (T109 = `~/js2wasi3-t109` vs T110, 5 interleaved, median → `~/t110-bench.log`), full test262 `--update` (`~/t262-t110.log`).
- 2026-10-12: first a-03 results (8 of 18): nbody 5080 → 3375 (−33.6%), binarytrees −6.3%, **knucleotide 5150 →
  5521 (+7.2%, all 5 runs)**, the rest −0.4…+2.8%. Likely cause (not verified by profile): `counts.get`/`.set` on a
  Map; `get` is a layout key (the prelude's descriptor literal), so the inline `ref.cast (ref $object)` of a `$map`
  calls wasmtime's `is_subtype` before the `get_named` call. Unasked, I then tried two fixes locally (testing
  `$function`/`$array`/`$map` first: knucleotide +1.2%, but nbody only −18.3%, richards −20.5% on the laptop; and a
  list of built-in method names never tried inline). User stopped it: tuning to the benchmarks, and not asked for.
  Both reverted; `crates/` is byte-identical to the build running on a-03. Open question for the user: how to
  handle non-plain receivers at inline sites (type tests first / a per-site switch-off / a cheaper plain-object
  test, F376).
- 2026-10-12: a-03 benchmarks done (`~/t110-bench.log`, T109 vs T110 as first proposed, median of 5, ms): nbody
  5080 → 3375 (−33.6%), richards 4398 → 3217 (−26.9%), earley-boyer 7871 → 6708 (−14.8%), deltablue 3771 → 3373
  (−10.6%), binarytrees 6750 → 6326 (−6.3%), crypto −1.9%, **knucleotide 5150 → 5521 (+7.2%)**, spectralnorm +2.8%,
  mandelbrot +2.5% (runs overlap), regexp +0.7%, the rest −0.9…+0.5% (raytrace −0.2%, laptop said −10%). Outputs
  same. test262 running.
- 2026-10-12: test262 stopped by the user before it finished (killed process group on a-03; `baseline.json`
  unchanged). T110 paused: the code (reads + writes inline, as first proposed) stays uncommitted in the working tree.
- 2026-10-12: **what broke T110** (knucleotide +7.2%; likely cause, not yet confirmed by a profile):
  1. knucleotide's hot loop is `counts.get(nucleo)` / `counts.set(nucleo, …)` on a `Map`.
  2. T110 inlines a constant-key access whenever the key is in **some** layout. `get` is: the prelude's descriptor
     literal `{ get, enumerable }`, compiled into every program. So every `x.get` in the program gets the inline
     check, including `counts.get`.
  3. The inline check first casts the receiver to `(ref $object)`. For an exact `$object` wasmtime does that inline,
     but a `$map` is a *subtype* of `$object`, so wasmtime calls its `is_subtype` libcall (F376): the cast succeeds,
     the slot check then fails, and the code calls `get_named` as before.
  4. So each `counts.get` / `counts.set` pays an extra libcall on top of the old cost.
  The bug is not knucleotide's: T110 decides by key name alone, and any key also used on Maps, arrays, functions
  (the other `$object` subtypes) makes those sites slower, wherever it happens (F409).
- 2026-10-12: user: no zigbee2mqtt runs for tests or measurement until asked; the replay acceptance check is
  removed (standard tests and benchmarks only).
