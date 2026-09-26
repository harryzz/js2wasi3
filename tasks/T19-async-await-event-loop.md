# T19: async/await, the P3 event loop and timers

Status: done | Depends on: T18

## Goal
`async function` and `await` built on T18's transform. Top-level `await`. The full callback event loop (D3): drain microtasks,
then `WAIT` on the waitable set, and in the callback settle the matching promise and loop again. `setTimeout` via
`wasi:clocks/monotonic-clock@0.3.1#wait-for` (async-lowered subtask).

## Read first
`probes/cmd.wat` (WAIT/callback codes, subtask state packing), `probes/cmd2.wat`.

## Acceptance
- [x] Fixtures `async/*.js` (ordering, await in loops, rejected await in try/catch, two concurrent timers) match node.
- [x] Two concurrent 50 ms timers finish in about 50 ms, not 100 ms.

## Notes / Result
**Done 2026-09-26.** Async functions, `await` (also at top level), and timers on the P3 callback event loop.

What changed:
- IR: `FnKind::Async`, `Intrinsic::NewAsync { resume, slots }`, `AsyncStart`, `Timer(k)` (+ `TIMER_*` names);
  `ir::suspend` also transforms `is_async` functions (no implicit first suspend; the start function makes the frame
  and returns `AsyncStart(frame)`); `ir::validate` lets `main` return a `jsval` (top-level `await`).
- Lowering: `async_sig`; `await` = `Suspend { Await }` + the throw check (no return path); a top-level `await`
  makes `main` async; closures of async functions are `FnKind::Async`. Rejected: exported async functions (T20),
  async generators and `for await` (later), timer functions in libraries (T20).
- Backend: `asyncfn.rs` (new): a frame is a `$generator` that never reaches JS, its last frame slot holds the
  promise; `Step` resolves / rejects it or awaits (`PromiseResolve` + `promise::Helper::AwaitThen`, the frame
  itself as both handlers, no closures); `promise_reaction_job` resumes a frame handler (fulfil → next, reject →
  throw), so `await` of a native promise takes one tick like V8. `[AsyncFunction: f]` in `console.log`.
