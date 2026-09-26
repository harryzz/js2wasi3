# T18: Suspension transform: generators as state machines

Status: todo | Depends on: T17

## Goal
An IR pass (D3) that turns a function containing `Suspend` points into a resumable frame: a GC struct holding the live locals,
a state index and a dispatch on entry. Use it for `function*`: `next/return/throw`, `yield*`, and try/finally around `yield`.

## In scope
- The pass lives in `crates/ir` and is backend-neutral. Write down its algorithm (liveness, how `try` regions are split) in the crate docs.
- Generator objects work with T15's iterator protocol.

## Out of scope
async (T19). The stack-switching alternative (T22).

## Acceptance
- Fixtures `generators/*.js` match node, including `return()` running `finally` blocks.

## Notes / Result

## Handoff (written 2026-09-26 at the end of the T17 session)

**State:** `main` at `005a3a0` (T17), clean tree. T01–T17 done: closures (T14), arrays/iteration (T15), classes,
accessors, `new` (T16), promises + job queue (T17). Generators are rejected in the lowering
(`crates/lower/src/lib.rs` ~826, "generator function (planned: T18)"); the backend also refuses any function with
`flags.is_generator`/`is_async` or an `Expr::Suspend` (`crates/backend/src/lib.rs` ~427/471, `check_supported`).

**Run first (baseline, ~1 min):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'      # all ok, 60 tests in total
scripts/diff.sh                              # 49 passed, 0 failed, 9 skipped
```
New fixtures go in `tests/fixtures/generators/` (`scripts/diff.sh tests/fixtures/generators`); node is the oracle, an
uncaught ending needs an empty `x.uncaught`. `console.log(...spread)` isn't supported: call `console.log` directly.

**Read first:** `docs/design.md` D2 (finally = completion dispatch) and D3 (why a state machine), T12's Notes (finally
lowering), T15's Notes (iteration: `$iter`, protocol path), T17's Notes; then:
- `crates/ir/src/lib.rs` — `Stmt` (Block/Loop/If/Break/Continue/Return/Try{catch only}/Throw), `Expr::Suspend { kind,
  arg }` (placeholder, printed as `yield(e)`), `FnFlags { is_async, is_generator }`, `Local { ty }` (locals are typed:
  `F64`, `I32`, `Bool`, `JsVal`). `ir::validate`, `ir::print`, `ir::throws` (`may_throw`).
- `crates/lower/src/lib.rs` — `lower_body` (`prologue`/`finish`, `Hidden { self_, this }` params), `try_stmt` and
  `struct Finally` (~496, ~2216): `finally` is already gone in the IR — it's a completion `I32` local + one copy of the
  body + `if` chains. So a `yield` inside `try`/`finally` sits inside an IR `Try` (a Wasm `try_table`) or inside the
  dispatch code.
- `crates/lower/src/closures.rs` — captures/cells; a generator's frame must hold the locals that live across a
  `yield` (captured variables are already cells, so they're just references).
- `crates/backend/src/iter.rs` — `$iter` protocol path: any object with `next` works with `for-of`/spread/destructuring;
  `%ArrayIteratorPrototype%` (`Builtin::ArrayIterator` in `object.rs::proto`, real methods via `func::Helper::New`) is
  the pattern for `%GeneratorPrototype%` (`next`/`return`/`throw`, `[Symbol.iterator]` returns `this`).
- `crates/backend/src/promise.rs` — the model for native function objects with an `env` (`native(...)`), `try_catch`
  pattern, lazy globals. T19 will resume async frames from reaction jobs (a `$reaction` with `derived` null is already
  handled in `promise_reaction_job`).

**Decisions to make first (log them in Progress):**
1. **Where the pass runs.** The task says `crates/ir`, backend-neutral: rewrite a generator `Function` into (a) a
   "start" function that allocates the frame and returns the generator object, (b) a "resume" function
   `(frame, mode: next/throw/return, value) -> result` with a `state` dispatch. Decide how the frame is represented in
   the IR (new IR type/intrinsics for frame field get/set, or spill every live local to a `$vals`-like array with
   f64 boxing). A per-generator GC struct is fastest but needs backend type generation; a `$vals` + boxing is simplest.
2. **Re-entering the middle of structured control flow.** Wasm can't jump into a `loop`/`block`/`try_table`. Usual
   approach: a dispatch at function entry with `br_table` into a flattened state machine, or keep structure and guard
   each region with `if state == k` (re-enter nested loops/ifs by conditions). `try` regions containing a `yield` must
   be split: after resuming, re-enter a fresh `try_table` covering the rest of the region (catch → same handler).
3. **Liveness:** which locals to save at each `yield` (all locals is correct and simplest first; document it).
4. **`return()`/`throw()` semantics:** resuming with a throw = throw at the `yield`; `return(v)` = a return completion
   at the `yield`, which must run enclosing `finally` code (the completion dispatch from T12 makes this "set completion
   = return, jump to the finally copy"). `yield*` delegates `next/throw/return` to the inner iterator.
5. **Generator object layout:** `$object` with the frame under a private symbol (like iterator objects' `$iter`,
   `G_SYM_STATE`) or a new `$generator` subtype (like `$promise`). States: suspended-start, suspended-yield,
   executing (re-entrant `next` → TypeError "Generator is already running"), completed.

**Pitfalls seen so far:**
- A Wasm `if`/`block` can't consume values from the outer stack: build the value inside the block or use a local
  (hit three times in T17). Use `wasm-tools validate -f all` on `build --core` output to find the function by index.
- `object::Helper::Throw` has type `[] -> []`: add `unreachable` if a value is expected after it.
- The IR fixtures' snapshots (`crates/lower/tests/snapshots.rs`) change if lowering output changes: `UPDATE_SNAPSHOTS=1`.
- Keep `crates/lower/tests/unsupported.rs` expectations in sync when a rejection message changes.
- `Object`/`Array` globals aren't function objects yet (F47), so `CtorName` is null for plain objects/arrays.
