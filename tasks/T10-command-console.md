# T10: Command world: `console.log` over WASI 0.3 stdout

Status: done | Depends on: T09

## Goal
A file without exports compiles to a `wasi:cli/run@0.3.1` command. Top-level code runs, and `console.log` writes
to stdout through `write-via-stream` (`stream<u8>`).

## Read first
`probes/cmd.wat` (exact import names, stream write, callback loop), `probes/wit/` (WASI 0.3.1 deps via `wkg wit fetch`).

## In scope
- The export is async-lifted with a callback (D3). v0 may run top-level code in one go, then flush writes
  by returning `WAIT` on blocked writes. Await the stdout `future` before `task.return`.
- `console.log` takes primitives joined by spaces (formatting objects comes later). `console.error` goes to stderr.
- Keep the WASI calls inside a small runtime "host ABI" module in the backend. JS lowering calls only its intrinsics.

## Out of scope
Promises and timers (T17/T19), stdin, env, exit codes other than ok/err.

## Acceptance
- [x] `js2wasi3 build tests/fixtures/hello.js -o hello.wasm && wasmtime run -Sp3 hello.wasm` prints `hello from js2wasi3`.

## Handoff (written 2026-09-25 at the end of the T07–T09 session)

**State:** `main` at `8479c15` (T09), clean tree. T01–T09 done. The pipeline: `frontend::parse` → `lower` (optimistic type inference,
see its crate docs) → `backend::emit` (core module; GC runtime, Ryu and ABI glue emitted on demand) → `component::package`
(generated library world from the ES exports).

**Run first (baseline, ~1 min):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'      # all ok, 43 tests in total
cargo run -q -- build tests/fixtures/strings.js -o /tmp/s.wasm && wasmtime run --invoke 'greet("x")' /tmp/s.wasm   # "hi x"
(cd probes && ./run.sh)                      # step with cmd.wat prints the P3 hello + timer lines
```

**What T10 can reuse (all in `crates/backend`):**
- `runtime.rs`: `Helper::ToString` (JS ToString of any primitive → `(ref $string)`), `Concat`, string globals via `Cx::str("...")`.
  Joining `console.log` args with spaces = `to_string` + `concat` with `" "`.
- `abi.rs`: `Helper::StringToUtf8` (`$string` → `(ptr, len)` in linear memory, lone surrogates → U+FFFD), the bump `cabi_realloc`
  and `HEAP_BASE`/bump global. Memory is emitted only when `abi` is true in `emit()`: a command always needs it, so that condition
  (and `global_base`) must also cover "this is a command".
- `ir::Intrinsic` is the place for new runtime operations with a fixed signature (e.g. `ConsoleLog { stderr }` taking one jsval
  string). The task asks for WASI calls to stay in a small backend "host ABI" module that lowering reaches only through intrinsics:
  a new `backend/src/host.rs` next to `abi.rs` fits that.
- Helpers are requested on demand via `Cx::helper` and emitted after the user functions and ABI adapters (`helper_base`).

**What must change:**
- Top-level code is rejected today in `lower`, `declare_top` (`"top-level {} (top-level code runs in the command world)"`, planned T10)
  and non-literal top-level initializers in `declare_globals` (F11). A file with no exports → collect top-level statements into a
  synthetic `main` function; globals stay globals. Whether files **with** exports may have top-level code (module init) is F11's
  question. Decide and log it.
- `console` is an unresolved global today (`resolve`: "global `console`", planned T21). `console.log(...)` is a
  `CallExpression` whose callee is a `StaticMemberExpression`; `call()` rejects it ("call of something other than a named
  function", T14). Special-case `console.log` / `console.error` there (only when `console` has no symbol, i.e. isn't shadowed).
- `component::package` builds a `Resolve` from one generated WIT text. The command world needs the WASI 0.3.1 packages:
  `probes/wit/deps/` has them (from `wkg wit fetch`). **Open decision:** vendor the needed WIT (at least `wasi-cli`, `wasi-io`?
  check what `wasi:cli/run` + `wasi:cli/stdout`/`stderr` pull in) into `crates/component/wit/` and `include_str!` them
  (`Resolve::push_source` per file, or `push_dir` on a temp dir), versus reading them at run time. `include_str!` keeps the CLI
  self-contained; suggest that.
- The exported core function names and the callback ABI are in `probes/cmd.wat` (verified with wasmtime 49):
  `[async-lift]wasi:cli/run@0.3.1#run` → i32, `[callback][async-lift]wasi:cli/run@0.3.1#run (event, index, payload) → i32`,
  imports `wasi:cli/stdout@0.3.1` `write-via-stream`, `[stream-new-0]write-via-stream` (→ i64: readable | writable << 32),
  `[async-lower][stream-write-0]write-via-stream (w, ptr, len)` (→ -1 = BLOCKED, else progress), `[stream-drop-writable-0]...`,
  `$root` `[waitable-set-new]` / `[waitable-join]` / `[subtask-drop]`, `[export]wasi:cli/run@0.3.1` `[task-return]run (i32)`.
  Callback return codes: `0` EXIT, `2 | (set << 4)` WAIT. `TOOLS.md` "Syntax gotchas" lists the naming rules.

