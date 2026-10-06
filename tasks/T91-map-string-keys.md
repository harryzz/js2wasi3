# T91: `Map` with string keys (F338)

Status: todo (handoff written 2026-10-06) | Depends on: T90 | Local

## Goal
Benchmarks Game knucleotide (`tests/bench/patched/knucleotide.js`, fasta 250,000): `Map` lookups and inserts with
fresh string keys (`seq.substring(i, i + k)`, never identical to a stored key) spend ~31% in the map's probe
and compare path. Make a `Map`/`Set` lookup with a string key cost one hash and, normally, one `string_eq`. The
results must stay the same as node (SameValueZero, insertion order, iterators that survive changes, T25).

## Why (measured 2026-10-06; don't re-measure)
- F338's "over 300 s" (T78, a-03, 2026-10-04) is **stale**: at 83c660b+T90 (laptop, default heap) knucleotide runs
  linearly: fasta 2,500 0.13 s, 10,000 0.38 s, 25,000 0.94 s, 100,000 3.15 s, **250,000 8.19 s** (node 1.41 s),
  output = node. (Something between T78 and T90 removed the blow-up; not investigated.)
- perf at 250,000 (self time): `map_find` 10.5%, `string_eq` 7.2%, `find_own_named` 6.2%, wasmtime
  `is_subtype_cached` 5.7%, `get_named` 5.6%, `string_hash` 4.9%, `same_value_zero` 4.2%, `box_number` 3.6%,
  `base_counts` 3.3%, `arg_get` 2.9%, memmove 2.9%, GC (forward + scan_field + collect) ~7%, `is_number` 2.3%,
  `map_hash` 2.2%, `lookup_named` 1.7%, `String.prototype.substring` 1.6%, `map_get` 1.6%, `map_set` 1.5%.
- Reading the code (`crates/backend/src/mapset.rs`): `map_find` probes the index and calls `same_value_zero` on
  **every** occupied slot it passes, with no stored-hash check first; `same_value_zero` calls `is_number` twice
  before it gets to strings; each map call hashes the key again (`map_hash` → `string_hash`, FNV-1a over all
  units).

## Read first
- `docs/design.md`: D5 / T25 (Map and Set), T74's paragraph (named helpers), T90's note.
- `crates/backend/src/mapset.rs`: the module doc (layout, obsolete tables and iterators), `hash`, `same`, `find`,
  `insert_index`, `rehash`, `get`, `set`, `delete`.
- `crates/lower/builtins/map.js`: the prelude's `Map`/`Set` methods over the `MapOp` intrinsics.

## Plan
1. Store each entry's hash in the `$mtable` (a `hashes: $i32s` array next to `keys`, kept by `rehash`/`set`/
   `delete`/`clear`), and in `map_find` compare it before calling `same_value_zero`. Or, if equally simple, put
   the hash in the index slot (`entry + 1` and hash bits together). Pick one, write the choice in Progress.
2. `same_value_zero`: test strings first (the only case reaching it with different refs that's common), numbers
   after.
3. Look at what `is_subtype_cached` (a wasmtime libcall for `ref.test`/`ref.cast` on a non-final type) comes from
   on this profile (perf call graph `-g`); note it as a follow-up unless it's in the map path.
4. Micro-benchmark `tasks/T91-files/` (string keys built at run time: get/set/has on a hit, a miss, deletes
   interleaved; plus number keys to show no loss) vs node, before/after; knucleotide 250,000 before/after
   (`scripts/bench.sh -r 3 knucleotide`).
5. Tests: `tests/fixtures/builtins/map-set.js`, `weakmap.js`, every fixture naming `Map`/`Set` (`grep -l`), a new
   fixture if a case is untested (iteration during deletes and rehash, `-0`/`NaN` keys, string vs number `"1"`/`1`),
   `wasm-tools validate`, a smoke run, `cargo test`; then (ask first) full test262 on a-03 and one zigbee2mqtt
   replay against T90's 21.47 s.

## Not in scope (follow-ups exist)
- The method lookups `counts.get`/`counts.set` (`get_named` + `find_own_named` + `lookup_named` ~13.5%: prototype
  hits miss the T74 cache): F359.
- Default-heap GC cost: F371.

## Acceptance
- [ ] A string-key hit normally does one `string_eq`; results = node (fixtures).
- [ ] Micro-benchmark and knucleotide before/after in Notes.
- [ ] Full test262 0 regressions; one replay; docs (design.md, HISTORY.md, feasibility doc), follow-ups.

## Notes / Result
(to be written)

## Progress
- 2026-10-06: handoff written after T90 (user: F338 next, in a new session). Timings and profile above.
