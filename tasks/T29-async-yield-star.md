# T29: `yield*` in async generators

Status: todo (proposed) | Depends on: T20 (async generators), T18 (sync `yield*`)

## Goal
`yield* v` inside an `async function*` works like node (F115). Today it's rejected ("`yield*` in an async
generator", `crates/lower/src/lib.rs`, the `YieldExpression` case). It is the largest single test262 cause left:
1,128 tests (language/statements/for-await-of 354, statements/class 266, expressions/class 266,
expressions/async-generator 119, statements/async-generator 65, expressions/object 58). About 570 of them test
`yield*` itself (`yield-star-*`); the rest use it on the way and have never run.

## Semantics (ES2025 14.4.14, `yield*` with generatorKind async)
- `GetIterator(v, async)`: `v[Symbol.asyncIterator]`; if it's `undefined`/`null`, `v[Symbol.iterator]` wrapped
  with CreateAsyncFromSyncIterator (each result's `value` awaited; a rejected value closes the sync iterator).
  A TypeError if neither is callable, or if the method's result isn't an object.
- Loop, starting with `received = undefined`, mode `next`:
  - `next`: `r = Await(Call(next, it, [received]))`; not an object → TypeError.
  - `throw`: if the inner iterator has `throw`, `r = Await(Call(throw, it, [received]))`; otherwise close the inner
    iterator (`AsyncIteratorClose`, awaiting `return`) and throw a TypeError ("The iterator does not provide a
    'throw' method").
  - `return`: if it has no `return`, `received` is awaited and returned from the generator; otherwise
    `r = Await(Call(return, it, [received]))`, and `done` → return `r.value` from the generator.
  - `done` → the `yield*` expression's value is `r.value` (mode `next`/`throw`).
  - Otherwise `received = yield IteratorValue(r)`: an async generator re-wraps the value (unlike a sync `yield*`,
    which passes the result object on as is), so the outer consumer never sees the inner result object, and its
    `value` isn't awaited again (AsyncGeneratorYield without Await, spec note).
- Each `Await` is a real suspension of the async generator (the job-queue order is checked by many tests).

## Plan (slices)
1. **Lowering**: in `generators.rs`, an async variant of `yield_star`: open with `AsyncIterOpen` (already used by
   `for await`: async iterator, else the sync one with values awaited), then the loop above, with each step's result
   awaited (`Suspend { awaiting: true }`, as `await` does in an async generator) and each non-done value yielded
   through the async generator's normal `yield` (not `YieldDelegate`). New intrinsics only where the existing ones
   don't fit (probably one "call `next`/`throw`/`return` by mode, or report a missing method" step like
   `DelegateStep`, without its sync-only parts). Fixture `tests/fixtures/generators/async-yield-star.js`: delegation
   to an async generator, to a sync iterable (values awaited), `return()`/`throw()` forwarding, a missing `throw`,
   `for await` over it, ordering against other promise jobs.
2. **test262**: rerun the tests whose reason was "`yield*` in an async generator" (1,128, ~8 min at -j 4); record new
   causes as follow-ups; update the baseline and `tests/test262/HISTORY.md`.

## Read first
- `crates/lower/src/generators.rs` (`yield_star`, `await`), `crates/lower/src/lib.rs` (`YieldExpression`, the
  rejection), `crates/ir/src/lib.rs` (`DelegateOpen`/`DelegateStep`, `AsyncIterOpen`/`AsyncIterNext`/
  `AsyncIterReturn`, `SuspendKind`), `crates/ir/src/suspend.rs` (how an async generator suspends).
- T20's Notes / Result, slice 1 (async generators, `for await`); T18's Notes / Result (`yield*`).

## Tests
The new fixture with `scripts/diff.sh` (and `tests/fixtures/generators`, `tests/fixtures/async`), `cargo test`,
`wasm-tools validate` + one wasmtime run; test262 as in slice 2.

## Checklist
- [ ] Slice 1: lowering + fixture
- [ ] Slice 2: test262 rerun, follow-ups, baseline

## Notes / Result

## Progress
- 2026-09-27: task written as a proposal after T28 (largest remaining test262 cause). Not started.