- Timers: `timers.rs` (new): `$timer` (type 21, the Timeout object), `setTimeout`/`setInterval`/`clearTimeout`/
  `clearInterval` (node's delay clamp and callback TypeError, via the new shared `promise::Helper::CheckCallback`),
  a list in creation order, `RunDue` (one pass: due timers by deadline then creation, jobs drained and the
  unhandled-rejection check after each callback: F66). Deadlines follow node: the current time in whole ms plus the
  delay.
- Host (`host.rs`): imports `monotonic-clock` `now` / `[async-lower]wait-until`, `[subtask-drop]`,
  `[subtask-cancel]` (fixed list; the command world imports `wasi:clocks/monotonic-clock@0.3.1`). `Loop`: while
  timers are active, one `wait-until` subtask for the earliest deadline (re-armed only if an earlier one appears;
  a stale one leaves the set and is cancelled), pump the streams, `WAIT`; no timers: `Flush` as before. `Tick`
  (the wait ended) runs the timers in a `try_table`: an uncaught exception or unhandled rejection in a timer ends
  the program like node (F35).
- Probe 8 `probes/timers.wat` (in `probes/run.sh`, TOOLS.md): concurrent waits overlap; `subtask.cancel` needs the
  subtask out of its waitable set first.
- Fixtures `tests/fixtures/async/`: `ordering`, `control`, `top-level`, `top-level-reject` (.uncaught),
  `unhandled` (.uncaught), `timers`, `timer-throw` (.uncaught), `timer-unhandled` (.uncaught). Tests:
  `suspend_transform_async` (ir), `concurrent_timers_overlap` (CLI, the timing acceptance), `unsupported.rs`.

Commands run: `cargo test` (63 tests, all ok, 3× in a row), `cargo clippy --all-targets` (clean), `cargo fmt`,
`scripts/diff.sh` (61 passed, 0 failed, 9 skipped), `wasm-tools validate` + `wasmtime run -Sp3` on the smoke tests.
Timing: two concurrent 50 ms timers ≈ 51 ms over the no-timer baseline; 2 × 300 ms concurrent 314 ms, sequential
624 ms (baseline 11 ms).

Testing note: node refreshes its clock when a timer is made, so the order of near-tied deadlines (or of a timer made
in a callback vs. others due soon) depends on how fast node runs. Timer fixtures keep such deadlines far apart.

Unsupported / deferred (Follow-ups F72–F77): async generators and `for await` (F72); timer list scan and async
frame costs (F73); Timeout objects, `setImmediate`, `process.nextTick` (F74); commands always import the clock
(F75); an unsettled top-level `await` (F76); `TASK_CANCELLED` still ignored (F26). String delays trap (F20).

Next smallest step (T20): async exports/imports at the WIT boundary; F65 (jobs in library exports) and F11
(library init code) come with it.

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

## Progress
- 2026-09-26: started. Tools unchanged (check-tools ok). Baseline: cargo test 61 ok, diff.sh 53 passed / 0 failed / 9 skipped.
- 2026-09-26: design: `async function` = generator-like frame (`$generator` struct reused, never escapes) + a
  promise in an extra frame slot; `await v` = PromiseResolve + a raw PerformPromiseThen whose handlers are the frame
  itself (no closures); `reaction_job` resumes a frame handler (fulfil → next, reject → throw). Timers: a sorted
  list in the runtime, one `wait-until` subtask per armed deadline (libuv-style: run all due timers in (deadline, id)
  order, drain microtasks after each), `subtask.cancel` for stale waits at exit.
- 2026-09-26: probe 8 (`probes/timers.wat`, added to `probes/run.sh`): two concurrent 50 ms `wait-until`s finish in
  ~60 ms; `subtask.cancel` traps unless the subtask first leaves its waitable set. TOOLS.md updated.
- 2026-09-26: async/await implemented: IR `FnKind::Async`, `Intrinsic::NewAsync`/`AsyncStart`; `ir::suspend` handles
  `is_async` (start fn returns `AsyncStart(frame)`, no implicit first suspend); lowering: `async_sig`, `await` =
  `Suspend{Await}` + throw check (no return path), top-level `await` makes `main` async (`run` drops its result);
  backend `asyncfn.rs` (New/Start/Step), `promise::Helper::AwaitThen` (unfiltered handlers), `reaction_job` resumes
  frames; `[AsyncFunction: f]`. Smoke test (ordering vs promise ticks, throw → reject, await rejected in try/catch,
  async arrow) identical to node.
- 2026-09-26: fixtures `tests/fixtures/async/{ordering,control,top-level,top-level-reject,unhandled}.js` all PASS
  (diff.sh). `ir::validate` now allows `main` to return a jsval. `switch` is unsupported in general (not T19).
- 2026-09-26: timers design: `$timer` (subtype of `$object`, returned to JS as the Timeout) in a runtime list;
  node-like cached loop time (`G_LOOP_NOW`, set at run start and at each wake-up), deadline = loop now + clamp(ms);
  one outstanding `wait-until` for the earliest deadline (cancel + re-arm if an earlier one appears); a pass runs
  due timers in (deadline, id) order, draining microtasks + the unhandled check after each (F66); an uncaught
  exception in a timer ends the program (F35). Timer functions are rejected in libraries (T20).
- 2026-09-26: timers implemented: `Intrinsic::Timer(k)` (lowering rejects in libraries), backend `timers.rs`
  (`$timer` type 21, set/clear code, Remove/Next/RunDue), `promise::Helper::CheckCallback` (shared with
  queueMicrotask), host imports `now`, `[async-lower]wait-until`, `[subtask-drop]`, `[subtask-cancel]` (fixed list,
  every command's world now imports `wasi:clocks/monotonic-clock@0.3.1`), host `Loop`/`Tick`/`Disarm`. Smoke test
  (timeouts with args, clearTimeout, setInterval + clearInterval, 0 ms, microtasks between timers) identical to node.
- 2026-09-26: acceptance timing: cargo test `concurrent_timers_overlap` (crates/js2wasi3/tests/build.rs) ok. Manual:
  two concurrent 50 ms timers 63 ms total vs 12 ms for a no-timer program (≈51 ms); 2×300 ms concurrent 314 ms,
  sequential 624 ms (base 11 ms).
- 2026-09-26: node refreshes its clock when a timer is inserted (ms steps), so a deadline is now `now()` truncated to
  ms + delay (intervals: from just before the callback); `G_LOOP_NOW` is only the due check of a pass. Fixture
  `timers.js` restructured into phases with far-apart deadlines (near ties depend on node's own speed); passes
  4/4 runs. `timer-throw.js`, `timer-unhandled.js` (.uncaught) pass. String delays trap (ToNumber of a string, F20).
- 2026-09-26: unit test `suspend_transform_async` (ir), unsupported.rs updated (exported async → T20, async generators, timers in libraries, `for await` → later). All ok.
- 2026-09-26: done. Full `cargo test` 63 ok (timing test made relative after one flaky run under load), clippy clean, fmt, `scripts/diff.sh` 61 passed / 0 failed / 9 skipped. Notes / Result, README (status, F26/F35/F66/F71, new F72–F76), design.md D3 + Testing updated. Not committed yet.
