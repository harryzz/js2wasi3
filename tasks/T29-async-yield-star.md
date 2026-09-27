# T29: `yield*` in async generators

Status: done | Depends on: T20 (async generators), T18 (sync `yield*`)

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
- [x] Slice 1: lowering + fixture
- [x] Slice 2: test262 rerun, follow-ups, baseline

## Notes / Result
**What changed**
- `crates/lower/builtins/asyncgen.js` (new, in `builtins::PRELUDE`): `$$asyncDelegateOpen` (GetIterator(v, async);
  a sync iterator is wrapped by `$$asyncFromSync`, CreateAsyncFromSyncIterator as a real object: each value awaited
  through `Promise.resolve(v).then(…)`, a rejection closes the sync iterator unless done, a missing `throw` closes it
  and rejects with a TypeError), `$$asyncDelegateNext`/`Method`/`Call`/`Close`/`Result`/`NoThrow` (the calls and
  node's TypeError messages). Rooted (`builtins::ASYNC_DELEGATE`) only when a `yield*` is in an async generator.
- `crates/lower/src/generators.rs`: `Lowerer::async_yield_star` (the spec loop: see its doc comment) and
  `prelude_call` (a direct prelude call with JS-value arguments); `lib.rs` uses it instead of rejecting.

**Tests**: fixture `tests/fixtures/async/async-yield-star.js` (delegation to async and sync iterables, `next(x)`,
`return()`/`throw()` forwarding with `finally`, a missing `throw`/`return`, errors, and the job order of delegating,
direct and sync-wrapped generators side by side): node = js2wasi3 on the first run. `unsupported.rs` lowers it now.
`cargo test` ok; `diff.sh` async + generators: 18 pass; `wasm-tools validate` ok.

**test262**: the 1,128 tests rejected for this reason (606 s at -j 4): 1,127 pass, 0 regressions. The one failure,
`async-generator/yield-star-async-from-sync-iterator-inaccessible.js`, reads
`(async function*(){}).constructor` (no AsyncGeneratorFunction constructor: F106). Baseline: 20,167 pass,
1,761 fail, 2,871 unsupported, 23 timeout, 1,808 skip: **81.2%** of 24,822 run (was 76.7%).

**Unsupported / differences**: CreateAsyncFromSyncIterator calls `then` on the wrapper promise through the property
(a patched `Promise.prototype.then` sees it; the spec uses PerformPromiseThen). No test in the subset shows it.

**Next smallest step**: by test262 count: wrapper objects (F106, ~800), classes (F58, ~580: private accessors,
computed instance keys), `defineProperty` on arrays (F145, 202).

## Progress
- 2026-09-27: task written as a proposal after T28 (largest remaining test262 cause). Not started.
- 2026-09-27: approved; started slice 1 (lowering).
- 2026-09-27: slice 1 implemented. Prelude `builtins/asyncgen.js`: `$$asyncDelegateOpen` (GetIterator async, else the sync iterator wrapped by `$$asyncFromSync` = CreateAsyncFromSyncIterator as a real object: value awaited via `Promise.resolve(v).then`, rejection closes the sync iterator), `Next`/`Method`/`Call`/`Close`/`Result`/`NoThrow` helpers with node's messages; rooted (`builtins::ASYNC_DELEGATE`) only when a `yield*` sits in an async generator. Lowering `Lowerer::async_yield_star` (generators.rs): the spec loop with each inner result awaited, values yielded by a plain `Suspend(Yield)` (not awaited again), `throw`/`return` forwarding, missing `throw` → close + TypeError, missing `return` → value awaited twice, `return` resumption awaited first (a rejection continues as `throw`). Fixture `async/async-yield-star.js` (incl. job order of delegating/direct/sync-wrapped generators side by side) = node on the first run. `unsupported.rs` now lowers `yield* [1]` in an async generator. `cargo test` ok; `diff.sh` async + generators: 18 pass; validate ok.
- 2026-09-27: slice 2: rerunning the 1,128 test262 tests whose reason was `yield*` in an async generator (background, --update).
- 2026-09-27: rerun done: 1,127 of 1,128 pass (the other needs the AsyncGeneratorFunction constructor, F106); baseline 81.2%. T29 done.
