# T17: Promises and the microtask queue (no suspension yet)

Status: done | Depends on: T16

## Goal
`Promise` (constructor, resolve/reject, `then/catch/finally`, `Promise.resolve/all`), `queueMicrotask`, and the job queue.
In the command world, drain the queue after top-level code and before `task.return`. Unhandled rejections go to stderr and return `err`.

## Read first
`docs/design.md` D3 (the event loop shape).

## Acceptance
- [x] Fixtures `promises/*.js` (ordering tests) print the same order as node.

## Notes / Result
**What changed**
- `crates/backend/src/promise.rs` (new): the `Promise` constructor (lazy function object, `new` only; a subclass
  instance made by `new` gets the subclass prototype), CreateResolvingFunctions, resolve (cycle TypeError, thenables via
  PromiseResolveThenableJob), fulfill/reject, PerformPromiseThen, `then`/`catch`/`finally`, `Promise.resolve`/`reject`/
  `all`/`allSettled`/`race` (iterator protocol, closing on a throwing `then`), `queueMicrotask` (node's
  ERR_INVALID_ARG_TYPE text), the job queue and its drain, unhandled-rejection tracking and node's report text.
- `runtime.rs`: types `$promise` (final subtype of `$object`) and `$reaction` (TYPES 19); globals for the constructor,
  `queueMicrotask`, the queue (head/tail) and the rejected list. `Builtin::Promise` (prototype slot, `then`/`catch`/
  `finally` real methods, `constructor`).
- `host.rs`: `run` = `main`, drain the jobs (inside the same `try_table`: an exception in a `queueMicrotask` callback
  is uncaught and stops the rest, like node), then report the first still-unhandled rejection as
  `Uncaught <error text>` or `Uncaught UnhandledPromiseRejection: … reason "<v>".` and exit 1. New `Report` helper.
- `inspect.rs`: `Promise { 1 }`, `Promise { <pending> }`, `Promise { <rejected> 2 }` (then own properties).
- `ir`/`lower`: `Intrinsic::PromiseCtor` / `QueueMicrotask` for the unshadowed globals; everything else (`new`, method
  calls, `instanceof`) is the generic machinery from T14–T16.
- Fixtures `tests/fixtures/promises/`: `order`, `chains` (promise/thenable resolution ticks, finally, pass-through,
  cycle, several handlers), `combinators`, `errors` (TypeError texts, subclass, handled later), `inspect`,
  `interleave`, and three `.uncaught` ones (`unhandled`, `unhandled-value`, `microtask-throw`).

**Commands**: `cargo test` (all pass), `cargo clippy --all-targets` (clean), `scripts/diff.sh` (49 passed, 0 failed,
9 skipped), `wasm-tools validate -f all` on `promises/order.js`'s core module, `wasmtime run -Sp3` of the fixtures.

**Unsupported / deferred**: species and Promise subclassing beyond `new` (F64), `Promise.any`/`withResolvers`/
`Symbol.toStringTag` (F64), jobs in library exports (F65), the unhandled check after later macrotasks (T19, F66),
node's `code` on the queueMicrotask TypeError and function reasons' source text (F67), the promise runtime in every
command (F68).

**Next smallest step**: T18 (generators via the suspension transform); T19 then resumes async functions from reaction
jobs (a `$reaction` with no derived promise is already supported by the job code).

## Progress
- 2026-09-26: started; status in progress.
- 2026-09-26: design: `$promise` (final subtype of `$object`: state, result, reactions, count, handled) and `$reaction`
  types (TYPES 19); one job queue in globals (4 slots/job), drained by `run` after `main`; unhandled rejections recorded
  and reported after the drain (node's text). `Promise`/`queueMicrotask` are lazy function objects
  (`Intrinsic::PromiseCtor`/`QueueMicrotask`); everything else is generic calls. New `crates/backend/src/promise.rs`.
- 2026-09-26: backend + lowering written; fixed two "if consumes the outer stack" validation errors;
  `promises/order.js` passes diff.sh (validate ok, wasmtime run ok).
- 2026-09-26: fixtures chains/combinators/errors/inspect/interleave + 3 uncaught ones pass; console.log of promises
  like node; `Object`/`Array` not being function objects needed a fallback for "an instance of X".
- 2026-09-26: end of task: cargo test ok, clippy clean, diff.sh 49 passed / 0 failed / 9 skipped. hello.js grows
  38.5 KB → 41.3 KB (promise runtime always linked, F68). Status done.
- 2026-09-26: user approved the commit ("T17: promises + microtasks", 005a3a0), no attribution lines, not pushed. Handoff for T18 written in its task file. Next: T18.
