# T74: property access speed (F303)

Status: done (2026-10-03) | Depends on: T69 (profiling setup), the production trial (2026-10-03)

## Goal
Make `o.x` / `o.x = v` / method calls cheaper: the production trial's profile (zigbee2mqtt's start-up on 52 devices)
spends ~16% of the CPU in property lookups (`get`, `find_own`, `string_hash`, `string_eq`, `own_get`,
`to_property_key`), the biggest part that is ours (wasmtime's GC, ~49%, isn't: other WasmGC + WASI P3 runtimes
exist; wasmtime is the reference). The user's order (2026-10-03): this first, then T75 (conversions and number
boxing), then T76 (the rest: compiled code, the regex VM). Same results everywhere; programs keep working.

## What we know (at start)
- Objects are dictionaries (D1, `crates/backend/src/object.rs`): keys/vals/attrs arrays in insertion order; up to
  `SMALL` (8) entries a linear scan comparing keys with `string_eq`, beyond that an open-addressing index whose
  FNV-1a hash of the key is recomputed on every lookup and at every prototype level (`lookup` → `own_get` →
  `find_own` per level). Strings are `(array i16)` with no room for a cached hash.
- F303's ideas: hash constant keys at build time and pass the hash; keep each key's hash in the index; an identity
  fast path (`ref.eq`) before `string_eq`; the 61,442 GC-ref globals (60,276 string constants) wasmtime walks as
  roots on every collection.
- Benchmark (`tasks/T74-files/bench.sh`, `bench.cjs`, `devices.json`): zigbee-herdsman-converters' definitions for
  the trial's 52 device types (model, manufacturer, clusters only) and their exposes serialized 200×, in T62's
  `zp/patched` tree with the 56 device modules they need. Baseline 2026-10-03 (i7-8565U, wasmtime 49.0.2): node
  load 130–180 ms, find 334–363 ms, **exposes 62–73 ms**; js2wasi3 load 62–87 ms, find 798–974 ms,
  **exposes 3,602–3,712 ms** (~55×); same checksum. Build 144 s, compile 106 s (35 MB).

## Plan
1. Profile the benchmark's exposes phase (perf, `--profile=perfmap`) and a micro-benchmark of the same patterns
   (`tasks/T74-files/micro.js`: literals, class instances and prototype methods, builder chains, objects past 8
   keys, `JSON.stringify`), to see which lookups dominate (own vs prototype, small vs indexed, constant vs computed
   keys).
2. Cheap, local changes first, each measured: identity fast path; the key's hash computed once per lookup (not
   per prototype level) and for constant keys at build time; hashes kept in the index.
3. If the gap stays large: per-site inline caches (an entry index guess validated by `keys[e] == k`), or shapes.
4. Tests: `cargo test`, `scripts/diff.sh` on the object/class/array fixtures, `wasm-tools validate` + wasmtime;
   test262 at the end (objects are everywhere).

## Acceptance
- [x] The benchmark's exposes phase measurably faster, same checksum; numbers recorded (−29%).
- [x] No fixture or test262 regressions.
- [x] Follow-ups and T75/T76 handoffs written.

## Notes / Result
**What changed** (backend `object.rs`, `props.rs`, `lib.rs`, `runtime.rs`; lowering `lib.rs`)
- `GetProp`/`SetProp`/`DefineProp` with a constant key other than an array index or `length` (`object::named_key`;
  sets: not `__proto__`) go to `GetNamed`/`SetNamed`/`DefineNamed`, which take the key's FNV-1a hash computed at
  build time (`object::key_hash`): no ToPropertyKey, no element paths, one hash per access (`LookupNamed`,
  `FindOwnNamed`, `ordinary_set_named`, `AppendNamed` without a second search). `v.length` is `GetLength`; a string
  primitive's methods are looked up on `String.prototype` directly.
- Every key compare (`find_own`, named or not) is inline: identity, length, first and last unit, then `string_eq`
  (empty keys still to `string_eq`: symbols).
- Inline caches: one `i32` per constant-key read site in `G_IC` (a growable `$i32s`, a new runtime global after
  `G_REGEXP_EXEC`): the entry index of the last own hit, valid when that entry's key is identical to the site's;
  a plain value returns without `property_value`.
