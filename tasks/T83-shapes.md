# T83: shapes (hidden classes) and shape-keyed inline caches for property access (F321)

Status: todo (handoff written 2026-10-05, chosen by the user after T82) | Depends on: T82 (numbers) | Cloud batch: yes
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
- [ ] Design note written here before code.
- [ ] Shapes + shape-keyed caches for own reads/writes/adds; prototype hits cached; anything else falls back with
      node's results.
- [ ] New fixture and the touched fixtures pass `scripts/diff.sh`; the listed test262 directories show no regressions;
      `wasm-tools validate`; a wasmtime smoke run; `cargo test` of the touched crates.
- [ ] Micro-benchmark before/after in Notes; F321 ticked (or what's left recorded); docs as `CLAUDE.md` says.
- [ ] Not here: the zigbee2mqtt replay and the full test262 run (T84, local on a-03).

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written after T82 (user chose F321 shapes as the next speed item; to run in the cloud on
  branch `shapes-batch`, like T80/T81).
