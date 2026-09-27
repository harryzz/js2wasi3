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
  (T16: `$function` has a `construct` entry `(self, new_target, args)` (null: not a constructor) and a kind;
  a class's code throws, its construct entry runs the constructor, which is keyed by the class scope in the closure
  analysis (constructor body + instance field initializers). Accessors are an attribute bit whose value slot holds
  an `$accessor { get, set }`; attributes: hidden, read-only, fixed, accessor, private; object flags for
  non-extensible / sealed / frozen. Private names are a fresh `$symbol` per class evaluation, captured like
  values. See `crates/backend/src/props.rs`, `crates/lower/src/classes.rs`.)
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
(T17: promises are `$promise` objects; every job (reaction, resolve-thenable, `queueMicrotask`) goes into one FIFO in
globals. `run` drains it after `main`, then reports the first rejection still unhandled (node's text) and fails. See
`crates/backend/src/promise.rs`.)
(T18: generators are the first user of the state machine. `ir::suspend` (run at the end of the lowering) turns a
`function*` into a start function (makes a `$generator`, runs the prologue) and a resume function
`(gen, mode, value)`: all locals are saved to a `$vals` frame at each `yield`; re-entry keeps the structured IR
(a dispatch on the saved point at the head of each statement list, `if` conditions skipped while resuming, a `try`
re-entered as a fresh `try_table`, a `catch` body entered by skipping it). `return()`/`throw()` resume with a mode
the lowering checks after each `yield`, so `return()` runs `finally` blocks through the D2 completion dispatch.
See the `ir::suspend` crate docs and `crates/backend/src/generator.rs`.)
(T19: an `async function` is transformed the same way; its frame is a `$generator` that never reaches JS, with the
promise in one more slot. `await v` = PromiseResolve + PerformPromiseThen with the frame itself as the handler, and a
reaction job whose handler is a frame resumes it (next / throw), so no closures are made. A top-level `await` makes
`main` async. Timers: `run`/the callback end with `host::Helper::Loop`: while timers are active, one
`[async-lower]wait-until` subtask for the earliest deadline is in the waitable set; when it returns, the due timers
run (microtasks and the unhandled-rejection check after each), then Loop again; with none left, the streams are
flushed and closed. See `crates/backend/src/{asyncfn,timers,host}.rs`.)
(T25: `setImmediate` callbacks queue in a FIFO run at the end of each timers pass (only those queued before it);
`process.nextTick` callbacks run after every timer/immediate callback, then the microtasks, until both are empty,
and after the main module's microtasks (an ES module, as in node). An unref'd timer doesn't keep the loop alive.
See `crates/backend/src/timers.rs`.)
(T20: `for await` is lowered like `for-of` on `Intrinsic::AsyncIter*` (sync iterables get node's
CreateAsyncFromSyncIterator ticks). Async generators are transformed like generators (`NewAsyncGenerator`, `await`
points marked) and driven by a request queue in `crates/backend/src/asyncgen.rs`. Every suspension point breaks to
one shared "save the locals" tail per resume function.)

### D4. The WIT boundary follows jco/componentize-js conventions
The entry module's ES exports become the world's exports. WIT types map to JS:
`result` ↔ return or throw, `option` ↔ value or `undefined`, `record` ↔ plain object, `variant` ↔ `{tag, val}`,
`resource` ↔ class, `list<u8>` ↔ `Uint8Array`, `future<T>` ↔ Promise, `stream<T>` ↔ async iterable.
v0 generates the WIT from the export signatures (all `f64`). Later versions take a user `.wit` world.
JS names become kebab-case WIT names, and the world is named after the entry file (or `--world`).
Parameter and result types (`f64` / `string`) come from JSDoc or usage (T09). See [wit-names.md](wit-names.md).
(T20 slice 2: a library's top-level code is `ir::Module::init`, run once by the first export call through the
export adapters (`abi::Helper::Init`); an `init` that threw fails every later call with the same error. When the
module can queue jobs (`ir::can_queue`), each call ends by draining them (`abi::Helper::Settle`), and an unhandled
rejection (or a job's exception) makes the call return `err`.)

### D5. Built-ins
A small core runtime emitted by the backend (allocation-free helpers, equality, `typeof`, string ops),
plus built-ins written in JS/TS and compiled by js2wasi3 itself, using intrinsics (T20).
(T21: the **prelude**, `crates/lower/builtins/*.js`, is JS appended to the user's source after that source parsed
cleanly on its own, and parsed with it as one module (`lower::parse` → `frontend::parse_with_prelude`), so it goes
through the same lowering, inference and backend. Its top-level names start with `$$`: `$$Math_floor` implements
`Math.floor` (a static member of an undeclared global `A.b` maps to `$$A_b`: a direct, typed call; as a value, a
function object named `floor`). A call of an undeclared `__intrinsic_<name>(…)` in the prelude is an IR operation
(Wasm `f64.floor`/`sqrt`/`min`/`copysign`…, the 32-bit words of an `f64`, a random seed). Only prelude declarations
reachable from the user's code are lowered (`builtins::Prelude`: an AST scan), so a program that uses no built-in
compiles exactly as without the prelude. Prelude variables with a literal initializer are globals (built-in
state). User references to prelude names are rejected like unknown globals. Numeric algorithms are ports of
what V8 uses (fdlibm), so results match node bit for bit where V8 does; see `builtins/math.js`.)
(T21 slice 2: `$$String_prototype_slice` is `String.prototype.slice`, kept when the user's code names a property `slice`
(`x.slice`, `x["slice"]`); the lowering lists the kept ones in `ir::Module::builtin_methods` and the backend installs
them on the prototype over the sentinels, so calls go through the (mutable) prototype. `$$parseInt` is the global
`parseInt`. String intrinsics read code units and build new strings. The runtime's ToNumber of a string calls the
prelude's `$$StringToNumber` (`ir::Module::string_to_number`), kept when the IR converts a JS value to a number; its
decimal conversion is correctly rounded (Clinger's fast path, else big-integer comparisons), `toString(radix)` is V8's
algorithm and `toFixed` exact, so the digits match node; see `builtins/string.js`.)
(T21 slice 3: `Array.prototype` methods are the spec's generic algorithms in `builtins/array.js` (`this[k]`, `k in this`,
`this.length`), so they take any array-like `this`; `push`/`pop`/`join`/`toString`/`values` stay native. `Array(…)` and
`new Array(…)` are direct calls of `$$Array`. `sort` is a stable merge sort (the same order as V8's TimSort for a
consistent comparator). Prelude code is typed by the same inference as user code: `+x.length` keeps a length an `f64`.)
(T21 slice 4: `builtins/object.js` (`Object.assign`/`fromEntries`, the `Object.prototype` methods, installed over the
sentinels like the others, so ToPrimitive calls the prelude's `toString`) and `builtins/json.js` (`JSON.stringify`: the
spec's algorithm writing pieces into one array joined at the end; node's circular-structure message). The intrinsic
`define(o, k, v)` is CreateDataProperty. A static of a global the prelude has statics for but not this one
(`JSON.rawJSON`) is rejected by name.)
(T20: typed arrays and `ArrayBuffer`s are native `$typed`/`$buffer` objects (`crates/backend/src/typed.rs`: element
access, printing) with their constructors and methods in `builtins/typedarray.js` on `Intrinsic::Typed` operations.)
(T25 slice 1: `JSON.parse` in `builtins/json.js`: recursive descent over code units with the parser state in prelude
globals (no user code runs while parsing), members stored with `define` (`__proto__` is an own key), numbers other
than short integers through `$$StringToNumber`, then the reviver walk; node's SyntaxError messages (with node 24's line/column) and context
excerpts reproduced exactly.)
(T25 slice 2: `Map`/`Set` keep their storage native and their methods in the prelude: `$map` (a subtype of `$object`)
holds an ordered SameValueZero hash table (`crates/backend/src/mapset.rs`: insertion order, deleted entries as holes,
objects hashed by an identity hash in `class`'s high bits); `builtins/map.js` implements the methods over
`__intrinsic_map_*`. Iterators survive rehash and `clear` by V8's obsolete-table transitions. `new Map(…)` is a
direct call of `$$Map`; `Map` itself isn't a value yet.)
(T25 slice 3: `Function.prototype.bind` is native (`func.rs`, bound functions are `$function`s of kind `KIND_BOUND`);
`arguments` makes a function lower as `f(...args)`. `Function.prototype` is itself a `$function` (returns `undefined`).
`$function` has a nullable `source` field: user functions get their source text (the span, methods from the key)
only when the code names a `toString` property, and `Function.prototype.toString` is installed only then; otherwise
functions print as native code and `"" + f` traps (F104). Anonymous functions under computed keys are named at run
time (`Intrinsic::SetFunctionName`).)
(T25 slice 4: a built-in constructor used as a value (`arr.map(String)`, `x.constructor === Array`) is a function
object over its prelude function (`$$Array`), made on first use (`Intrinsic::BuiltinCtor`), with `prototype` and the
prelude statics the code names; its prototype's `constructor` is it. A program that only names `constructor` gets
stubs (same identity and `name`, calling traps) so it doesn't link every constructor's code. `Math`/`JSON` values
are objects of the prelude functions the code names (`Intrinsic::Namespace`). What the prelude links is decided by
the names in the user's code: property names, and string literals (`s[m]()`, `Math[k]`); native methods that are
not always needed are gated the same way (`ir::Module::names`). A name built at run time finds nothing (F109).)
(T25 slice 5: well-known symbols are made on first use (`G_WELL_KNOWN`); the runtime honours `Symbol.toPrimitive`,
`Symbol.hasInstance` and a user `Symbol.toStringTag` only when the code names them. `console.log` format strings are
node's `formatWithOptions` in `builtins/console.js`, linked when a string in the code has a `%`. The formatter
(`inspect.rs`) now matches node's `util.inspect` for cycles (a stack of the objects being formatted), symbol keys,
escapes, > 100 entries and Map/Set iterators; it is linked into every command through the uncaught-exception path.)

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
- node runs with `--no-warnings`: process warnings (node 24's `TimeoutNegativeWarning`, …) start with `(node:<pid>)`
  and aren't JS behaviour.
- Timers (T19): node refreshes its clock whenever a timer is made (ms steps), so the order of near-tied deadlines,
  or of a timer made inside a callback against others due soon, depends on node's speed and the machine's load.
  Keep such deadlines far apart in fixtures (see `tests/fixtures/async/timers.js`).

## Working rules
- One task file = one session. Don't start the next task without approval.
- Each iteration runs only that task's tests, `wasm-tools validate` and one wasmtime smoke run.
  Broad suites run only at the end of a task or on request.
- Unsupported syntax reports the construct and its source location. It never miscompiles silently.
- No C, Rust or LLVM IR source is generated as an intermediate step. Wasm is emitted directly.
