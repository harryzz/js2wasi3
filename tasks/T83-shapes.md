# T83: shapes (hidden classes) and shape-keyed inline caches for property access (F321)

Status: done (2026-10-05, cloud, branch `shapes-batch`; handoff chosen by the user after T82) | Depends on: T82 (numbers) | Cloud batch: yes
(`tasks/CLOUD-BATCH.md`, branch `shapes-batch`)

## Goal
Property reads and writes with constant keys hit an inline cache keyed by the object's **shape** (own hits and
prototype hits), instead of today's `G_IC` guess ("the entry index where this site last found the key, if the key
there is identical") plus a `get_named` / `find_own_named` lookup on every miss. Same results as node everywhere:
property order, attributes, accessors, `delete`, `defineProperty`, `Object.keys`/`for-in`/`JSON.stringify`, proxies
and exotic objects keep their exact semantics. Dictionary mode stays for objects that don't fit (deletes, very many
keys, whatever is simpler to leave out: decide in the design note).

## Why (measured; don't re-measure here: no replay in the cloud)
- T82 (2026-10-05, zigbee2mqtt replay on a-03): js2wasi3 22.6 s CPU vs node 8.66 s (2.6×) after T80+T81.
- T79's profile (before T80/T81): property-access helpers ~27% **self** (`get_named`, `find_own_named`, `get`,
  `find_own`, …): with UTF-8 and RegExp gone, roughly half of what's left. `box_number` 3.3% (F323/F334).
- T78's probe (F334, `tasks/T78-benchmarks.md` Progress 2026-10-04): `b.x += b.vx * dt` over 2 objects, 6M updates,
  node 45 ms, js2wasi3 1.89 s: every access is a lookup.
- F321 (T74): prototype hits (methods) and keys built at run time (`JSON.parse`, computed keys) miss today's caches.

## Read first
- `docs/design.md`: T13 (objects are `$object` dictionaries: insertion-ordered entries in GC arrays, a hash index
  past 8 entries, a prototype), T74's paragraph (named helpers, FNV-1a key hash at build time, `G_IC`, `key_temp`),
  T14/T15 (`$function`, `$array` are `$object` subtypes), T69 (casts against non-final types cost a wasmtime libcall:
  F350 — keep new hot-path casts against final types).
- `crates/backend/src/object.rs` (`$object` layout, `GetNamed`/`LookupNamed`/`FindOwnNamed`, `SetNamed`,
  `AppendNamed`, `DefineNamed`, the `G_IC` code), and where the lowering emits constant-key accesses (`crates/lower`).
- WasmGC limits: struct layouts are fixed at compile time, so a shape is a run-time object (keys + attributes + slot
  indexes + transitions) and the values stay in an array of slots; no interior pointers; i31 is 31 bits.

## Plan
1. **Design note first** (in this file, committed before code): the `$shape` struct (keys in order, attributes,
   parent, transitions keyed by interned key, a lookup map past N keys); where an `$object` keeps it and how its
   entries change (values only while it has a shape); when it falls back to dictionary mode; how enumeration,
   `defineProperty`, `delete`, `Object.setPrototypeOf`, freezing, and the `$object` subtypes see it; what happens to
   `G_IC`.
2. Shapes for ordinary objects and object literals (`DefineNamed`: literals of the same site share one shape chain).
3. Inline caches keyed by shape at constant-key sites: own read = shape compare + slot load; write to an existing
   key = shape compare + slot store; add = shape compare + transition + append.
4. Prototype hits (methods): cache (receiver shape, holder, slot) and validate the chain cheaply (e.g. a per-prototype
   or global "shape changed" epoch). If time allows: intern run-time keys (`JSON.parse`) against the module's
   constants so their objects share shapes with literals. Stop there; unboxed number slots (F323/F334) are a
   follow-up unless trivially in reach.
5. **Micro-benchmark** (sanity check only, like T80/T81): `tasks/T83-files/prop-bench.js`: T78's nbody-like probe
   (`b.x += b.vx * dt`), objects from the same literal read in a loop, method calls through a class prototype,
   `JSON.parse` results read by key, a polymorphic site (2–4 shapes), an object that goes to dictionary mode. node vs
   js2wasi3 before/after, wasmtime 49.0.2 with `-O gc-heap-initial-size=536870912`, compile time excluded.
   Optionally `scripts/bench.sh` on Richards and DeltaBlue (property-heavy), one run each.

