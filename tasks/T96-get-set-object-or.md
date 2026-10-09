# T96: the generic `get`/`set` without wasmtime's `is_subtype` (F379)

Status: done (2026-10-09, cloud session; full test262 on a-03: no changes) | Depends on: T95 | Cloud

## Goal
zigbee2mqtt's replay still spends ~2.4% in wasmtime's `is_subtype` (T94 profile). `subtype_sites.py` (fixed to
match functions by index and accept `(@name …)` headers) on a-03's `p.wasm` + `out-t94` lists the hot helpers with
a `ref.test`/`ref.cast (ref $object)`: `get` 4.9% (4 tests), `to_property_key` 1.4%, `to_string` 1.1%, `set` 0.6%
(5), `regexp_plain`, `to_number`, `js_add` 0.5%, `get_prototype_of` 0.4%. `get` serves `.length` of anything but
arrays and strings (`get_length`): typed arrays (Buffers) pay `test_object`'s fallback and `as_obj` each time.

## Plan
1. `get` and `set`: `runtime::object_or` instead of `test_object` + `as_obj` (as T92/T93).
2. The smaller helpers if a benchmark or micro shows them (a typed-array `.length` / `buf[k]` micro).
3. Measure: plain objects (`T92-files`), a Buffer/typed-array micro, Octane; tests: fixtures, full `diff.sh`,
   `cargo test`, validate, smoke; full test262 on a-03 (user: no replay).

## Acceptance
- [x] Before/after in Notes; results = node; test262 on a-03: no changes since the baseline.
- [x] Docs, follow-ups (F382, F383; F379 ticked).

## Notes / Result
**What changed**: the object test + cast in the generic `get` and `set` (`object.rs`) is `runtime::object_or`
(T92's one-chain test-and-cast); a new `runtime::object_then(s, v, body)` (body runs with the `(ref $object)` when
`v` is an object) replaces `test_object` + `ref.cast (ref $object)` in `to_string`, `to_number`, `js_add`, loose
equality (the same loop), `get_prototype_of`, `to_property_key` (`to_key`); `regexp_plain` uses `object_or`.
Plain objects take the same number of checks as before.

**Measurements** (cloud, best of 5 alternating, ms; outputs identical; node in brackets),
`T96-files/nonplain.js`: `arr[k] = v` 116 → 95 [20]; `fn[k]` 111 → 99 [58]; `arr[k]` 116 → 105 [22]; `"" + arr`
763 → 692 [85]; `buf.length` 568 → 529 [4]; `u8.length` 504 → 487 [2]; `String(map)`, `+date` unchanged. Plain
objects (`T92-files`): reads 346 → 332, methods 540 → 460, meth.js 654 → 645 (unchanged or better).

**Found**: a typed array's or a Buffer's `.length` costs ~250 ns (node 2–4 ns): `get_length` sends it to `get`,
which walks to `%TypedArray%.prototype` and calls its getter. zigbee2mqtt reads Buffer lengths constantly: F382.

**Commands**: full `scripts/diff.sh -j 4`: 247 pass, 19 skip, 2 known sandbox failures; `cargo test -p
js2wasi3-backend` ok; `cargo test -p js2wasi3` 30 pass, 4 `wac` tests (tool missing); validate + smoke.

**a-03**: full test262 no changes since the baseline (0 regressions). No replay (user).

**Not done**: Remaining `test_object` + cast sites not in the profile.

**Next smallest step**: F382 (typed-array `.length` fast path, guarded against a redefined or
deleted getter).

## Progress
- 2026-10-09: task created (user: F379). `subtype_sites.py` needed two fixes first (repeated names print as
  `$"#funcN name" (@name "name")`; matched by function index now).
- 2026-10-09: `get`/`set` with `object_or`; `object_then` for to_string, to_number, js_add (+ loose equality),
  get_prototype_of, to_key; regexp_plain `object_or`. Validates. nonplain.js: arr[k]=v −18%, fn[k] −11%, arr[k]
  −9%, "" + arr −9%, buf.length −7%; plain objects unchanged/better. Found: typed `.length` ~250 ns (F382). Full
  suite 247 pass; cargo test ok (wac missing). Docs; F379 ticked; F382, F383. Done (test262 pending on a-03).
- 2026-10-09: a-03 (user): full test262 no changes since the baseline; HISTORY.md; F383 ticked.
