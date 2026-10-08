# T91: `Map` with string keys (F338)

Status: in progress (code done 2026-10-08 in a cloud session; full test262 + one replay left, local) | Depends on: T90 | Local

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
- [x] A string-key hit normally does one `string_eq`; results = node (fixtures).
- [x] Micro-benchmark and knucleotide before/after in Notes.
- [ ] Full test262 0 regressions; one replay; docs (design.md done, HISTORY.md, feasibility doc), follow-ups (done:
  F373, F374).

## Notes / Result
Code part done 2026-10-08 in a cloud session (4 cores, noisy; tools installed there at the pinned versions). Left
for a local session: full test262 on a-03 (0 regressions expected: no semantics changed), one zigbee2mqtt replay
vs T90's 21.47 s, then HISTORY.md (test262) and `docs/zigbee2mqtt-feasibility.md`, and tick F338.

**What changed** (`crates/backend/src/mapset.rs`, `runtime.rs` type of `$mtable`):
- The hash index holds pairs: `[2i]` = entry + 1, `[2i + 1]` = the entry's hash. `map_find` compares the slot's
  hash before loading the key and calling `same_value_zero`, so a string-key hit costs one `string_hash` and
  normally one `string_eq`, and a probe past another key reads one cache line.
- `$mtable.hashes` (new last field): each entry's hash, written by `set`, so `rehash` copies hashes instead of
  hashing every key again; `obsolete_drop` drops it.
- `same_value_zero` tests strings first (a string equals only a string), then numbers, then BigInts.
- `hash_f64` (numbers): `x = bits ^ bits >> 32; x *= 0x9E3779B97F4A7C15; x ^ x >> 32`. The old `(lo ^ hi) * odd`
  kept a small integer's trailing zero bits, which the index mask takes: integer keys shared a few slots and probes
  ran long (maps.js `num`: 4,096 integer + 4,096 double keys, 3.9–4.7 s → 0.22–0.25 s).
- New fixture `tests/fixtures/builtins/map-hash.js` (run-time string keys, `"1"`/`1`, `-0`/`NaN`, 10,000 number keys,
  delete and re-add, iteration across deletes and rehashes, Set with objects/symbols/BigInts, `clear`).

**Measurements** (same session, interleaved; node 24.21.0):
- knucleotide fasta 250,000 (`scripts/bench.sh` builds, `.cwasm` timed directly): before 8.74–10.45 s, after 7.60–8.46 s over 4
  interleaved runs (min −13%), node 1.84–2.22 s; output identical. `scripts/bench.sh -r 3 knucleotide`: before
  8.34 (8.35) s, after 7.49 (7.78) s.
- `tasks/T91-files/maps.js` (n=300,000; node / before / after, ms, 3 runs): hit ~48 / 830–1010 / 836–895;
  miss ~30 / 526–769 / 560–634; churn ~100 / 572–818 / 623–692; set ~31 / 215–267 / 163–169; num ~43 /
  3,915–4,747 / 218–251. String cases: equal to −20% (the probes were already short there; their cost is
  hashing, the method calls and `substring`).
- perf after: `map_find` ~10% self, of which ~half is the first index load (cache miss on the 12-/18-mer tables);
  `string_eq` 7.5% (the one compare a hit needs), `get_named` + `find_own_named` + `lookup_named` ~15% (F359),
  `is_subtype_cached` + `is_subtype` ~7.5% from `get_named` on a `$map` receiver (F373), `string_hash` 4.9%.

**Commands**: `cargo build --release -p js2wasi3`; `scripts/diff.sh` on `map-hash.js`, `map-set.js`, `weakmap.js`
and every fixture naming `Map`/`Set` (16 files + `tests/fixtures/commonjs`: all pass with node 24.21.0; with the
sandbox's default node 22 three fail on node-version output differences only); `wasm-tools validate` + a
`wasmtime run -Sp3` smoke of `map-hash.js`; `cargo test -p js2wasi3-backend` (ok); `scripts/bench.sh -r 3
knucleotide`; perf (`linux-tools` 6.8, `--profile=perfmap`).

**Unsupported / unchanged**: string hashes aren't cached on the string (each call hashes the key: `string_hash`
4.9%); method lookups (F359, F373).

**Next smallest step**: the local finish above; then F373 (cast to the exact final type in `get_named`), which is
~7.5% of knucleotide.