- Lowering: `o.k = v` / `o.k op= v` keep a constant key in place (`key_temp`) instead of a `key` local.
- `tasks/T74-files/`: `bench.cjs` + `devices.json` + `bench.sh` (zigbee-herdsman-converters' definitions/exposes for
  the production trial's 52 device types), `micro.js` + `micro.sh`, `perf-cats.py` (profile categories, callers).

**Results** (i7-8565U, wasmtime 49.0.2, 256 MiB heap; same checksum as node)

| benchmark | before | after | node |
|---|---|---|---|
| exposes ×200 | 3,602–3,712 ms | 2,529–2,732 ms (−29%) | 62–73 ms |
| find (52 definitions) | 798–974 ms | 595–700 ms (−27%) | 334–363 ms |
| micro: builders / literals / prototype methods / big objects / stringify (min) | 124 / 107 / 86 / 36 / 159 ms | 76 / 59 / 49 / 29 / 111 ms | 3–4 ms each |

Profile after T74 (benchmark): conversions/operators/boxing ~28% (T75), GC + wasmtime type checks ~26%, property
access + object building ~20%, the compiled program ~18%, JSON ~5%, other prelude ~3%, regex ~1%.

**Commands run**: `cargo test` (all pass); `scripts/diff.sh` twice (236 pass, 3 fail under the parallel load: F320;
those 3 pass alone); arrays/objects/classes/builtins fixtures (94 pass); test262 on `a-03`: 11,050 property tests,
no changes since the baseline (`tests/test262/HISTORY.md`).

**Unsupported / left** (follow-ups): F321 (prototype hits and run-time keys miss the caches: shapes or key
interning), F322 (string constants as GC-root globals), F320 (`diff.sh` under load). Not measured: zigbee2mqtt
itself end to end (the benchmark is its converters' code).

**Next smallest step**: T75 (conversions, operators and boxing: `===` with a constant operand inline first).

## Checklist
- [x] profile
- [x] changes
- [x] tests, docs, follow-ups, handoffs

## Progress
- 2026-10-03: started after the production trial (docs/zigbee2mqtt-feasibility.md). Benchmark written and
  baselined (above).
- 2026-10-03: benchmark profile (exposes phase dominates): property access ~26% (`get` 6.8, `find_own` 5.5,
  `string_eq` 4.1, `string_hash` 3.0, `to_property_key` 2.3, `own_get` 2.1, `property_value` 1.8), then `strict_eq`,
  `to_string`, `$$jsonQuote`, `box_number`, GC. `o.k` and `o.m(…)` are `GetProp(o, "k")`: the generic `get`
  (array/typed checks, `is_nullish`, `to_key`, then `lookup` → `own_get` → `find_own` per prototype level, with
  `string_eq` called per scanned entry and the FNV hash recomputed per level).
- 2026-10-03: change 1: `GetProp` with a constant key that is neither an array index nor `length` →
  `object::Helper::GetNamed(v, k, hash)` (hash computed at build time, `object::key_hash`): objects walk the chain
  with `FindOwnNamed` (no array own-get, no rehash), others take the generic `get`; `find_own`'s key compare is
  inline (identity, then lengths, then `string_eq`) in both. `tasks/T74-files/micro.js` + `micro.sh` (5 runs,
  minimum, wasmtime with the 256 MiB heap; output equal to node's): base → change 1: big objects 36 → 25 ms,
  builders 124 → 99, literals 107 → 90, prototype methods 86 → 60, stringify 157 → 148 (node: 3–4 ms each).
  Run-to-run noise ~30%: compare minimums. With wasmtime's default heap the micro is GC-bound (root tracing of
  the module's GC globals and type-registry calls ~30%): a reason to cut the number of GC-ref globals (F303).
- 2026-10-03: change 2: `SetProp` with a constant key (not `__proto__`) → `object::Helper::SetNamed` →
  `props::Helper::SetNamed` (`ordinary_set_named`: the hash given, no array paths, a new property through
  `AppendNamed`, `define` without the search). It didn't help at first: `o.k = v` lowered to `key = "k";
  $set(obj, key, …)`, so the backend saw a local; `lower::member_assign` now keeps a constant key in place
  (`key_temp`). Change 3: `DefineProp` with a constant key (object literals) → `DefineNamed`; `key_matches` also
  compares the first and last code unit before `string_eq` (same-length keys: `a`/`b`, `value_min`/`value_max`;
  empty keys still go to `string_eq`, as a symbol equals only itself). micro (min of 5): builders 99 → 80 ms,
  literals 90 → 75, prototype methods 60 → 53, big objects ~25, stringify ~145. `cargo test`: all pass (no IR
  snapshot has a member assignment). Running: the whole `diff.sh`.
- 2026-10-03: `diff.sh` (changes 1–3): 236 pass, 3 fail: `async/timers-more`, `commonjs/ws`, `node/fetch` (the
  load-sensitive ones of the wasmtime 49.0.2 run: timer order, 10 s timeouts under the parallel load); rerun alone later.
- 2026-10-03: change 4: inline caches for constant-key reads: `GetNamed(v, k, hash, site)`, one `i32` per site in
  the runtime global `G_IC` (`(ref null $i32s)`, after `G_REGEXP_EXEC`; grown by `ic_store`, ≥ 1,024); a hit is
  `e < count && keys[e] == k` (identity: objects made by the same code share the constant key strings), a miss
  is `FindOwnNamed` (remembered) or `LookupNamed` on the prototype. Own properties only (prototype hits need
  shapes to validate). Change 5: `v.length` → `GetLength` (an array's field, a string's length, else `get`);
  `get_named` on a string primitive looks up `String.prototype` directly. micro (min of 9): builders 76 ms,
  literals 61, prototype methods 49, big objects 30 (computed keys `"key" + k` never hit the cache), stringify 112.
  **Benchmark** (5 runs, same checksum as node): exposes 3,602–3,712 → **2,529–2,732 ms** (−29%), find 798–974 →
  595–700 ms (−27%). arrays/objects/classes/builtins fixtures: 94 pass.
- 2026-10-03: test262 on a-03 (11,050 property tests, 306 s): no changes since the baseline. Notes / Result written; done.
