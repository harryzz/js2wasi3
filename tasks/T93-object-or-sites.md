# T93: `object_or` at the other hot object test + cast sites (F376)

Status: done (2026-10-09, cloud session) | Depends on: T92 | Cloud (test262 + replay on a-03: F378)

## Goal
T92 made `get_named` test and cast objects in one inline chain (`runtime::object_or`). ~69 other sites still do
`test_object` + `as_obj` (`ref.cast (ref $object)`), which calls wasmtime's `is_subtype` for every object that
isn't exactly `$object` (a `Map`, array, function, typed array…). Convert the sites that profiles show, keep plain
objects as fast as before.

## Read first
- `docs/design.md`: T92's note (why chains stay short).
- `tasks/T92-object-type-tests.md`: Notes and Progress (the reverted first version: +65% on plain-object reads).
- `crates/backend/src/runtime.rs`: `object_or`, `JsTest::test_object`; `crates/backend/src/object.rs`: `as_obj`.

## Plan
1. Measure where `is_subtype` costs: perf over the Benchmarks Game and Octane programs (`scripts/bench.sh`'s
   builds, `--profile=perfmap`): its share per program. perf can't unwind through the libcall trampoline (T91), so
   find the calling helpers from the programs' hot helpers that contain a `test_object` + cast.
2. Convert those helpers to `object_or` (or the same chain), one at a time, measuring the program that showed it
   and the plain-object micros (`tasks/T92-files/props.js`, `meth.js`) each time.
3. Tests: fixtures of the touched paths, full `scripts/diff.sh` once at the end, `cargo test`, validate, smoke.
   Full test262 + one replay on a-03 (F378).

## Acceptance
- [x] `is_subtype` share per benchmark before/after in Notes; plain objects unchanged.
- [x] Results = node (fixtures).
- [x] Docs (design.md), follow-ups (F379; F378 extended).

## Notes / Result
**Where `is_subtype` was left after T92** (perf, 15 s of each of the 18 benchmarks, `T93-files/subtype_sites.py`
lists hot helpers containing a `ref.test`/`ref.cast (ref $object)`): at most 1.4% anywhere (EarleyBoyer 1.4%,
RayTrace 1.1%, regexp/revcomp 0.3%, the rest ≤ 0.1%; knucleotide 0.0% since T92). zigbee2mqtt's last profile
(T79, before T89/T92) had ~3%; it can't be profiled here (F379).

**What changed** (three helpers):
- `construct_ordinary` (`func.rs`): the constructor's result tested with `test_object` instead of a bare
  `ref.test (ref $object)`: the usual result, `undefined`, isn't an exact `$object`, so every `new F()` paid the
  libcall.
- `instance_of_value` (`func.rs`): the instance tested and cast once with `runtime::object_or` (into `O`), no cast
  later: an array or function instance no longer pays the libcall.
- `set_named` (`object.rs`): `object_or` as in `get_named` (T92).
Plain objects take the same number of checks as before in all three.

**Measurements** (cloud, alternating T92 build vs T93 build):
- `is_subtype` share: EarleyBoyer 1.4% → 0.1%, RayTrace 1.1% → 0.4%.
- Octane scores (4 runs each, higher is better): EarleyBoyer 1,192 → 1,204 avg, RayTrace 371 → 377,
  Richards 300 → 305, DeltaBlue 403 → 390 (one outlier at 346): all within noise, as the ≤ 1.4% share predicted.
- Plain objects (`T92-files/props.js` reads + writes, 12 rounds): median 397 → 360 ms, min 345 → 336; methods
  (`meth.js`, 8 rounds): medians ~741 / ~756: unchanged within noise.
- Outputs: all fixtures and benchmarks the same as node (Octane prints a speed score, the only difference).

**Commands**: full `scripts/diff.sh -j 4`: 247 pass, 19 skip, 2 fail (the known sandbox ones: `dgram`, no IPv6;
`fetch-https`, proxy); `cargo test -p js2wasi3-backend` ok; `cargo test -p js2wasi3`: 30 pass, 4 fail
(`*_compose_with_wac`: `wac` isn't installed in this sandbox, TOOLS.md); the nested `library/scalars`,
`library/futures` fixtures fail through `diff.sh` on T91 too (not command fixtures; covered by `cargo test`).
Validate + smoke; perf.

**Not done**: the other ~66 `test_object` + cast sites: no benchmark shows them (≤ 0.4% left). zigbee2mqtt's
remaining share is unknown until a replay is profiled (F379). Full test262 (F378, with T92).

**Next smallest step**: F378 + F379 on the next a-03 run (one test262, one replay with `PERF=`).

## Progress
- 2026-10-09: task created from F376 (user: "write docs where need then start F376"). Docs check: T91/T92 are in
  design.md, HISTORY.md (T91), the feasibility doc; added F378 (full test262 for T92, none ran).
- 2026-10-09: `scripts/bench.sh -r 1` (all 18, small sizes, cloud): all SAME as node; nbody* 291 s (2,859×),
  mandelbrot* 117 s (683×) stood out. Checked for a T91/T92 regression: nbody 20,000 steps T90 8.9 s, T91
  7.5–8.8 s, T92 7.4–10.3 s alternating (first T92 run noisy): none. Long-standing (F334: property access and
  boxing in float loops).
- 2026-10-09: `is_subtype` per benchmark after T92: ≤ 1.4% (EarleyBoyer 1.4, RayTrace 1.1). Hot helpers with
  `$object` tests: `set_named`, `instanceof_value`, `construct_ordinary` (bare `ref.test` on the constructor's
  `undefined` result), `create_this` (tests `F.prototype`, a plain object: inline, left). Converted the first three.
  Validates; fixtures with instanceof/new/class pass (library/ nested ones fail on T91 too). EarleyBoyer 1.4 → 0.1%,
  RayTrace 1.1 → 0.4%; Octane scores and plain-object micros within noise. Full suite 247 pass; cargo test ok
  (wac tests: tool missing). Docs, F379. Done.
- 2026-10-09: a-03 (user): full test262 "No changes since the baseline" (0 regressions, T92 + T93); HISTORY.md;
  F378 ticked. Replay with `PERF=` (`t79/out-t93`): cpu_steady 21.7 s under perf (not comparable to 21.00 s),
  matching as T90 (139 exact, 1 unmatched). perf head (call graphs): `get_named` 8.0%, `find_own` 5.1% (from
  `own_get` ← `lookup` ← generic `get`), `find_own_named` 4.6%, `get` 4.5%, `call_value` 3.1%, GC forward 2.7%,
  `string_hash` 2.6% (inside `find_own` ← `get`: run-time keys hashed per access). `is_subtype` share pending
  (asked for a `-g none` report, F379).
- 2026-10-09: zigbee2mqtt perf (`-g none`, T93 replay): `is_subtype*` ≈ 2.6% (1.58 + 0.66 + 0.39; T79 ~3.4%),
  `get_interned_func_ref` ~0.1%. Benchmarks don't show these callers; asked the user to run `subtype_sites.py` on
  `p.wasm` (F379). Computed-key path (`get`/`find_own`/`own_get`/`string_hash`/`to_property_key`) ≈ 15%: F363 updated
  (string-hash caching doesn't need shapes).