**Pitfalls seen so far:**
- `probes/cmd.wat` **drops** the future returned by `write-via-stream` and doesn't handle partial writes. The task says to await
  the stdout future before `task.return`, and a write can complete partially: loop on the remaining bytes after each event.
- Test snippets are ES modules, so strict mode applies (e.g. `delete x` is an early error).
- wasmtime 49 `--invoke` can't call exports named like WIT keywords (F15); not an issue for `run`, but avoid it in tests.
- Command components need `wasmtime run -Sp3`.
- Remember `CLAUDE.md`: set `in progress`, append to `## Progress` after every step, update Follow-ups at the end.

## Progress

- 2026-09-25: Start. Tools unchanged (check-tools ok). Read design, handoff, `probes/cmd.wat`, backend/lower/component.
  Core import names checked with `wasm-tools component embed --dummy-names legacy --async-callback` on a world
  importing `wasi:cli/stdout` + `stderr` and exporting `run`. Node 20 facts: a single argument prints raw; with ≥2 args a
  string first argument is a format string (`%%` → `%`); `console.log(-0)` prints `-0`; `warn` → stderr, `info`/`debug` → stdout.
- 2026-09-25: Design: `ir::Module.main` (command entry). No ES exports ⇒ command: top-level statements go into a
  synthetic `main`, top-level variables stay globals (non-literal initializer ⇒ `SetGlobal` in `main`). Files with exports
  keep rejecting top-level code (F11 → T20: a library has no place to run init code before the first export call without a guard).
  `console.*` in a library is rejected (sync exports can't wait on a stream; T20). Output is appended to a per-stream
  buffer in linear memory while `main` runs, then flushed by the callback loop (partial writes, then drop the writable end,
  then await the `write-via-stream` future, then `task.return`). New intrinsics: `ConsoleWrite{stderr}`, `Inspect`
  (ToString, but `-0` → `"-0"`), `NoFormat` (traps on a `%` in a string first argument when there are ≥2 args).
- 2026-09-25: IR (`Module.main`, 3 intrinsics, `signature()` result is now `Option`) and lowering done (builds).
  Backend plan: `host.rs` owns the 15 fixed core imports (per stream: `write-via-stream`, stream new/write/drop, future
  read/drop; `waitable-set-new`, `waitable-join`, `task-return`), per-stream globals (handles, buffer ptr/len/cap, written
  offset, phase) and helpers `append`, `step`, `flush`, the `run` export and its callback. A command always has the JS
  runtime and memory. `string_to_utf8` is split into `utf8_len` + `utf8_encode` so `append` encodes straight into the buffer.
- 2026-09-25: Backend `host.rs` + runtime `inspect`/`no_format` + component `command_world` (vendored WASI WIT in
  `crates/component/wit/`, pushed into the `Resolve` in dependency order) done. Import names guessed from the pattern
  (`[future-read-1]`: the future is payload type 1) were accepted by wit-component. **Acceptance passes**:
  `js2wasi3 build tests/fixtures/hello.js -o hello.wasm && wasmtime run -Sp3 hello.wasm` prints `hello from js2wasi3`, exit 0.
  A mixed program (globals, calls, `-0`, stdout+stderr, loop) matches node byte for byte; `add.js` library still runs.
- 2026-09-25: Output is now written as soon as it's appended (`pump`): wasmtime takes writes synchronously, so the buffer
  is reused (a 46 MB output runs under `-W max-memory-size=1048576`) and output before a trap isn't lost. A reader that
  goes away (`| head`) marks the stream dropped: later output is discarded, the stream still gets closed, `run` returns
  `err` (exit 1, no trap; node exits 0 there → follow-up for T11). Checked against node: 46 MB output, empty program,
  stderr only, UTF-8 + lone surrogate, `%` trap.
- 2026-09-25: Tests: lowering (`commands_lower`, `command_globals`, `command_rules`; library-mode messages updated),
  CLI (`hello_command_runs`, `console_matches_node` with `tests/fixtures/console.{js,out,err}` from node,
  `command_output_is_streamed`). Found and fixed a pre-existing bug: an entry file named like a WIT keyword
  (`stream.js`, `type.js`) failed in `select_world`; it now selects `%<name>`.

## Notes / Result
**Done.** `js2wasi3 build tests/fixtures/hello.js -o hello.wasm && wasmtime run -Sp3 hello.wasm` prints `hello from js2wasi3`
(exit 0). `tests/fixtures/console.js` prints byte for byte what node prints, on stdout and stderr.

What changed:
- `ir`: `Module.main` (a command's entry point; validated: no params, no result, no exports next to it). Intrinsics
  `Inspect`, `NoFormat`, `ConsoleWrite { stderr }`; `Intrinsic::signature` returns `Option<Type>` (no result for writes).
  The printer ends a command with `main = main#N`.
- `lower`: no ES exports ⇒ command. Top-level statements (not function declarations or module declarations) go into a
  synthetic `main`; top-level variables stay globals, assigned by `main` at their declaration. A literal initializer is
  the initial value when no code can run first; a `var` that could be read earlier starts as `undefined` (hoisting).
  `main` reading a `let`/`const` before its declaration is rejected (TDZ). `console.log/info/debug` → stdout,
  `console.error/warn` → stderr: primitives shown like node (`-0`), joined by spaces, plus `\n`. With ≥2 arguments a
  string first argument is a format string: rejected for a literal containing `%`, a trap (`$no_format`) for other strings
  containing `%`. Libraries keep rejecting top-level code (F11 → T20) and reject `console` (T20).
- `backend`: new `host.rs` (the only place with WASI calls): 15 fixed core imports, per-stream globals, helpers
  `append`/`pump`/`advance`/`step`/`closed`/`flush`, the `run` export (`[async-lift]wasi:cli/run@0.3.1#run`) and its callback.
  Output is appended to a per-stream buffer and written at once when the host takes it; a blocked write finishes in the
  callback loop. After `main`: write the rest, drop the writable end, await the `write-via-stream` future, `task.return`
  (`err` if a stream failed), `EXIT`. Imports shift the function index space (`func_base`); a command always has the JS
  runtime and memory. `abi.rs`: `string_to_utf8` split into `utf8_len` + `utf8_encode`. `runtime.rs`: `inspect`, `no_format`.
- `component`: `command_world` (imports `wasi:cli/stdout` + `stderr`, exports `wasi:cli/run`, all `@0.3.1`). The WASI
  0.3.1 WIT is vendored in `crates/component/wit/` and embedded with `include_str!` (decision: the CLI stays self-contained).
  Fixed: `select_world` now uses `%<name>`, so entry files named like WIT keywords (`stream.js`, `type.js`) build.
- CLI: builds a command world when the IR has `main`.

Commands run: `cargo test -q` (49 tests, all ok; 43 before T10), `cargo clippy --all-targets` (clean), `wasm-tools validate` + `wasmtime run -Sp3`
on hello/console/46 MB output/empty/stderr-only/`%` trap, `probes/run.sh` (unchanged).

Unsupported / deferred (Follow-ups F22–F28 in `tasks/README.md`): format strings, objects in `console.log`, TDZ checks
for reads from functions, uncaught exceptions → `err`, cancellation, `var` in nested top-level blocks used by functions,
the exit status when stdout's reader goes away.

Next smallest step (T11): a harness that builds each `tests/fixtures/*.js` command, runs it with `wasmtime run -Sp3` and
`node`, and diffs stdout, stderr and the exit status; `console.js` is a ready first case.
