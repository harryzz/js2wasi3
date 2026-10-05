# T83: shapes (hidden classes) and shape-keyed inline caches for property access (F321)

Status: todo (handoff written 2026-10-05, chosen by the user after T82) | Depends on: T82 (numbers) | Local or cloud

## Goal
Property reads and writes with constant keys hit an inline cache keyed by the object's **shape** (own hits and
prototype hits), instead of `G_IC`'s "entry index if the key there is identical" guess plus a `get_named` /
`find_own_named` lookup on every miss. Same results as node everywhere; dictionary mode stays for objects that
don't fit (many keys, deletes, accessors/attributes changes if simpler), and test262 must not regress.

## Why (measured; don't re-measure here)
- T82 (2026-10-05, zigbee2mqtt replay on a-03): js2wasi3 22.6 s CPU vs node 8.66 s (2.6×) after T80+T81.
- T79's profile (1 GiB, before T80/T81): property-access helpers ~27% **self** (`get_named`, `find_own_named`,
  `get`, `find_own`, …): with UTF-8 and RegExp gone, roughly half of what's left. `box_number` 3.3% (F323/F334).
- T78's probe (F334): `b.x += b.vx * dt` over objects, node 45 ms, js2wasi3 1.89 s: every access is a lookup.
- F321 (T74): prototype hits (methods) and keys built at run time (`JSON.parse`, computed keys) miss today's caches.

## Read first
- `docs/design.md`: T13 (objects are `$object` dictionaries: insertion-ordered entries in GC arrays, a hash index
  past 8 entries), T74's paragraph (named helpers, FNV-1a key hash at build time, `G_IC`, `key_temp`), T14/T15
  (`$function`, `$array` are `$object` subtypes), T69 (casts against non-final types cost a libcall: F350).
- `crates/backend/src/object.rs` (`$object` layout, `GetNamed`/`LookupNamed`/`FindOwnNamed`, `SetNamed`,
  `AppendNamed`, `DefineNamed`, the `G_IC` code), and where the lowering emits constant-key accesses.
- WasmGC limits (T82's note): struct layouts are fixed at compile time, so a shape is a run-time object (key list +
  slot indexes + transitions) and the values stay in an array of slots; no interior pointers; i31 is 31 bits.

## Plan (first steps; refine after reading the code)
1. Design note in this file: the `$shape` struct (keys in order, attributes, parent, a transition table keyed by
   interned key, a lookup map past N keys), where an `$object` keeps it (a field next to the entries; the entries
   become values-only when the object has a shape), when an object falls back to dictionary mode (delete, very many
   keys, prototype-like objects if needed), how `Object.keys`/`for-in`/`JSON.stringify`/`defineProperty` see it.
2. Shapes for ordinary objects and object literals (`DefineNamed`: literals of the same site share one shape chain).
3. Inline caches keyed by shape at constant-key read and write sites: own hit = shape compare + slot load; write to
   an existing key = shape compare + slot store; add = shape compare + transition + append.
4. Prototype hits (methods): cache (receiver shape, holder, slot) and validate the chain cheaply (e.g. a global or
   per-prototype "shape changed" epoch); then run-time keys: intern `JSON.parse` keys against the module's constants.
5. Measure: T78's probe and `scripts/bench.sh` subsets (Richards, DeltaBlue: property-heavy) locally; then **ask** for
   one zigbee2mqtt replay on a-03 against T82's 22.6 s (memory "ask before long runs"; don't re-run node or T82).

## Tests
The fixtures this touches (`scripts/diff.sh` on object/property/class/JSON fixtures), `wasm-tools validate`, one
wasmtime smoke run per iteration; test262 on a-03 at the end (ask first) — property semantics are its biggest area.

## Acceptance
- [ ] Design note written here before code.
- [ ] Shapes + shape-keyed caches for own reads/writes/adds; prototype hits cached.
- [ ] No diff.sh or test262 regressions (test262 run on a-03 after the user's OK).
- [ ] Numbers: T78 probe, bench subset, one zigbee2mqtt replay; table in `docs/zigbee2mqtt-feasibility.md`.
- [ ] Unboxed number slots (F323/F334) decided: in this task or a follow-up.

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written after T82 (user chose F321 shapes as the next speed item).
