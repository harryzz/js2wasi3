# T92: object type tests without wasmtime's `is_subtype` libcall (F373)

Status: done (2026-10-08, cloud session) | Depends on: T91 | Cloud

## Goal
`test_object` and the casts to `$object` (`ref.test`/`ref.cast (ref $object)`) call wasmtime's `is_subtype` runtime
function whenever the value is an object but not exactly `$object` (a `Map`, typed array, promise, wrapper…):
`$object` isn't final, so only an exact match is inline. knucleotide pays it twice per `counts.get`/`counts.set`
(`get_named`): `is_subtype_cached` + `is_subtype` + trampoline ~9.3% of its profile (T91). Make these checks inline.

## Plan
1. `$object`'s subtypes are exactly 10 final types (function, array, promise, generator, timer, map, mapiter,
   buffer, typed, wrapper). `test_object`: test them all as exact final types (inline) before the `ref.test (ref
   $object)` fallback, which is then inline for plain objects and a libcall only for rare non-object structs.
2. `cast_object(v)` (`JsTest`): a `br_on_cast` chain to those final types, then `ref.cast (ref $object)` (inline
   for a plain object). Replace `local.get v; ref.cast (ref $object)` everywhere.
3. Measure: knucleotide (`scripts/bench.sh -r 3 knucleotide` and interleaved runs vs the T91 build), the T91
   micro-benchmark, and a plain-object case (T90's `adds.js`, plus property reads) to show plain objects don't lose.
4. Tests: fixtures touching Maps/typed arrays/promises/wrappers, `cargo test -p js2wasi3-backend`, validate, smoke.

## Acceptance
- [x] No `is_subtype` libcall left in knucleotide's profile from `get_named`; results = node.
- [x] knucleotide and plain-object before/after in Notes.
- [x] Docs (design.md), follow-ups (F376, F377).

## Notes / Result
**What changed**: `runtime::object_or(s, v, not_object)` (`crates/backend/src/runtime.rs`): one chain that tests and
casts a value to `(ref $object)`: the abstract `struct` test, `$number`/`$oddball` out, then `br_on_cast` to the
final `$function`, `$array`, `$map` and last to `$object` (inline for a plain object), else the `not_object` code.
`get_named` (`object.rs`) uses it instead of `test_object` + `as_obj` (`ref.cast (ref $object)`). Before, a `Map`
receiver paid wasmtime's `is_subtype` twice per call (the test's fallback and the cast); arrays and functions
paid it once (the cast). Plain objects: 7 checks before and after.

The plan's first version (all 10 final subtypes tested exactly in `test_object`, a `br_on_cast` chain in every
cast) was reverted: an exact final-type test costs ~1.5 ns here, and ~20 per property access made plain-object
reads 65% slower (Progress).

**Measurements** (cloud, interleaved, T91 build vs T92 build; outputs identical):
- knucleotide fasta 250,000: best 5.71 → **5.16 s (−9.6%)**, median ~5.8 → ~5.28 s (4 rounds); the libcall
  (`is_subtype_cached` + `is_subtype` + trampoline, ~9.3% of the T91 profile) is gone from the profile.
- Plain objects (`T92-files/props.js`, 3M iterations, 11 rounds): reads best 372 → 358 ms, medians ~420 / ~392;
  methods (`T92-files/meth.js`, 4M calls, 12 rounds): median 752 ms both. Unchanged within noise.
- Array/function method calls (`arr.push/pop/shift`, `f.call`): no measurable change (other costs dominate).

**Commands**: `cargo build --release -p js2wasi3`; full `scripts/diff.sh -j 4` (node 24.21.0): 247 pass, 19 skip,
2 fail (`node/dgram.js`: no IPv6 here; `node/fetch-https.js`: proxy; both fail before T91 too); `cargo test -p
js2wasi3-backend` ok; `wasm-tools validate` + smoke; perf (`--profile=perfmap`).

**Not done**: the other 69 `test_object` + `as_obj` sites (set_named, the generic `get`/`set`, calls…) still pay
the libcall for non-plain objects (F376). zigbee2mqtt replay (a-03, 2026-10-09, run by the user): CPU 21.00 s (T91 21.26 / 21.51 s, T90 21.47 s), matching
as T90 (139 exact, 1 unmatched): no loss, 1–2% less (within noise). F377 done.

**Next smallest step**: F376 in the next helper a profile shows (`set_named` for `map.size`-style writes, or the
generic `get`), with the same before/after checks.

## Progress
- 2026-10-08: task created from F373 (user: "go with F373 and let's check it's a real improvement"). 70
  `test_object` sites, 72 casts to `$object`, all in backend helpers (1 in `lib.rs`), so code size changes little.
- 2026-10-08: first version (all 10 final subtypes tested exactly in `test_object`, `cast_object` = `br_on_cast` chain
  over them, 64 local casts replaced): knucleotide 5.51 → 5.26 s best of 5 interleaved (−4.5%), output same. But a
  plain-object micro (`props.js`: reads/writes on plain objects, class method calls, arrays; 3M iterations)
  regressed: reads 415 → 690 ms (+65%), methods 500 → 760 ms (+50%), arrays equal. An exact final-type test is
  inline but ~1.5 ns here, and that version added ~20 per property access. Reverted. Next: one combined chain
  (function, array, map, then `br_on_cast (ref $object)`, inline for a plain object) replacing test + cast in the
  hot helpers only (`get_named` first): fewer checks than today for plain objects, no libcall for a `Map`.
- 2026-10-08: second version: `runtime::object_or` (one test-and-cast chain: struct, number/oddball out, br_on_cast
  function/array/map/object) in `get_named` only. Validates; knucleotide best 5.71 → 5.16 s (−9.6%, 4 rounds,
  output same); props.js reads equal or better (best 372 → 358), methods (meth.js, 12 rounds) median 752 = 752;
  array/function method micro unchanged. Full fixture suite 247 pass, 2 known sandbox fails; cargo test ok; perf: no
  `is_subtype` in knucleotide. Docs, follow-ups F376, F377. Done.
- 2026-10-09: replay on a-03 (user): cpu_steady 21.00 s, 38.32 ms CPU per event, 139 exact / 1 unmatched (= T90). Feasibility doc; F377 ticked.
