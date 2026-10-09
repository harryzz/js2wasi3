# T94: computed-key property access: hash a key once (F363)

Status: done (2026-10-09, cloud session; replay + test262 on a-03: F381) | Depends on: T93 | Cloud (+ a-03: test262, replay)

## Goal
Computed-key access (`o[k]`, `get_length` on non-arrays, RegExp internals) is ~15% of the zigbee2mqtt replay
(T93 perf: `find_own` 5.1%, `get` 4.5%, `string_hash` 2.6%, `own_get` 1.8%, `to_property_key` 1.7%). `lookup` calls
`own_get` per prototype level and each `find_own` on an object with a hash index hashes the key again (FNV-1a
over all units); `Map`'s `get` then `set` of the same key hash it twice (knucleotide `string_hash` ~4%).

## Plan
1. A one-entry memo in `string_hash`: globals `G_HASH_KEY` (the last string hashed) and `G_HASH_VAL`; the same
   string object (`ref.eq`) returns the stored hash. Safe because a string visible to JS is never mutated
   (checked: `$strbuf`'s buffer never escapes, `str_buf_get` copies). Covers per-level rehashing and back-to-back
   uses of one key, with no change to the string representation.
2. Measure: knucleotide (`get` + `set` per key), a computed-key micro (`o[k]` own hits, prototype hits, misses,
   keys built at run time), plain-object micros (T92-files). Profile the remaining computed-key cost.
3. Only if (1) leaves most of it: caching hashes in strings (representation change) or per-site caches, as a
   separate design note.
4. Tests: fixtures, cargo test, validate, smoke; full test262 + replay on a-03.

## Acceptance
- [x] Before/after for knucleotide, the computed-key micro and plain objects in Notes; results = node.
- [x] Docs (design.md), follow-ups (F381; F363, F371 updated).

## Notes / Result
**What changed**: a one-entry memo in `string_hash` (`object.rs` `hash()`): globals `G_HASH_KEY` (the last
string hashed) and `G_HASH_VAL` (`runtime.rs`, after `G_SET_IC`). The same string object (`ref.eq`) returns the
stored hash. A key found on a prototype, or missing, is hashed once per lookup instead of once per indexed level;
a key used twice in a row (`m.get(k)` then `m.set(k, …)`, `o[k] = o[k] + 1`) is hashed once. Safe because strings
visible to JS are never mutated (`$strbuf`'s buffer never escapes; `str_buf_get` copies). Constant-key access
(`get_named`, `set_named`) gets its hash at build time and is untouched. Cost: one global load and `ref.eq` per
`string_hash` call, one global store per real hash.

**Measurements** (cloud, alternating T93 build vs T94 build, outputs identical):
- `T94-files/keys.js` (2M accesses, 3 rounds, ms; node in brackets): prototype hits 203–229 → 161–174 (−20%) [58];
  misses 155–168 → 142–152 (−7%) [36]; read-then-write 217–234 → 200–219 (−7%) [41]; own hits 124–188 →
  122–127 [123]; keys built at run time ~5,800 both [127].
- knucleotide: best 4.07 → 3.89 s, median ~4.27 → 3.90 s (−8.6%).
- Plain objects (`T92-files/props.js`, 8 rounds): reads median ~355 / ~362 ms, methods ~493 / ~505: unchanged within
  noise (they don't call `string_hash`).

**Found**: the keys-built-at-run-time case is GC-bound, not the property path: perf ~70% in wasmtime's root
scanning (`trace_vmctx_roots` 23%, `RegisteredType::root` 26%) on the default heap; with `-O
gc-heap-initial-size=268435456` the same case takes 315 ms instead of 5,800 ms. Added to F371.

**Commands**: full `scripts/diff.sh -j 4`: 247 pass, 19 skip, 2 known sandbox failures (`dgram`, `fetch-https`);
`cargo test -p js2wasi3-backend` ok; `cargo test -p js2wasi3` 30 pass, 4 `*_compose_with_wac` fail (no `wac` here);
validate + smoke; perf.

**Not done**: caching hashes in the strings themselves (representation change) and per-site caches for computed
keys: the zigbee2mqtt replay should show first how much of F363's ~15% is left (F381). Full test262 (F381).

**Next smallest step**: F381 (one profiled replay + test262 on a-03), then decide on F363's remainder.

## Progress
- 2026-10-09: task created from F363 (user). Read the path: `get` → `lookup` (per level) → `own_get` → `find_own`
  (hashes when the object has an index). Hashing once up front would make small objects (no index, linear scan,
  no hash today) pay a hash: chose a one-entry memo in `string_hash` instead (plan 1).
- 2026-10-09: implemented the memo; validates. keys.js: prototype hits −20%, misses/read-write −7%, own hits equal;
  knucleotide median −8.6%; plain objects unchanged. Keys built at run time: ~5,800 ms, GC root scanning on the
  default heap (315 ms with a 256 MiB heap): F371. Full suite 247 pass; cargo test ok (wac missing). Done.
