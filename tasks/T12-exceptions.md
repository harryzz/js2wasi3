# T12: Exceptions with native Wasm EH

Status: done | Depends on: T11

## Goal
Implement D2: `throw`, `try/catch` (with and without a binding), `finally` (including break/continue/return inside it),
nesting and rethrow, plus the internal TypeError/ReferenceError helpers used by the runtime.

## In scope
- Lower `finally` in the IR (duplicate or dispatch). Use `try_table` + `exnref`, with one tag carrying a `jsval`.
- Boundary policy: a library export returns `result<T, js-error>` (`record js-error { name: string, message: string, stack: option<string> }`).
  A command prints `Uncaught <name>: <message>` to stderr and returns `err`.

## Acceptance
- [x] Fixtures `exceptions/*.js` (≥8 cases, including finally+return and a throw from a nested call) match node.
  (9 command fixtures; `finally.js` has finally + return, `nested.js` throws from nested calls.)
- [x] A Wasm trap, such as an explicit `unreachable` in a test helper, is **not** caught by JS `catch`.
  (`exceptions/trap.js`, checked by `traps_are_not_caught` in `crates/js2wasi3/tests/build.rs`.)

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
- 2026-09-25: started. `check-tools.sh` all ok (unchanged, no probe rerun). Baseline: `cargo test` 49 ok, `diff.sh` 3 pass / 6 skip.
- 2026-09-25: decisions.
  1. **Error values**: a runtime `$error` struct `{name, message: (ref $string)}` is a new jsval kind (type 4 of the runtime rec
     group). `typeof` → `"object"`, ToString → Error.prototype.toString (`name: message`, or just one if the other is empty),
     ToNumber → NaN, ToBoolean → true, `==` against a primitive compares its ToString, `console.log` shows `[Name: message]`
     (node's format for an error without a stack; we have no stacks). Lowering special-cases `new E(m)` / `E(m)` for the 7
     native error constructors (global, unshadowed) and `.name` / `.message` reads (intrinsic: TypeError on nullish,
     `undefined` on other primitives). T13 turns it into a real object.
  2. **finally**: dispatch, not duplication. Per `finally`: an `i32` completion local (0 normal, 1 throw, 2+ one code per
     distinct break/continue/return crossing it) and a `jsval` value local; one function-wide return-value local. The
     protected part (`try` + `catch`) becomes `block $f { try { … } catch (v) { kind = 1; val = v } }`, exits crossing it
     become `kind = k; br $f`, then the finally body, then `if kind == k` re-issues each exit (which may cross the next outer
     finally the same way) and `if kind == 1 throw val`. **No `exnref`/`catch_all`/`throw_ref`**: there is one tag, traps
     must stay uncatchable, and re-throwing the same jsval loses nothing (no stack traces).
  3. **Library boundary**: only exports that may throw (IR analysis `ir::may_throw`: an uncaught `throw`, a call of a
     function that may throw, a throwing intrinsic) return `result<T, js-error>`; the others keep their plain WIT signature
     (existing `--invoke` tests unchanged). `record js-error { name: string, message: string, stack: option<string> }` is
     declared in the world when needed; `stack` is always `none`. A thrown non-Error value gives `name: ""` and the
     `console.log` text of the value as `message`.
  4. **Uncaught in a command**: `run` catches `$js` around `main`, writes `Uncaught <text>` to stderr (text: ToString for
     errors, `console.log` format otherwise), and `task.return(err)` → exit 1 like node. Harness (F29): a sidecar
     `x.uncaught` makes `diff.sh` reduce node's stderr (source excerpt + stack) to `Uncaught <message lines>`.
  5. **TDZ**: `main` reading a `let`/`const` before its declaration throws a ReferenceError at run time (was a build error).
     F22: a top-level `let`/`const` gets a `bool` init flag only when a function could run before its declaration (a
     reference to a top-level function from `main` code before the declaration ends); reads/writes from functions then check it.
- 2026-09-25: IR: `Intrinsic::NewError(ErrorKind)` / `ErrorProp(Name|Message)`, `Intrinsic::may_throw`, `ir::may_throw`
  (new `throws.rs`), `AbiSig.throws`; validator checks `Try`'s catch local and `Throw`'s operand are `jsval`.
  Backend: `$error` type (T_ERROR = 4, TYPES = 5), tag section (tag 0, emitted with the JS runtime), `Throw`/`Try`
  emission, helpers `new_error`, `get_name`/`get_message`, `throw_read_error`, `describe`; `to_number`/`to_boolean`/
  `typeof`/`to_string`/`loose_eq`/`js_add`/`inspect` know errors; `length` of nullish throws a TypeError.
  `abi::adapter` takes the `AbiSig` and returns `result<T, js-error>` via a return area when `throws`.
  `host`: `run` catches around `main`, `uncaught` helper. Component: `ExportSig.throws`, `JS_ERROR` record in the
  world, `js-error` name clash check; `cargo test -p js2wasi3-component` ok (7). Backend builds; lowering next.
- 2026-09-25: lowering: `throw`, `try`/`catch`/`finally` (`Lowerer::jump` routes break/continue/return through
  enclosing finally scopes; `try_stmt`/`try_catch`), `new E(m)`/`E(m)`, `.name`/`.message`, `always_returns` knows
  `throw`/`try`, exports get `sig.throws` from `ir::may_throw`. TDZ: `Var.guard` (`read_var`/`write_var`/`assign_value`);
  main's early reads throw, F22 flags (`<name>$init` bool globals) from `tdz_flags()`, which maps function references to
  positions: needed `SemanticBuilder::with_build_nodes(true)` in the frontend (nodes were empty → index panic).
  Harness: `scripts/node-uncaught.mjs` + `x.uncaught` sidecar in `diff.sh` (F29).
  9 fixtures in `tests/fixtures/exceptions/` (basic, errors, finally, loops, nested, rethrow, runtime, uncaught,
  uncaught-primitive): `scripts/diff.sh tests/fixtures/exceptions` → 9 passed. `wasm-tools validate` ok and a manual
  `wasmtime run -Sp3` of runtime/uncaught/loops (uncaught exits 1).
- 2026-09-25: library boundary: `exceptions/lib.js` + `throwing_exports_return_results` (build.rs). Bug found and fixed:
  the adapter pushed the arguments before `try_table` (a block type without params can't see them) → args inside.
  `.length` of a known string is now `Intrinsic::StringLength` (inline, can't throw), else every string export using
  `.length` would get a `result`. Regression in `strings.js` (`len` became `result`): the parameter's round-1 guess
  `Num` joined with the later `Str` into `Any`; fix: `next_facts` restarts the kinds when string parameters grow.
  `unsupported.rs` updated (TDZ is no longer a build error; new `exceptions` test), backend test now uses `Suspend`
  as its unsupported IR, IR test `may_throw_follows_calls`. `diff.sh` default now includes subdirectories
  except `unsupported/`. `cargo test`: 54 ok; clippy clean; `diff.sh`: 12 passed, 0 failed, 8 skipped.

## Notes / Result
**Done.** Exceptions follow design D2 (details in the decisions above and in `docs/design.md` D2, `docs/wit-names.md`).

What changed:
- IR: `Intrinsic::{NewError, ErrorProp, StringLength}`, `Intrinsic::may_throw`, `ir::may_throw` (`crates/ir/src/throws.rs`),
  `AbiSig.throws`; the validator types `Try`/`Throw`.
- Lowering: `throw`, `try`/`catch`/`finally` (dispatch through a completion local, `Lowerer::jump`/`try_stmt`), the 7
  native error constructors with or without `new`, `.name`/`.message`, `always_returns` knows `throw`/`try`.
  TDZ: `main`'s early accesses throw a ReferenceError; functions check a `<name>$init` flag when they could run
  before the declaration (F22). Type inference restarts when a parameter becomes a string.
- Backend: `$error` runtime type, the tag, `try_table`/`throw`, error-aware runtime helpers, TypeError for
  `.length`/`.name`/`.message` of `undefined`/`null`, adapters returning `result<T, js-error>`, `run` catching
  uncaught exceptions. Component: `result<T, js-error>` and the `js-error` record in generated worlds.
- Frontend: semantic analysis builds AST nodes (needed to map references to positions).
- Harness: `x.uncaught` + `scripts/node-uncaught.mjs`; default fixture set includes subdirectories.

Commands: `cargo test` (54 ok), `cargo clippy --all-targets` (clean), `scripts/diff.sh` (12 passed, 8 skipped),
`wasm-tools validate --features all` + `wasmtime run -Sp3` on runtime/uncaught/loops, `wasmtime run --invoke` on `lib.js`.

Unsupported / different from node (see Follow-ups F31–F36):
- No stack traces: `console.log(err)` prints `[TypeError: msg]` (node prints the stack), `js-error.stack` is `none`.
- Errors aren't objects yet: no `instanceof`, other properties, `cause`, `class X extends Error`, `new` of anything else.
- Stack overflow and other Wasm traps are uncatchable (node throws a catchable RangeError for deep recursion).
- `ir::may_throw` is conservative (e.g. `.length` of a value not known to be a string).

Next smallest step: T13 (objects and prototypes) turns `$error` into an object with `Error.prototype` and replaces the
`name`/`message` intrinsics with property access.