## Tests (as `CLAUDE.md` and `tasks/CLOUD-BATCH.md`, nothing bigger)
- `scripts/diff.sh` on the object/property fixtures this touches (`tests/fixtures/` objects, classes, getters/setters,
  `delete`, `defineProperty`, `Object.*`, `JSON`, `for-in`, proxies, spread/rest, `Reflect`); a new fixture
  `tests/fixtures/builtins/object-shapes.js` covering what shapes can get wrong: insertion order after delete and
  re-add, integer-like keys first, attribute changes (writable/enumerable/configurable), accessor ↔ data, freeze/seal/
  preventExtensions, a cached site that sees a new shape, a prototype method replaced or shadowed after the cache
  filled, `setPrototypeOf` after caching, `__proto__` in literals, many keys (dictionary mode), objects from
  `JSON.parse` and `Object.create(null)`.
- `wasm-tools validate`, one wasmtime smoke run, `cargo test` of the touched crates.
- test262: only the directories most at risk, with `node scripts/test262.mjs <dirs>` against `tests/test262/
  baseline.json` (no `--update` of the full baseline in the cloud): `built-ins/Object`, `built-ins/Reflect`,
  `language/expressions/object`, `language/expressions/assignment`, `language/statements/for-in`,
  `language/expressions/class`/`language/statements/class` if they run in reasonable time, `built-ins/JSON`. The full
  suite runs in T84 on a-03.

## Acceptance
- [x] Design note written here before code.
- [x] Shapes + shape-keyed caches for own reads/writes/adds; prototype hits cached; anything else falls back with
      node's results.
- [x] New fixture and the touched fixtures pass `scripts/diff.sh`; the listed test262 directories show no regressions;
      `wasm-tools validate`; a wasmtime smoke run; `cargo test` of the touched crates.
- [x] Micro-benchmark before/after in Notes; F321 ticked (or what's left recorded); docs as `CLAUDE.md` says.
- [x] Not here: the zigbee2mqtt replay and the full test262 run (T84, local on a-03).

## Design note (2026-10-05, before code)

**Idea**: a shape is a run-time `$shape` that owns an object's *layout* (keys, attributes, hash index, count); a
shaped object's own `keys`/`attrs` fields **alias the shape's arrays**, so every existing reader (lookups,
enumeration, `Object.keys`, `for-in`, `JSON.stringify`, inspect, `defineProperty` validation) works unchanged. Values
stay per object in `vals`. An object without a shape is in dictionary mode exactly as today.

**Types**
- `$shape` (new final struct, runtime type 29; `$bigint` moves to 30): `keys: (ref $keys)` and `attrs: (ref $bytes)`
  of exactly `count` entries (never written once made), `index: (ref null $i32s)` (the same FNV-1a open-addressing
  index a dictionary gets, built when the shape has more than `SMALL` = 8 keys), `count: i32`, and the transitions:
  `child`/`sibling: mut (ref null $shape)` (a linked list of children; a child is identified by its last key, compared
  like `find_own` compares keys, and its last attribute byte), `nchild: mut i32`.
- `$object.index` becomes `(mut eqref)`: null or an `$i32s` (dictionary mode, as today) or the object's `$shape`
  (shaped). `index` is private to `object.rs`; `find_own` takes the shape's index when it holds a shape (a `ref.test`
  against the final `$shape`: inline, no libcall, F350). "Is shaped" = `index` is a `$shape`. All `$object` subtypes
  keep their field numbers.

