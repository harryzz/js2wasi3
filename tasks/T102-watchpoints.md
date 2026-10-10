# T102: watchpoint flags for built-in invariants (F390)

Status: done (2026-10-10) | Depends on: T101 | Local

## Goal
JSC's idea (`runtime/JSGlobalObject.h`'s watchpoint sets, `docs/bun-jsc-analysis.md` #3): a fact about the built-ins
("`%ArrayIteratorPrototype%.next` is the built-in one") is kept in one flag, cleared once by the first write that
changes it; fast paths test the flag instead of looking the properties up. Two groups here:

1. **Array and string iteration** (`for-of`, spread, destructuring, `Array.from`…: `iter.rs` `Open`). Today arrays
   and strings skip the protocol unless the program names `iterator`, and then pay a generic
   `get(v, Symbol.iterator)` per loop; a replaced `%ArrayIteratorPrototype%.next` is never seen. **Wrong today**
   (checked 2026-10-10):
   ```js
   Object.getPrototypeOf([][Symbol.iterator]()).next = function () { return { done: true }; };
   for (const x of [1, 2]) console.log('for-of', x);   // node: nothing; js2wasi3: 1, 2
   console.log('spread', [...[3, 4]].length);          // node: 0; js2wasi3: 2
   ```
2. **RegExp** (`regexp_plain`, T76/T81): the loop over `REGEXP_PLAIN_KEYS` (10 keys of `RegExp.prototype` compared
   with a snapshot on every call) becomes one flag test; the per-object part (class, prototype, own `lastIndex`) stays.