## Progress
- 2026-10-06: handoff written after T90 (user: F338 next, in a new session). Timings and profile above.
- 2026-10-08 (cloud session, 4 cores; tools installed: wasmtime 49.0.2, wasm-tools 1.259.0, node 24.21.0): before,
  at c940818: `scripts/bench.sh -r 3 knucleotide` node 1.55 (1.66) s, js2wasi3 **8.34 (8.35) s**, SAME.
  Micro-benchmark `T91-files/maps.js` (n=300000; node / js2wasi3, 3 runs): hit 62 / ~800 ms, miss 29 / ~570,
  churn 88 / ~520–750, set 24 / ~175, **num 36 / ~3,450 ms**. The number case is a hash defect, not probing cost:
  `hash_f64` multiplies `lo ^ hi` by an odd constant and the index takes the low bits, but a small integer's f64
  has zero low word and its high word ends in zeros, so the product does too: integer keys pile into a few slots
  (linear probing over long runs). Fix it here (the index must take well-mixed bits).
- 2026-10-08: design choice: a per-entry `hashes: $i32s` array in `$mtable` (field after `vals`), not hash bits in
  the index slot: same one extra load per probe, no limit on entry counts, and `rehash` reuses the stored hashes
  instead of hashing every key again.
- 2026-10-08: implemented: `hashes` as `$mtable`'s last field (index 7, not after `vals`: fewer field renumberings),
  filled by `set`, copied by `rehash` (no rehashing of keys), dropped by `obsolete_drop`; `map_find` compares it
  before the key load and SameValueZero; `same_value_zero` tests strings first; `hash_f64` mixes in 64 bits
  (`x = bits ^ bits>>32; x *= 0x9E3779B97F4A7C15; x ^ x>>32`). Builds, validates. maps.js: **num ~3,450 → ~205 ms**
  (node 36); hit ~800 → ~800, miss ~570 → ~520, churn ~520–750 → ~500–550, set ~175 → ~165: string cases barely
  move, so probing wasn't their cost here. Profiling next.
- 2026-10-08: knucleotide after stored hashes: 7.28 (7.67) s (node 1.73). perf (installed linux-tools 6.8 in the
  sandbox; `wasmtime run --profile=perfmap`): `map_find` still ~10–11% self. Simulated the table in Python on the
  real keys (FNV-1a, final sizes): 1.0–1.5 probes per insert for every k (12-/18-mers: 138k/140k keys, 524,288
  slots, 1.18), so the hash is fine; `perf annotate`: ~49% of `map_find` is the first index-slot load, a cache
  miss on the 12-/18-mer tables. So switched to the hash **in the index slot** (pairs `[2i]` = entry + 1,
  `[2i+1]` = hash; per-entry `hashes` kept only for `rehash`): a probe that misses touches one line, not the
  `hashes` array, the key and its string. knucleotide 7.24 (7.45) s: within noise of the first version; the
  remaining `map_find` time is that one load, which any hash lookup pays.
- 2026-10-08: step 3, `is_subtype_cached` (~6.3% + `is_subtype` 1.2%): perf can't unwind through the libcall
  trampoline, so found the sites statically (non-final types: `$string` 2, `$object` 4) in the hot functions:
  `get_named` does `test_object` then `ref.cast (ref $object)` on its receiver; a `$map` is not one of the exact
  final types `test_object` checks first, so `counts.get`/`counts.set` each pay two libcalls (test + cast). Method
  lookup path (F359), not the map path: new follow-up. (`same_value_zero`/`map_hash` test `$string` on exact
  `$string`s: inline.)
- 2026-10-08: tests: new `builtins/map-hash.js`; `scripts/diff.sh` on it and the 16 fixtures naming `Map`/`Set`
  plus `tests/fixtures/commonjs`: 3 failures turned out to be the sandbox's node 22 shadowing node 24 in PATH
  (`/opt/node22/bin` first; the first bench node times were node 22 too); with node 24.21.0 first: all pass.
  `cargo test -p js2wasi3-backend` ok; validate + smoke ok. Interleaved knucleotide before/after with node 24:
  before 8.74–10.45, after 7.60–8.46, node 1.84–2.22 s. Docs: design.md (T25 paragraph), Notes, follow-ups F373
  (`get_named` is_subtype libcalls on a `$map`), F374 (index size). Left local: full test262, replay.
- 2026-10-08: a-03 (run by the user by hand: no Claude credits there; `~/t91-run.sh` = `~/t90-run.sh` with t90→t91):
  full test262 0 regressions, the same 26 changes since the baseline as T87/T89/T90 (all to pass); baseline updated
  and pushed (5186053); HISTORY.md entry. Replay: `t79/t91.log` ends in a different report (JSON: startup 12.2 s,
  steady 903 s, 548 events, 39.25 ms CPU per event) instead of T90's "replay seconds, publishes, error lines";
  waiting for the matching number before comparing with 21.47 s.