**Which objects have a shape**: exact `$object`s from `alloc` with a class without flag bits (`{}`, literals,
`new C` of ordinary classes, `Object.create`, `JSON.parse` results, errors, built-in prototypes) start at the empty
root shape (a global, made in the global's initializer). Arrays, functions, wrappers, maps, promises, typed arrays,
generators and timers are never shaped (they keep today's paths). Shaped implies extensible (see below).

**Transitions** (adding a property `k` with attributes `a` to a shaped object, from every add path: `define`,
`append_named`, so `[[Set]]`, CreateDataProperty, `put`): find the child `(k, a)` of the current shape or make it
(copy keys/attrs + 1, index past 8 keys), point the object's `keys`/`attrs`/`index`/`count` at it, grow `vals` if
needed (doubling, at least 4), store the value. Run-time keys (`JSON.parse`, computed keys) compare by content, so
objects with the same key order share shapes whatever made their key strings: no key interning needed.

**Dictionary mode** (`unshape`: copy the shape's keys/attrs/index into the object, `index` := its own `$i32s` or null):
- a `delete` of an own property; any attribute change (`defineProperty` on an existing key that changes its bits,
  accessor ↔ data, `put` over an existing key); `preventExtensions`/`seal`/`freeze` (so shaped ⇒ extensible: the add
  caches need no extensibility check);
- an add past `SHAPE_MAX` = 64 keys, a shape with `CHILD_MAX` = 64 children already, or past the global budget of
  65,536 shapes (shapes are never freed: objects used as hash maps must not grow the tree without bound).
- `Object.setPrototypeOf` doesn't touch shapes: a shape doesn't include the prototype; caches that depend on the
  chain check the receiver's prototype themselves.

**Prototype epoch**: a `class` flag bit `IS_PROTO` (0x1000; the identity hash moves from bit 12 to bit 13) marks
objects some cache depends on as a prototype (set when a cache is filled, on every object from the receiver's
prototype to the holder, or to the end of the chain). A layout change of a flagged object (an add, a delete, an
attribute change, `integrity`, a new prototype via `set_proto`) bumps the global `G_EPOCH` (i32), which invalidates
every chain-dependent cache entry at once. Value writes don't bump it (cached holders' values are read live).

**Inline caches** (monomorphic, per constant-key site, as `G_IC` today): `G_IC` (`$i32s`) gets 3 ints per site
(`slot`, `epoch`, and T74's entry guess) and a new `G_ICR` (`$vals`) 3 refs per site (`shape`, `proto`, `extra`),
grown together. Sites are numbered for `GetNamed`, `SetNamed` and `DefineNamed` (both get a `site` argument).
- read, own: `o.index == shape` (`ref.eq`, no cast) → `o.vals[slot]`.
- read, prototype (`proto` non-null): `o.index == shape && o.proto == proto && epoch == G_EPOCH` → `extra[slot]`
  where `extra` is the holder's `vals` array (a holder's `vals` only change identity on an add, which bumps the
  epoch since the holder is flagged), or `undefined` with `slot` −1 (absent on the whole chain; not cached when the
  receiver's prototype is null). Accessors and built-in sentinels go through `props::value` as today.
- write, existing key: `o.index == shape` (filled only for a writable own data property) → `o.vals[slot] = v`.
- write, add (`extra` = the next shape): `o.index == shape && o.proto == proto && epoch == G_EPOCH` → transition.
  Filled only when the slow path added exactly `k` with default attributes and no object on the chain has `k`.
- literal define (`DefineNamed`): `o.index == shape` → transition (no chain check: CreateDataProperty).
- Unshaped receivers (arrays, functions, dictionary objects) keep T74's entry guess.

**`G_IC` before**: one entry index per read site. Kept as the third int for unshaped receivers.

**Out of scope**: polymorphic caches, unboxed number slots (F323/F334), shapes for arrays/functions, attribute
transitions (an attribute change goes to dictionary mode).

## Notes / Result
**What changed** (backend only; no IR or frontend change):
- `runtime.rs`: `$shape` (type 29), `$ic` (30), `$ics` (31; `$bigint` moved to 32); `$object.index` is an `eqref`
  (null / `$i32s` dictionary index / `$shape`); globals `G_EPOCH`, `G_ROOT_SHAPE` (made in its initializer),
  `G_SHAPES_N`; `G_IC` is an `$ics` (one `$ic` struct per site: `shape`, `proto`, `extra`, `slot`, `tag`, `guess`).
- `object.rs`: shapes (root shape from `alloc` for classes without flag bits; transitions by key content + attribute
  byte in `define`/`append_named`; `unshape` to dictionary mode on delete, attribute change, integrity levels, past 64
  keys, 64 children or 65536 shapes); `IS_PROTO` (0x1000) marks objects found as prototype holders, whose layout
  changes and `setPrototypeOf` bump `G_EPOCH`; `get_named`/`set_named`/`define_named` take a site and check
  `$ic.shape` (own read/write, add with the next shape, prototype hit valid while `o.proto == ic.proto` and
  `tag == G_EPOCH`, absent cached too). Unshaped receivers keep T74's per-site slot guess (now also for writes).
- `props.rs` (`put`, `define_property`, `integrity` go through `set_attr`/`unshape`), `mapset.rs` (identity hash
  moved to bit 13), `lib.rs` (`SetNamed`/`DefineNamed` sites numbered like `GetNamed`).
- `docs/design.md` (T83 paragraph), new fixture `tests/fixtures/builtins/object-shapes.js`, bench files in
  `tasks/T83-files/`.

**Tests**: `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`:
all pass but `node/fetch-https.js` and `node/dgram.js` (fail the same before T83: no outbound DNS/HTTPS or IPv6 in
the sandbox). `cargo test -p js2wasi3-backend -p js2wasi3`: pass. test262 (`node scripts/test262.mjs`, no `--update`):
`built-ins/Reflect`, `built-ins/JSON`, `language/statements/for-in`, `language/expressions/assignment`,
`language/expressions/object` (2092 tests): "No changes since the baseline"; with `language/expressions/class` and `language/statements/class` added (10518 tests, final build): "No changes since the baseline". `built-ins/Object` is not in
the sandbox's sparse test262 checkout / baseline: not run (T84's full run covers it).

**Micro-benchmark** (`tasks/T83-files/bench.sh`, wasmtime 49.0.2 precompiled, min of 5, ms; same output as node):

| case | node | before | after |
|---|---|---|---|
| nbody | 18 | 1287 | 1238 |
| literals | 7 | 136 | 87 |
| methods | 3 | 119 | 85 |
| json | 5 | 57 | 27 |
| polymorphic | 5 | 116 | 123 |
| dictionary | 22 | 153 | 155 |

Gains are prototype hits (methods), run-time keys (JSON.parse objects share shapes by key content) and adds
(literals). nbody is dominated by number boxing (F323/F334), not property lookup. `polymorphic` is a little slower:
one entry per site thrashes between shapes (F357). A hit still costs ~15 ns: `test_object`'s type tests and the
helper call (F359).

**Unsupported / left**: F357-F361. **Next smallest step**: T84 (local, a-03): merge, full test262, one replay.

## Progress
- 2026-10-05: handoff written after T82 (user chose F321 shapes as the next speed item; to run in the cloud on
  branch `shapes-batch`, like T80/T81).
- 2026-10-05 (cloud): tools installed in the sandbox (wasmtime 49.0.2, wasm-tools 1.259.0, node 24.21.0, rustc
  1.98.1; wkg/wit-bindgen/wac/wasm-opt not installed: not needed here). Branch `shapes-batch` from `origin/main`.
- 2026-10-05: `tasks/T83-files/prop-bench.js` + `bench.sh` written; before (min of 3, ms): node nbody 18, literals 7,
  methods 3, json 5, polymorphic 5, dictionary 22; js2wasi3 nbody 1336, literals 149, methods 117, json 56,
  polymorphic 112, dictionary 151 (same output as node).
- 2026-10-05: design note written (above), committed before code.
- 2026-10-05: implemented (backend `object.rs`, `props.rs`, `runtime.rs`, `mapset.rs`, `lib.rs`): `$shape` (type 29),
  `$ic`/`$ics` (30, 31; `$bigint` 32), `$object.index` an eqref, root shape on `alloc`, transitions in `define`/
  `append_named`, `unshape`/`set_attr` for deletes, attribute changes and `integrity`, `IS_PROTO` (0x1000; the identity
  hash moved to bit 13) and `G_EPOCH`; read/write/add/literal caches per site. Design change from the note: one
  `$ic` struct per site in `G_IC` (an `$ics` array) instead of parallel `$i32s`/`$vals` arrays: each array access
  costs a bounds check sequence in wasmtime, a struct field doesn't (the hit path has 2 array accesses instead of 5).
  Measured on a hit: ~15 ns per `get_named` call either way (most of it `test_object`'s type tests and the call); the
  shape check is about as fast as T74's guess for own hits, the gains are prototype hits, writes and adds.
- 2026-10-05: `tests/fixtures/builtins/object-shapes.js` (new) passes; `scripts/diff.sh` on `objects/`, `classes/`,
  `builtins/`, `arrays/`, `commonjs/`, `modules/`, `node/`: all pass but `node/fetch-https.js` and `node/dgram.js`,
  which fail the same with the build before T83 (the sandbox has no outbound DNS/HTTPS for wasmtime and no IPv6).
  `cargo test -p js2wasi3-backend -p js2wasi3`: all pass (installed wac 0.11.0 for the 4 composition tests).
- 2026-10-05: `set_named` on unshaped receivers: T74's slot guess for writes too (dictionary 165 -> 155 ms).
  Final bench (min of 5) in Notes.
- 2026-10-05: test262 Reflect, JSON, for-in, expressions/assignment, expressions/object: no changes since the
  baseline (2092 tests); the same plus expressions/class and statements/class with the final build (10518 tests):
  no changes since the baseline.
