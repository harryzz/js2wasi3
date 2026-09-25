# js2wasi3 design

A compiler written from scratch: **JavaScript (ES modules) → a WASI 0.3 Wasm component**,
using WasmGC for the heap, native Wasm exception handling for JS exceptions, and the
Component Model's async ABI for I/O. Porffor is a reference for JS semantics only;
`docs/archive/porffor-wasi-p3-task.md` is the superseded plan.

## Pipeline

```text
entry.js ──frontend──▶ AST + scopes ──lower──▶ IR ──backend──▶ core Wasm ──component──▶ component.wasm
          (oxc)                       (crates/ir)  (wasm-encoder)            (wit-component, WASI 0.3.1 WIT)
```

| Crate | Owns | Must not |
|---|---|---|
| `frontend` | parsing, scopes, early errors, spans, diagnostics | know about Wasm |
| `ir` | JS-aware IR data types, printer, IR passes (e.g. the suspension transform) | depend on oxc, encode bytes |
| `lower` | AST (+ semantic) → IR lowering, "unsupported" diagnostics | encode bytes |
| `backend` | IR → core module: types, functions, runtime helpers, tag | call WASI directly from JS lowering |
| `component` | WIT world selection/generation, the Canonical ABI glue, packaging | contain JS semantics |
| `js2wasi3` | CLI | contain logic |

## Decisions

### D1. Heap: WasmGC
- A JS value is `(ref null eq)` (written `jsval`): `i31ref` for small integers, and GC structs for
  boxed f64, strings, objects, functions, symbols, bigints. `undefined`, `null`, `true` and
  `false` are singleton structs, so `null` is never a JS value.
  (T08: integers in [-2^30, 2^30) except -0 are `i31ref`; the singletons are one `$oddball` struct type
  with a kind field. Variables, params and results are `f64` unless a non-number is ever stored in them:
  see `crates/lower`, "Type inference", and `crates/backend/src/runtime.rs`.)
  (T13: objects are `$object` dictionaries: entries in insertion order in GC arrays, a hash index past 8 entries, a
  prototype. Built-in prototypes are created lazily; their methods are sentinels that trap when read until functions
  exist (T14/T21). Errors are objects of class `error`. See `crates/backend/src/object.rs`.)
  (T14: functions are `$function` structs, a subtype of `$object` holding a `$code` funcref
  `(self, this, args) -> jsval` and an environment array. Captured variables are boxed in cells, one per
  variable (flat closures); direct calls stay typed, calls through values use one uniform `call_ref`.
  See `crates/backend/src/func.rs` and `crates/lower/src/closures.rs`.)
  (T15: arrays are `$array`, a subtype of `$object` with a dense `$vals` store (holes are Wasm null) and a length;
  symbols are `$symbol`, a subtype of `$string` compared by identity, so property keys stay string arrays.
  Iteration uses an internal `$iter` state with fast paths for arrays and strings and the protocol otherwise.
  See `crates/backend/src/{array,iter,inspect}.rs` and `crates/lower/src/patterns.rs`.)
  (T09: string literals are immutable globals built with `array.new_fixed`; number → string is Ryu
  (`crates/backend/src/numstr.rs`), with its tables in a passive data segment loaded by `array.new_data`.)
- Strings: `(array i16)` (UTF-16, as JS requires), ropes added later only if a benchmark calls for them.
- Objects: start as dictionary objects (a hash map in GC arrays) with a prototype. Hidden classes and
  inline caches only after T12 works and a benchmark justifies them.
- Linear memory exists **only** for the Canonical ABI boundary (`cabi_realloc`, string/list copies).
  (T09: a bump allocator, reset by every export adapter once its string arguments are copied into
  GC strings; see `crates/backend/src/abi.rs`. The core-side adapters live in the backend because they
  use the GC runtime; `crates/component` only generates the WIT and packages.)

### D2. Exceptions: native Wasm EH
- One private tag `$js (param (ref null eq))`. `throw` becomes `throw $js`. `try/catch` becomes `try_table (catch $js)`.
  `finally` is lowered in the IR (normal, break/continue/return and throw paths).
- Wasm traps stay traps: JS can't catch them.
- At the boundary the tag never escapes: a library export returns `result<T, js-error>`, and a
  command prints the error to stderr and returns `err`.
- (T12) `finally` uses dispatch, not duplication: an `i32` completion local per `finally` (0 normal, 1 throw,
  2+ one per `break`/`continue`/`return` leaving it), one copy of the finally body, then `if` chains re-issue
  the jump or `throw $js` the saved value. No `catch_all`/`exnref`/`throw_ref`: one tag, traps stay uncatchable.
  Only exports that may throw (`ir::may_throw`) get `result<T, js-error>` (see [wit-names.md](wit-names.md)).
  An uncaught exception in a command prints `Uncaught <name>: <message>` (or the `console.log` text of a
  non-error) to stderr and exits 1. Runtime errors (`.length` of `null`, TDZ) throw node's TypeError/ReferenceError.