Measured cost before (perf, 2026-10-10, `docs/performance.md`'s profiles): `regexp_plain` 0.6% of octane-regexp and
regex-logs, 4.7% of template-render; `iter_open` ≤ 0.4% of every profiled program. So: a correctness fix for (1) and a
small speed gain; not F349's ~9% (`docs/bun-jsc-analysis.md` said so; that was before T81).

Not here: species, `Promise.prototype.then`, `Map`/`Set` (no measurement shows them; follow-ups if one does).

## Design
- Attribute bits (the `$bytes` attrs array, bits 1/2/4/8 used): `WATCH_ITER` (16: `Array.prototype[Symbol.iterator]`,
  `%ArrayIteratorPrototype%.next`), `WATCH_STRITER` (32: `String.prototype[Symbol.iterator]`), `WATCH_REGEXP` (64:
  `REGEXP_PLAIN_KEYS` on `RegExp.prototype`), set when the prototype is made (`object.rs` `proto`).
- One `i32` global `G_INTACT`: a bit per group, all set at start; cleared by `$watch_fire(attrs)` (`G_INTACT &=
  ~(attrs >> 4)`).
- Every place that changes an existing entry's value or attributes, or removes it, fires its bits. The common write
  (`set_named` on a writable data property) already tests `attrs & (ACCESSOR | READONLY)`: the watch bits join that
  mask, so it costs nothing; the slow path fires. Census of the write sites (2026-10-10): `object.rs` `define_named`,
  `set_named`, `define`, `remove`, (`grow` copies attrs: unchanged); `props.rs` `set_attrs`, `set`,
  `define_property` (`private_access`: private names only).
- Users: `Open`'s array fast path: an array whose prototype is `Array.prototype`, no own properties, and the
  `WATCH_ITER` bit; strings: `WATCH_STRITER`. Otherwise the protocol (as for other objects). `regexp_plain`:
  `WATCH_REGEXP` instead of the snapshot loop (the snapshot code goes).

## Read first
- `crates/backend/src/iter.rs` (`Open`), `object.rs` (`proto`, `regexp_plain`, `REGEXP_PLAIN_KEYS`, the write
  sites above), `props.rs` (`set`, `set_attrs`, `define_property`), T90 (`G_EPOCH`, `IS_PROTO`), T81 notes.

## Plan
- [x] Attr bits, `G_INTACT`, `$watch_fire`; mark the guarded entries in `proto`.
- [x] Fire at every write site of the census (and check the census again with a wider search).
- [x] `Open`: the flag instead of the `iterator`-gated lookup; `regexp_plain`: the flag instead of the snapshot.
- [x] Fixture `tests/fixtures/builtins/watch-iter.js` (replaced/deleted/redefined `next`, `[Symbol.iterator]` on the
  prototype and on an instance, strings, spread, destructuring, `Array.from`, a frozen prototype, after-the-fact
  restores); `regex-observable.js` and the regex fixtures still pass.
- [x] test262 subsets: `built-ins/Array/prototype/Symbol.iterator`, `built-ins/ArrayIteratorPrototype`,
  `language/statements/for-of`, `language/expressions/array` (spread), `built-ins/RegExp/prototype`,
  `built-ins/String/prototype/{match,matchAll,replace,replaceAll,split,search}`; then the full run on a-03 (ask).
- [x] Before/after: template-render, regex-logs, octane-regexp (`scripts/bench.sh` / the scriptc workloads).

## Acceptance
- The example above prints what node prints; every fixture touched = node; test262 0 regressions.
- No benchmark slower beyond noise; template-render's `regexp_plain` share down.

## Notes / Result
**What changed** (backend only):
- `object.rs`: attribute bits `WATCH_ITER` (16), `WATCH_STRITER` (32), `WATCH_REGEXP` (64); `watch_fire` (clears the
  groups of an attribute byte in `G_INTACT`); `Helper::WatchMark` marks the guarded entries when `proto` makes
  `Array.prototype`, `%ArrayIteratorPrototype%`, `String.prototype`, `RegExp.prototype` (a missing key clears its group
  at once); fires in `set_named` (the watch bits join the `ACCESSOR | READONLY` mask: the usual write costs nothing),
  `define_named`, `define`, `remove`. `RegExpSnapshot` and `regexp_snapshot` are gone; `regexp_plain` tests
  `WATCH_REGEXP` instead of comparing ten entries.
- `props.rs`: fires in `set` (existing data property), `set_attrs` (integrity levels, `put`), `define_property`'s apply.
- `runtime.rs`: `G_INTACT` (`(mut i32)`, all groups set) in `G_REGEXP_SNAP`'s slot.
- `iter.rs` (`Open`): an array takes the index path when its prototype is `Array.prototype`, it has no own properties
  and `WATCH_ITER` holds; a string when `WATCH_STRITER` holds; everything else the protocol. No more `names("iterator")`
  gate: a replaced `next` (which no name test could see) is now honoured.

**Result.** Correctness: a replaced, deleted, redefined or getter-backed `%ArrayIteratorPrototype%.next` /
`Array.prototype[Symbol.iterator]` / `String.prototype[Symbol.iterator]` is seen by `for-of`, spread, destructuring,
`Array.from`, `new Set(…)` (the example in Goal now prints node's output). Speed (a-03, 5 interleaved runs):
template-render −4.1% (the `regexp_plain` share), 12 other programs within ±0.8%.

**Tests.** New `tests/fixtures/builtins/watch-iter.js`; `scripts/diff.sh` on `builtins/`, `objects/`, `classes/`: 98
pass; the 7 RegExp fixtures pass; `cargo test -p js2wasi3-backend` ok; `wasm-tools validate` + wasmtime runs. test262:
the subsets (1,648 tests) and the full run on a-03: no changes since the baseline (28,105 of 28,984, 97.0%).

**Not done / found.** Species, `Promise.prototype.then`, `Map`/`Set` watch groups (nothing measured shows them: they
can join the same mechanism with a bit each). F396 (`Object.create` drops symbol-keyed descriptors), F397 (string
iterators share `%ArrayIteratorPrototype%`). Subclass instances of `Array` get `Array.prototype` (F106/F108), so they
take the index path as before.

**Next smallest step.** None needed here. F391 (unboxed number arrays) or F389 (`var x;` stays `f64`) are the next
speed items from `docs/bun-jsc-analysis.md`.

## Progress
- 2026-10-10: task created (user: "next up F390"). Measured the targets first (see Goal): `regexp_plain` 0.6–4.7%,
  `iter_open` ≤ 0.4%; found the replaced-`next` bug (node vs js2wasi3 above). Design and write-site census above.
- 2026-10-10: implemented: `WATCH_ITER`/`WATCH_STRITER`/`WATCH_REGEXP` attribute bits, `G_INTACT` (in
  `G_REGEXP_SNAP`'s slot), `object::watch_fire` at the census sites (`set_named`'s mask, `define_named`, `define`,
  `remove`, `props` `set`, `set_attrs`, `define_property`'s apply), `Helper::WatchMark` in `proto` (replaces
  `RegExpSnapshot`); `iter_open` and `regexp_plain` test the flag. Release build clean (0 warnings); the example now
  prints `spread 0` only, as node.
- 2026-10-10: bug of mine fixed: `G_INTACT & bit` used as a 0/1 boolean (the string group's bit is 2): now `!= 0`.
  `tests/fixtures/builtins/watch-iter.js` = node (plain, own iterator, another prototype, regex match arrays;
  `next` replaced/restored, a `next` getter; `Array.prototype[Symbol.iterator]` replaced/deleted/redefined;
  `String.prototype[Symbol.iterator]`; frozen `Array.prototype`). The 7 RegExp fixtures (`regex-observable.js`, …)
  pass. Found, not in scope: `Object.create(p, descriptors)` drops symbol-keyed descriptors (F396; `defineProperties`
  keeps them); `class extends Array` instances get `Array.prototype` (F106/F108).
- 2026-10-10: `scripts/diff.sh` on `builtins/`, `objects/`, `classes/`: 98 pass, 0 fail (1 skip, as before);
  `cargo test -p js2wasi3-backend` ok. test262 subsets (release, -j 4, local): `Array/prototype/{Symbol.iterator,values}`,
  `language/statements/for-of`, `language/expressions/array`, `RegExp/prototype`, `String/prototype/{match,matchAll,
  replace,replaceAll,split,search,Symbol.iterator}`: 1,648 tests, "No changes since the baseline"
  (`ArrayIteratorPrototype` isn't in the sparse checkout). A laptop before/after of template-render, regex-logs,
  octane-regexp (HEAD vs T102, 5 alternating runs) is noise (±2×; outputs equal): the measurement goes to a-03.
- 2026-10-10: a-03 (user: both runs now): HEAD's compiler (T101, `~/js2wasi3-head-t101`) vs T102, the same sources
  (`~/t104/a`: the scriptc comparison's shared files), 5 interleaved runs, median (`~/t102-bench.sh`):
  | program | HEAD ms | T102 ms | change | output |
  |---|---|---|---|---|
  | template-render | 1101 | 1056 | -4.1% | same |
  | regex-logs | 2526 | 2526 | +0.0% | same |
  | csv-numbers | 4718 | 4735 | +0.4% | same |
  | json-records | 3875 | 3886 | +0.3% | same |
  | records-sort | 4920 | 4961 | +0.8% | same |
  | functional-pipeline | 4610 | 4614 | +0.1% | same |
  | word-graph | 2479 | 2470 | -0.4% | same |
  | regexredux | 3024 | 3006 | -0.6% | same |
  | octane-regexp | 22322 | 22191 | -0.6% | same |
  | octane-richards | 5025 | 5043 | +0.4% | same |
  | octane-deltablue | 3726 | 3734 | +0.2% | same |
  | octane-earley-boyer | 10185 | 10145 | -0.4% | same |
  | nbody | 6068 | 6113 | +0.7% | same |
  template-render: every T102 run (1,048–1,085) at or below HEAD's best (1,088): the `regexp_plain` share (4.7%)
  as expected; the rest within ±0.8% (noise), so the wider write mask in `set_named` costs nothing measurable.
  Full test262 started on a-03 (`~/t262-t102.log`, `--update --causes 25 subset`).
