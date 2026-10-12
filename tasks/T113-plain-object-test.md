# T113: a plain-object test that never calls `is_subtype` (root R1)

Status: in progress | Depends on: T112 | Local (+ a-03 only after asking)

## Goal
Root R1 of `docs/root-causes.md`: `$object` isn't final, so wasmtime compiles `ref.test`/`ref.cast (ref $object)`
inline only for an exact `$object`; any other value (a string, a number box, a Map, an array, a function…) calls
the `is_subtype` libcall. T112 measured what that costs T110's inline sites (`tests/bench/micro/property-paths.js`,
T109 → T110): a non-object receiver +18 ns, a Map method call +31 ns, and the counts show knucleotide's 17.5M
subtype misses (+7.2%). Every later inline object path (T110, F359, T111) hits the same wall.

The goal is a test "is this a plain object?" that costs one inline compare on the hit and never calls
`is_subtype` on a miss, so a miss costs no more than the old path (T112's rule).

## Idea (to measure, not decided)
- Plain objects become a **final** type `$plain` (a subtype of `$object` with no new fields): `ref.test (ref
  $plain)` is one inline type-id compare, for every value.
- The catch (why T109 avoided a subtype): generic code casts to `(ref $object)`; for a `$plain` that is no longer
  an exact `$object`, so those casts would call `is_subtype`. 187 sites cast or test `$object` directly
  (`as_obj`, `test_object`, `ref.cast`/`ref.test` of `runtime::object()`), 16 go through `object_or`. Each must
  test `$plain` first (one compare), then the other final kinds, then `$object`.
- Alternative to compare: keep `$object` exact for plain objects and make the inline sites test with
  `br_on_cast` to the final kinds first only on the miss path (T112's laptop trial: 3 tests before the cast
  halved T110's gain: measure where the tests go, not only how many).

## Plan
- [ ] Count (`build --count`): at the 187 + 16 sites, which receiver kinds arrive (plain / subtype / not an
  object), on the 18 programs.
- [ ] Micro-benchmarks first: extend `tests/bench/micro/` with the generic paths a `$plain` type would touch
  (`Object.keys`, spread, `for-in`, `JSON.stringify`, method calls, `instanceof`), hit and miss.
- [ ] Prototype the cheaper option on a branch; `scripts/gate.sh` against T112's commit: nothing slower, misses
  not dearer than before.
- [ ] Then T110's inline sites on top (F409), gate again.

## Acceptance
- `scripts/gate.sh`: no program slower (2% on minimums), no micro case slower (1 ns and 3%); T110's misses no
  dearer than T109's paths; test262 no changes (a-03, ask).

## Progress
- 2026-10-12: proposed by T112 (step 6) from `docs/root-causes.md`; user agreed ("Yes 1 and 2").
