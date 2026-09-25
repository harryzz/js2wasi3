# T10: Command world: `console.log` over WASI 0.3 stdout

Status: todo | Depends on: T09

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
- `js2wasi3 build tests/fixtures/hello.js -o hello.wasm && wasmtime run -Sp3 hello.wasm` prints `hello from js2wasi3`.

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

## Notes / Result