### D3. Async: P3 at the boundary, a state machine inside
Component Model async handles concurrency **between the component and the host**:
async lift/lower, `future`, `stream`, waitable sets and the callback event loop. It gives no
way to pause **one JS function in the middle of its body** while other JS keeps
running in the same export call. JS needs that for every `await`, every generator `yield`
(even with no I/O) and every async generator, and many such frames can be paused at once.

| Option | Needs | Measured | Verdict |
|---|---|---|---|
| **State machine**: the IR turns async functions and generators into resumable frames (GC struct + state index) | only default features | plain code | **default** |
| Core stack switching (`cont.new`/`resume`/`suspend`) | `-W stack-switching` (experimental) | ~0.03 µs per switch | optional backend mode later (T22) |
| Component Model threads (🧵) | 2 experimental flags | ~0.75 µs per switch (goes through the host) | not for JS coroutines |
| Stackful async lift (🚟) | 1 experimental flag | one stack per export call | not enough on its own |

The event loop is the **async callback lift**. The export runs JS until no microtasks are left,
then returns `WAIT` on one waitable set (or `EXIT` after `task.return`). The callback settles the
Promise for the finished handle, drains microtasks again, and repeats.
Probes 2 and 5 in `probes/` show this loop working with Wasmtime 49.
(T10: a file without exports is a command: its top-level code is a synthetic `main`. `crates/backend/src/host.rs` holds
every WASI call (fixed imports) and the `run` export + callback; `console.*` output goes to per-stream buffers that are
written as soon as the host takes them, and the callback finishes blocked writes, closes the streams and awaits their
futures before `task.return`. The WASI 0.3.1 WIT is vendored in `crates/component/wit/`.)

### D4. The WIT boundary follows jco/componentize-js conventions
The entry module's ES exports become the world's exports. WIT types map to JS:
`result` ↔ return or throw, `option` ↔ value or `undefined`, `record` ↔ plain object, `variant` ↔ `{tag, val}`,
`resource` ↔ class, `list<u8>` ↔ `Uint8Array`, `future<T>` ↔ Promise, `stream<T>` ↔ async iterable.
v0 generates the WIT from the export signatures (all `f64`). Later versions take a user `.wit` world.
JS names become kebab-case WIT names, and the world is named after the entry file (or `--world`).
Parameter and result types (`f64` / `string`) come from JSDoc or usage (T09). See [wit-names.md](wit-names.md).

### D5. Built-ins
A small core runtime emitted by the backend (allocation-free helpers, equality, `typeof`, string ops),
plus built-ins written in JS/TS and compiled by js2wasi3 itself, using intrinsics (T20).

## Testing
Test host: wasmtime 49.0.1 (`TOOLS.md`). JS oracle: `node`.

| Layer | Where | Checks |
|---|---|---|
| Unit | `cargo test` in each crate | naming, IR validation, runtime helpers, `unsupported` diagnostics (`crates/lower/tests/unsupported.rs`) |
| IR snapshots | `crates/lower/tests/snapshots.rs` | `tests/fixtures/x.js` lowers to `x.ir` (`UPDATE_SNAPSHOTS=1` rewrites them) |
| Components | `crates/js2wasi3/tests/build.rs`, `crates/backend/tests/emit.rs` | exports called with `wasmtime run --invoke` and compared with node's results (`x.out`, or values in the test); `console.js` against `console.out`/`.err` so `cargo test` covers a command without node |
| Commands | `scripts/diff.sh` | node vs js2wasi3 + `wasmtime run -Sp3`: stdout, stderr and exit status |

**Differential harness** (T11): `scripts/diff.sh [-j jobs] [fixture.js | dir]...`
- The default is `tests/fixtures` and its subdirectories (T12: `exceptions/`), except `tests/fixtures/unsupported/`
  (rejection tests, not diff targets).
- It builds the compiler once, then runs the fixtures in parallel (10 s timeout each). It prints PASS/FAIL/SKIP with
  the time per fixture (SLOW at ≥ 2 s), the diffs or build diagnostics of failures, and a summary. Exit 1 on any failure.
- `x.skip` next to `x.js` skips it; its first line is the reason (e.g. libraries: exports, nothing to run).
- `x.uncaught` next to `x.js` (T12, F29): the program ends with an uncaught exception. node's stderr (source excerpt,
  stack) is reduced to `Uncaught <message>` by `scripts/node-uncaught.mjs` before the comparison.
- Fixtures are ES modules for node too (`tests/fixtures/package.json`: `"type": "module"`), so strict mode matches.
- A command fixture is self-checking: it prints its results, and node's output is the expectation, so no `.out`
  file is needed. Keep each one under 2 s. Constructs that aren't supported yet show up as build failures.
- `cargo test` doesn't run it (it needs node, F30).

## Working rules
- One task file = one session. Don't start the next task without approval.
- Each iteration runs only that task's tests, `wasm-tools validate` and one wasmtime smoke run.
  Broad suites run only at the end of a task or on request.
- Unsupported syntax reports the construct and its source location. It never miscompiles silently.
- No C, Rust or LLVM IR source is generated as an intermediate step. Wasm is emitted directly.
