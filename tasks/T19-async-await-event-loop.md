# T19: async/await, the P3 event loop and timers

Status: todo | Depends on: T18

## Goal
`async function` and `await` built on T18's transform. Top-level `await`. The full callback event loop (D3): drain microtasks,
then `WAIT` on the waitable set, and in the callback settle the matching promise and loop again. `setTimeout` via
`wasi:clocks/monotonic-clock@0.3.1#wait-for` (async-lowered subtask).

## Read first
`probes/cmd.wat` (WAIT/callback codes, subtask state packing), `probes/cmd2.wat`.

## Acceptance
- Fixtures `async/*.js` (ordering, await in loops, rejected await in try/catch, two concurrent timers) match node.
- Two concurrent 50 ms timers finish in about 50 ms, not 100 ms.

## Notes / Result

## Handoff (written 2026-09-26 at the end of the T18 session)

**State:** T01–T18 done. T18 added the suspension transform: `crates/ir/src/suspend.rs` (algorithm in its module
docs), run at the end of `lower::lower`. Async functions are still rejected in the lowering (`declare_function`,
`lower_nested`: "async function (planned: T19)") and by the backend's `check_supported` (`is_async`, `Suspend`).

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'      # all ok, 61 tests in total
scripts/diff.sh                              # 53 passed, 0 failed, 9 skipped
```

**Read first:** `docs/design.md` D3 (incl. the T17/T18 notes), T18's Notes / Result, `crates/ir/src/suspend.rs`,
`crates/lower/src/generators.rs` (`suspend`: how a `Suspend` is followed by the throw/return checks),
`crates/backend/src/generator.rs` (`Helper::Resume`: status, `yielded`, the `try_table` around `call_ref $resume`),
`crates/backend/src/promise.rs` (`promise_reaction_job`: a `$reaction` with `derived` null is already handled).

**Suggested shape (F71):**
- An `async function` could lower exactly like a generator (flag, `jsval` result, implicit first suspend, each
  `await x` = `Suspend { Await, x }` + the throw check; `return v` resolves). The start function would make the
  promise and run the body synchronously up to the first `await` (no implicit first suspend then: the body starts
  at once). `ir::suspend` needs the `Await` kind (today `unreachable!`) and a way for the start function to return
  the promise instead of the frame object.
- Resuming: `await v` = PromiseResolve(v) + PerformPromiseThen with native fulfil/reject functions whose `env` holds
  the frame; they call the resume function with mode next/throw. Returning / throwing out of the resume function
  settles the async function's promise.
- F35/F66 (catch `$js` in the event-loop callback; unhandled-rejection check after every macrotask) are also T19's.

**Pitfalls seen in T18:**
- Wasm `select` returns its *first* operand when the condition is non-zero; T18 had three bugs from pushing the
  operands the other way round.
- Break out of a `Seq` is only legal before `ir::suspend` flattens it (the `return` path of a `yield`); the
  validator runs after the pass.
- The fixture's `console.log(...spread)` and `Math` aren't supported; built-in array methods like `map` are
  sentinels that trap when read (F49).
