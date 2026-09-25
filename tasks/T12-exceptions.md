# T12: Exceptions with native Wasm EH

Status: todo | Depends on: T11

## Goal
Implement D2: `throw`, `try/catch` (with and without a binding), `finally` (including break/continue/return inside it),
nesting and rethrow, plus the internal TypeError/ReferenceError helpers used by the runtime.

## In scope
- Lower `finally` in the IR (duplicate or dispatch). Use `try_table` + `exnref`, with one tag carrying a `jsval`.
- Boundary policy: a library export returns `result<T, js-error>` (`record js-error { name: string, message: string, stack: option<string> }`).
  A command prints `Uncaught <name>: <message>` to stderr and returns `err`.

## Acceptance
- Fixtures `exceptions/*.js` (≥8 cases, including finally+return and a throw from a nested call) match node.
- A Wasm trap, such as an explicit `unreachable` in a test helper, is **not** caught by JS `catch`.

## Handoff (written 2026-09-25 at the end of the T10–T11 session)

**State:** `main` at `61a6c08` (T11), clean tree. T01–T11 done. Both milestones are reached: libraries run with
`wasmtime run --invoke`, and a file without exports is a `wasi:cli/run@0.3.1` command (`console.*` → stdout/stderr).

**Run first (baseline, ~1 min):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'      # all ok, 49 tests in total
scripts/diff.sh                              # 3 passed, 0 failed, 6 skipped (libraries)
(cd probes && ./run.sh)                      # step 1 (p.wat) is a GC + EH try_table/catch example
```
Testing layers and the harness: `docs/design.md`, "Testing". New fixtures for this task go in `tests/fixtures/exceptions/`
and are checked with `scripts/diff.sh tests/fixtures/exceptions` (node is the oracle: no `.out` files needed).

**Where things are today:**
- IR (`crates/ir/src/lib.rs`): `Stmt::Try { body, catch_local: Option<LocalId>, catch_body }` and `Stmt::Throw(Expr)` exist as
  placeholders. `finally` is meant to be lowered away before the IR (design D2). The validator checks them structurally only.
  The printer prints them.
- Lowering (`crates/lower/src/lib.rs`): `throw` and `try` are rejected by the catch-all in `stmt()`
  (`statement \`ThrowStatement\``, planned "later"). `has_value_return` already looks into try/catch/finally blocks.
  `resolve()` rejects a TDZ read in `main` with planned T12. With exceptions it could throw a ReferenceError instead.
  F22 covers the same problem for reads from functions.
- Backend (`crates/backend/src/lib.rs`): **no tag is emitted yet**. `check_supported` rejects `Try`/`Throw` with "planned: T12".
  wasm-encoder has `TagSection`, `try_table` with `Catch::One { tag, label }`/`Catch::All`, `throw`, `throw_ref`.
  The test validator already enables `WasmFeatures::all()`. wasmtime 49 runs EH by default, also inside async callbacks
  (`TOOLS.md`).
- Command entry (`crates/backend/src/host.rs`, `Helper::Run`): calls `main`, then `flush`. **F25**: `run` must catch `$js`
  around `main`, print `Uncaught …` to stderr through `Append(1)`, and make `flush` return `err` (set the `G_ERR` host global).
  The callback runs no JS yet, so only `run` needs the catch.
- Runtime traps that are really JS errors. They should throw once there is a tag:
  `runtime.rs` `length` (`.length` of `null`/`undefined` → TypeError, `unreachable` at ~line 736). `no_format` (~788) and
  ToNumber of a string (~384, F20) are *unimplemented features*, not JS errors: keep those as traps.
  (`numstr.rs:337` and `abi.rs:184` are internal "can't happen" / out-of-memory traps.)

**Decisions to make first (log them in Progress):**
1. **What an Error value is before objects exist (T13).** The goal lists TypeError/ReferenceError helpers, and the boundary
   needs `name` and `message`. Suggestion: a runtime `$error` struct `{ name: (ref $string), message: (ref $string) }` as a
   new jsval kind (`typeof` → `"object"`, ToString → `"Name: message"`). Lowering would special-case `new Error(m)` /
   `new TypeError(m)` / … (like `console.*` in T10) and `e.name` / `e.message` reads (like `.length`). T13 turns it into a
   real object with a prototype. The alternative is to allow only primitives in `throw` until T13.
2. **`finally`.** Either duplicate the finally body on each exit edge (normal, `break`/`continue`/`return` crossing it, throw),
   or dispatch through a completion local (kind + value) after one copy. Duplication grows exponentially with nesting;
   dispatch is the safer default. Throw path: `catch $js` → run finally → `throw $js v` again. JS doesn't need
   `catch_all`/`exnref`, because traps must not be caught and there is only one tag. The task text says `exnref`: decide
   and log it.
3. **Library boundary.** The task says an export returns `result<T, js-error>`. That changes every export's WIT, and
   `wasmtime --invoke` then prints `ok(3)`, so every `--invoke` test in `crates/js2wasi3/tests/build.rs` changes too.
   Option: only exports that may throw (transitively: `throw`, calls of throwing functions, throwing runtime helpers)
   get `result`. Decide, then update `docs/wit-names.md` / D4 notes.
4. **Uncaught in a command vs node.** node prints a source excerpt and a stack for an uncaught error, and exits 1.
   Our `run` → `err` gives exit status 1 too. The task wants `Uncaught <name>: <message>` on stderr, so the stderr diff
   will fail. **F29**: give `scripts/diff.sh` a sidecar mode, e.g. `x.stderr-ignore`, or compare only the first line.
   Uncaught-error fixtures need that. Fixtures that catch everything need nothing special.

**Acceptance reminders:** ≥ 8 fixtures in `tests/fixtures/exceptions/`, including finally + return and a throw from a nested
call, all passing `scripts/diff.sh`. A Wasm trap, such as the `%` format string that traps in `no_format`, must **not** be
caught by a JS `catch`: this makes a ready-made fixture, with `.skip`/stderr handling, or a `cargo test` in
`crates/js2wasi3/tests/build.rs`.

**Pitfalls seen so far:**
- A command's globals are mutable and set by `main`. Functions reading a `let`/`const` before `main` set it get a
  placeholder (F22). With exceptions this can become a ReferenceError check, or stay deferred.
- Wasm function indices are offset by the 15 host imports in commands (`func_base` in the emitter). A new core function
  or export must go through `func_base` / `helper_index`, not raw IR ids.
- Strict mode applies (ES modules), for node too (`tests/fixtures/package.json`).
- Remember `CLAUDE.md`: set `in progress`, append to `## Progress` after every step, update Follow-ups at the end.

## Progress

## Notes / Result
