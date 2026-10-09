# T93: `object_or` at the other hot object test + cast sites (F376)

Status: in progress (started 2026-10-09, cloud session) | Depends on: T92 | Cloud (+ a-03 for test262/replay)

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
- [ ] `is_subtype` share per benchmark before/after in Notes; plain objects unchanged.
- [ ] Results = node (fixtures).
- [ ] Docs (design.md), follow-ups.

## Notes / Result
(to be written)

## Progress
- 2026-10-09: task created from F376 (user: "write docs where need then start F376"). Docs check: T91/T92 are in
  design.md, HISTORY.md (T91), the feasibility doc; added F378 (full test262 for T92, none ran).
