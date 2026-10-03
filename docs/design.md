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
  exist (T14/T21). Errors are objects of class `error`. See `crates/backend/src/object.rs`.
  T63: an error's own `stack` is lazy like V8's: when the code names `stack`, a hidden accessor whose get/set are
  the sentinel; the first read makes it `${String(e)}\n    at <anonymous>` (the header as of that read; no source
  positions at run time) and a write replaces it, both as a hidden data property, which
  `Object.getOwnPropertyDescriptor` also shows (node: get/set functions). Printing an error ignores such a
  one-frame `stack`, so `console.log(e)` and uncaught errors print as before.
  T68: `Error.captureStackTrace(o, fn)` is JS in the prelude (`builtins/object.js`): an own `stack` accessor like
  V8's (get/set functions, not enumerable), made on its first read from `Error.prototype.toString`'s steps on `o`
  then, plus the same one frame; `fn` hides nothing. Since `Error` has a prelude static, its `Object.prototype`
  methods (`Error.isPrototypeOf`) count as inherited, as for constructor values. `Error.stackTraceLimit` and
  `Error.prepareStackTrace` are plain properties nobody reads (F296).)
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
  (T38: a `function` using `new.target` (itself or in its arrows, which capture it like `this`) takes it as a hidden
  param after `this` (`undefined` in calls) and gets its own construct entry `f$construct` (`FnKind::OrdinaryWith`):
  CreateThis from the new target, the call, then the object result or `this`. T43: arrows capture `arguments` the
  same way (`Cap::Arguments`, the nearest non-arrow function's object).)
  (T31: a private getter/setter pair is one `$accessor` entry with `PRIVATE | ACCESSOR`, installed like private
  methods; computed class field keys are evaluated with the class into a cell per key, which the constructor
  captures; `static {}` blocks are lowered inline like static field initializers. See `crates/lower/src/classes.rs`.)
  (T32: `$array` has `eattrs: (ref null $bytes)`, each element's attribute bits (null: all default); an accessor
  element's slot holds its `$accessor`, so element readers resolve it with `props::Helper::Value`; `class` flag
  `LENGTH_READONLY` for a non-writable `length`. `Object.defineProperty` shares ValidateAndApplyPropertyDescriptor
  between dictionary entries and elements. T34: element writes are either [[Set]] (`array::Helper::SetIndex`: for an
  absent element, once `runtime::G_INDEX_PROTO` says an index key reached the prototype chain, setters and read-only
  properties there apply) or CreateDataProperty (`DefineIndex`: literals, spread, `__intrinsic_define`); the prelude
  builds its own arrays with definitions (`$$append`). T33: holes read through the prototype chain. T35: the native
  `push`/`pop`/`join`/`toString` fall back to the prelude's generic versions (`ir::Module::array_generic`) for a
  `this` that isn't an array. See `crates/backend/src/array.rs`.)
  (T30: primitive wrapper objects are `$wrapper`, a final subtype of `$object` with the primitive in `prim`; a String
  wrapper's indices and `length` are real read-only own properties. ToPrimitive unwraps through the built-in
  `valueOf`/`toString`; the prelude reads `prim` with `__intrinsic_unwrap` and wraps with `__intrinsic_to_object`.
  See `crates/backend/src/wrapper.rs`.)
  (T69: wasmtime 49 compiles `ref.test`/`ref.cast` against a concrete non-final type (`$string`, `$object`) to an
  inline compare only for the exact type; any other value calls its `is_subtype` libcall, even for "no". Value tests
  of a local go through `runtime::JsTest` (`test_string`, `test_object`): the abstract `array`/`struct` kind and exact
  final types first, the concrete test only where it's exact or rare; same results. Host side: the copying collector
  grows its heap only once live data fills the active semi-space, so a program with a large live set collects
  constantly: `-O gc-heap-initial-size` (README); upstream draft in `tasks/T69-files/wasmtime-issue.md`.)
  (T40: a BigInt is a `$bigint`, a final `(array (mut i32))`: the sign, then 32-bit magnitude limbs, little-endian,
  normalized; never changed once made, so `===`, `==`, SameValueZero and Map/Set hashing compare contents. The type
  (the last of the `rec` group) and the globals of `BigInt.prototype`/`BigInt` (after the runtime's; `Builtin::BigInt`
  is outside `Builtin::ALL`) exist only when `ir::Module::bigint` says the user's code can make one (a BigInt literal,
  or it names `BigInt`); without it the module is byte-for-byte what it was. The arithmetic is native
  (`crates/backend/src/bigint.rs`: schoolbook multiplication with `i64` products, Knuth's division D, shifts,
  `& | ^` on two's complement, correctly rounded `Number(…)`); an operator with an operand of unknown kind becomes
  `Intrinsic::Numeric(op)` (ToNumeric, then the BigInt or the number operation; the result is `Any`), and relational
  operators `JsLess`. Literals are constant globals.)
  (T09: string literals are immutable globals built with `array.new_fixed`; number → string is Ryu
  (`crates/backend/src/numstr.rs`), with its tables in a passive data segment loaded by `array.new_data`.)
  (T57: string and BigInt constants are mutable `(ref $string)`/`(ref $bigint)` globals, initialized to an immutable
  placeholder (an empty string, a zero BigInt) and filled by the module's start function: their contents are one
  passive data segment (after Ryu's tables), each constant `i32.const offset; i32.const length; call
  consts$string|consts$bigint; global.set` (the makers are `array.new_data`), 1,024 constants per function
  (`consts$init<k>`; the last is the start function and calls the others). Constant expressions
  (`array.new_fixed` of `i32.const`s) need no code, but wasmtime compiles all of a module's initializers as one
  function whose compile time grows superlinearly with their count and length (10k short strings: 11 s; now
  ~20 µs per constant). Uses are unchanged (`global.get`, non-null), and so is identity: each constant is made once,
  at instantiation, as before. Every program with strings changed (uniformly); no run-time cost measured.)
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
- (T58) A `try`'s catch local is set to `null` before its `try_table`: it's assigned only on the catch path but read
  after the `try` (`finally`'s and `for-of`'s rethrow), and without a definition on the normal path wasmtime's SSA
  construction follows the read back to the function's entry through every earlier statement (a function with
  N `for-of`s translated in O(N³): 400 took 26 s; now 0.14 s). Generated code should define a local on every path
  before a read that can see the entry value.
- (T52) Array destructuring closes its iterator through the same `finally` dispatch as `for-of`: a throw, a
  generator `return()` at a `yield` inside the pattern or another jump closes it unless it's done; an assignment
  pattern's member target has its reference (object, key) evaluated before the iterator steps. A `null`/`undefined`
  base in a property get/set throws before its key is converted (an object key is named `#<Class>`, a symbol
  `Symbol(desc)`). node converts the key twice in `o[k] op= v` (the spec: once); js2wasi3 follows node.
- (T50) **wasmtime 49 caveat**: a GC reference assigned to a local inside a `try_table` and read in its `catch` is wrong
  after a collection during the throwing call (`drc`: freed unless the value is also live on the normal path; the
  default copying collector: stale, "gc heap corruption"); values defined before the `try` are fine. Prelude code
  avoids the pattern (it fills an array made before the `try`); user code can still hit it (F224).
  Fixed in wasmtime 49.0.2 (GHSA-hw8m-q44c-ggrf, GC rooting across `try_call`): F224's repro equals node under both
  collectors; the prelude's workaround stays (harmless).
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
one shared "save the locals" tail per resume function. Slice 3: a library's `export async function` is lifted
with the callback ABI too: the lift starts the call and steps it (jobs, then one `wait-until` subtask for the
earliest timer, backpressure raised while it waits); when the promise settles the callback `task.return`s its
`result` and exits. See `crates/backend/src/abi.rs`.)

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
rejection (or a job's exception) makes the call return `err`. Slice 3: an `async` export is an `async func`
returning `result<T, js-error>`, `T` being what the promise settles to (JSDoc `@returns {Promise<T>}`, else the
`return` values); a world with one imports `wasi:clocks/monotonic-clock` for timers. Slice 4a: `--wit` takes a user
world; its scalar types decide the boundary: numbers ToNumber, integers ToInt32 and then narrowed to their width
(64-bit ones: numbers, saturated), `bool` ToBoolean, `char` ↔ a one-code-point string, and without a `result`
a throw traps. Slice 4b-1: `option`, `result` (at the top: return or throw, `E` by jco's `payload` rule),
`tuple`, `record`, `enum`, `flags` with jco's JS shapes; `crates/backend/src/wit.rs` has the Canonical ABI
flattening, layout and conversions. Slice 4b-2: `list<T>` ↔ an array (numeric element types up to 32 bits and `f64`: the typed
array, `list<u8>` ↔ `Uint8Array`, as in jco), `variant` ↔ `{tag, val}`; params past 16 flat values come in memory. Slice 5a: an imported interface's functions are ES imports
(`import { f } from "ns:pkg/iface"`): each is an IR function `(jsval…) -> jsval` calling `Intrinsic::CallImport`,
whose backend adapter (`abi::Helper::Import`) lowers the arguments as the params' tuple and lifts the result, then
frees what it allocated (the bump pointer goes back); an interface export is `export const iface = { … }`, its
functions exported as `ns:pkg/iface#f`. Slice 5b: an `async func` import is `[async-lower]`: its adapter
gives a promise; a subtask that doesn't return at once joins the export call's waitable set and a pending table
(`handle → [promise, return area, import]`), and the callback's `Event` settles it (lift, the import's unwrap
function, resolve or reject); the call keeps waiting while imports are pending. Slice 6a: `future<T>` ↔ Promise in an `async func`'s
signature: each future type imports its intrinsics (`[future-new-N]f` …, N from wit-parser's
`find_futures_and_streams`); a received future is a promise fed by `future.read`, a given one is `future.new` plus
a native `then` handler that writes the settled value; reads and writes wait in the pending table, and the call
goes on after `task.return` until its writes end. Slice 6b: `stream<T>` ↔ an async iterable of chunks shaped like
`list<T>`, mostly in the prelude (`builtins/stream.js`, kept when the world has a stream): a received stream is
`$$StreamReader(h, id)`, an async generator reading with `__intrinsic_stream_read` (one 64 KiB `stream.read` per
chunk, `[chunk, ended]`); a given one is `stream.new` plus `$$StreamPump(iterable, w, id)`, an async function whose
`for await` writes each chunk with `__intrinsic_stream_write` (stored as a `list<T>`; a partial write goes on from
the pending table); each drops its end in `finally`. The pump counts as pending until it drops the writable end. User guide:
[user-worlds.md](user-worlds.md).)
(T41: **modules are joined at the source level** (scope hoisting, as Rollup/esbuild do): `frontend::link` loads the
graph from the entry (relative specifiers, `.js` required as in node), resolves every import and `export … from` to
the exporting module's binding, gives each module's top-level bindings a unique name (kept unless it clashes with
another module's, a global another module reads, or a binding in a scope where an importer's reference would be
captured; the entry's names first), deletes module imports and dependencies' `export` keywords (`export default
<expr>` becomes a `const`), and concatenates the modules in evaluation order (post-order DFS, entry last) into one
source, which goes through `parse_with_prelude` and the lowering as one program. Live bindings, cycles, function
hoisting across modules and TDZ follow from it being one scope. `Program` keeps a segment map (joined offset → file
and offset) for diagnostics and the original names (a renamed function's `name`; a replaced reference's text for
TDZ messages, which node words with the importer's local name) and the references to imports (writes throw
"Assignment to constant variable."). Only the entry's exports are WIT exports; `build --command` drops them like a
dependency's (node runs such a file as a program). An entry without module imports is parsed exactly as before,
so single-file programs compile to the same bytes. Slice 2: a namespace object (`import * as ns`, `export * as ns
from`) is made before any module runs by code the linker writes at the start (`const namespace$1 =
$$ModuleNamespace(keys, getters)`, allowed to call the prelude): a null-prototype, non-extensible object of class
`CLASS_MODULE` with a getter per export (so it's live and throws in the TDZ); the runtime, gated on the name
`$module`, shows the getters as writable data properties (descriptors, `console.log` as node's `[Module: null
prototype] {…}`), and fails writes with node's messages. Slice 3: when a command's dependency has top-level `await`,
the linker records each module's joined range (`SourceMap::modules`) and starts the generated code with `await
$$EvaluateModules(__intrinsic_module_bodies(), tla, requests)`; the lowering makes each module's top-level statements
its own function (`module$k`; globals stay shared, TDZ checks as in `main`) and the intrinsic gives their function
objects; `builtins/module.js` runs the spec's InnerModuleEvaluation / ExecuteAsyncModule /
AsyncModuleExecutionFulfilled (the ticks match node's). Other programs keep one `main`. `import.meta` is a generated
per-module `const` made by `$$ImportMeta` with the source's absolute path at build time; a JSON module (`with { type:
"json" }`) is checked at build time and loaded as `export default <the JSON>;` (`"__proto__"` keys made computed). Slice 4: `import()` loads only modules known at build time. The linker
finds each one's specifier patterns (string literals, `const`s, `?:`/`||`/`??`, template and `+` patterns, `*`
within a path segment) and matches them against the files (`frontend::Host::list`); `--include <glob>` adds modules
for specifiers it can't work out (without it, such an `import()` is a build error). Those modules load after the static
graph; one that fails to parse or link (or imports one that does) isn't in the program: its table entry rejects with
the error at run time, as node does. With any `import()` every module is a function; `import(x, o)` becomes
`$$DynamicImport(k, x, o)` (`k`: the importer), which resolves `x` against the importer's absolute directory, looks it
up in the table (absolute path → module, namespace), and evaluates it with the same evaluator (the spec's Evaluate,
one promise per module) one job later. Slice 5: bare specifiers are resolved at build time with node's ESM algorithm
(`frontend/src/packages.rs`: `node_modules` upwards, `exports`/`imports` with node 24's conditions, legacy `main`);
node's built-in modules are rejected.)
(T54: **CommonJS**. Which files: node's rule (`Format` in `link.rs`): `.mjs` ESM, `.cjs` CommonJS, else the nearest
`package.json` `"type"`, else syntax detection (ESM syntax = `import`/`export` statements, `import.meta`, top-level
`await`; a file that only parses as CommonJS, like one with a top-level `return`, is CommonJS). An entry node would run
as CommonJS that can't tell (no `module`/`exports`/`require`/`__filename`/`__dirname`/`arguments`, no top-level
`this`) stays an ES module, so such programs keep their bytes. **Strict**: CommonJS is compiled as strict code (a
measurement of zigbee2mqtt's graph: 81% of CommonJS files, 94% of the bytes, start with `"use strict"`, and none of
the others uses sloppy-only syntax); sloppy-only syntax is a build error with a hint (F232). **Representation**: a
file is parsed inside node's wrapper `(function (exports, require, module, __filename, __dirname) {…})` (offsets map
back through an `EvalMap` layer), and joined as a hoisted `function cjs$k(…)`; its top-level names stay inside it.
The linker writes a table first, `modules$1 = [[fn, filename, dirname, requests, module, parent]…]`, `requests`
being the module's static `require("…")`/`require.resolve("…")` specifiers resolved at build time (node's
LOAD_AS_FILE/LOAD_AS_DIRECTORY; packages with `packages::resolve_require`: the `require` conditions): `[spec, k]`, an
ES module's `[spec, -3, evaluation index, namespace, filename]`, not found `[spec, -1]` (node's `MODULE_NOT_FOUND`
when it runs: optional dependencies in `try` work), another resolution error `[spec, -1, code, message]`. JSON files
are CommonJS `module.exports = <JSON>`. `builtins/cjs.js`: `$$cjsLoad` runs a module once (node's Module._load: a
cycle sees the partial `module.exports`, a module that throws leaves the cache), `module` objects share a `Module`
prototype (`parent` getter), `require` is per module (`resolve`, `main`). A CommonJS entry ends with `$$cjsMain(table,
k)`. **ESM → CommonJS**: each CommonJS module's `locals` are `default`/`'module.exports'` (its `module.exports`) and
the names `cjs_exports()` finds (an AST version of cjs-module-lexer, its rules probed against node, re-exports
followed), so ResolveExport and namespaces are unchanged; after the module's function the linker writes `const
exports$k = $$cjsImport(table, k)` and a snapshot `const` per name, where ESM evaluation reaches it. **CommonJS →
ESM**: such a program uses the evaluate mode (every module a function); `require` evaluates the module
synchronously (`$$moduleRequire`, `evaluate(-m - 1)` on the running DFS stack), a graph with top-level `await` is
`ERR_REQUIRE_ASYNC_MODULE`, the result is the namespace (`__esModule: true` added when it has a default) or its
`"module.exports"` export. Code for this in shared prelude functions is in `if (__intrinsic_cjs_mode()) { … }`
blocks (kept when the source has the linker's `$$cjs…` code), which hold no functions and use only what their
function already captures: the closure analysis runs over dropped blocks too. **Computed `require`**: T41's
`import()` patterns (`.js`/`.json` added when a pattern has no extension); the matches and `--include`d modules are
listed in `$$cjsKnown([...])` and found at run time by path (LOAD_AS_FILE suffixes, `index.*`). Unlike `import()`,
a specifier the build can't work out isn't a build error (CommonJS code does it on paths that don't run): it finds
only what the build has, else node's error plus a line pointing at `--include`. Likewise `require` of a node
built-in is a build error only where it always runs (module top level, not in `try` or a function), else
`ERR_UNKNOWN_BUILTIN_MODULE` when it runs.)
(T55: **node's built-in modules** are CommonJS files shipped with the compiler (`crates/frontend/node/<name>.js`,
listed in `frontend/src/node.rs`), which the linker loads at the path `node:<name>` for both `x` and `node:x`, so
`require`, `import`, `import()` (table key `dir\nx`; the run time drops a `node:` prefix it finds there, in an
`__intrinsic_cjs_mode()` block) and the interop above work unchanged, with one module object per built-in. Their code
may use the prelude and its intrinsics like the linker's generated code (`SourceMap::is_node_module` →
`Program::is_generated`), doesn't make functions keep their source text (`Prelude::to_string`: it named whole
modules' text as string constants), and their namespaces have every listed property as a named export and no
`'module.exports'` (`node::exports`, for modules whose `module.exports` the lexer can't see). `require.resolve("path")`
is `"path"`. They're ports of node 24's lib where it's pure JS (`path`'s POSIX half, `events`, parts of `util` and
`assert`), else our own (`os`: placeholders for a Linux-like host; `tty`: `isatty` false). Built-ins js2wasi3 lacks keep
T54's rule (a build error where it always runs); unknown `node:x` is node's `No such built-in module`.)
(T56: **`Buffer`**, node's global, is `node:buffer`'s export, made by the linker: a CommonJS module with an unresolved
`Buffer` gets `const Buffer = require("node:buffer").Buffer;` first in its wrapper (loaded lazily, in node's order);
an ES module's unresolved `Buffer` references become `node:buffer`'s `Buffer` export and the module requests it, like
an import (`Module::buffer_refs`); programs that don't name it don't load it. `node/buffer.js` is the `buffer` npm
package (feross, which re-prototypes plain `Uint8Array`s, so typed arrays need no species) with node 24's errors and
checks, a WHATWG UTF-8 decoder, `base64url`, `subarray` giving a Buffer; `node/string_decoder.js` is node's former
pure-JS decoder. Functions of node's built-in modules and every CommonJS module's wrapper function keep no source text
(F246: the 66 KB `buffer` module as a string constant cost 30 s in wasmtime).)
(T59: **a command imports WASI interfaces** from JS like a user world's (`import { f } from "wasi:x/y@0.3.1"`,
`component::COMMAND_WASI_IMPORTS`: those the runtime doesn't import itself); its boundary has only the interfaces
its `import`s name (a boundary's types decide some of the runtime). The calls **block** (`ir::AbiSig::blocking`):
an `async func` is sync-lowered (a plain import name; the callback-lifted `run` task may block: probe 9), a returned
`stream<T>` is read to its end into one `list<T>`-shaped value and a returned future to its value (async
`stream.read`/`future.read`, then `waitable-set.wait` on a fresh set when they'd block: sync stream built-ins need
wasmtime's 🚝), a stream argument (a `list<T>`-shaped value) is written after the call returns, before the results
are read. Resources are low-level: a handle is a number, `[method]r.f`/`[static]r.f`/`[constructor]r` and a
synthetic `[resource-drop]r` are imported by their WIT names (string imports), `self` first. **`node:fs`** is JS
over that (`node/fs.js`; its imports are in `node:internal/wasi_fs`, an ES module, the only built-in one:
`node::is_esm`): paths resolve against `process.cwd()` ("/" under wasmtime) and the longest preopen, WASI's
`error-code` becomes node's error (code, Linux errno, libuv's message), `Stats` have node's shape with what WASI
lacks zeroed; callbacks and promises run the sync call at once and deliver the result later (`setImmediate`; T70: promises
settle in that later turn too, `fs.js` `later`, so `setImmediate` callbacks and a closed socket's `close` come first,
as with node's thread pool). `constants` is node's deprecated module.)
(T60: **`node:stream`** is node 24.21.0's own `lib/stream.js`, `stream/promises`, `_stream_*` and
`lib/internal/streams/*` (and `internal/fs/streams` for `fs.createReadStream`/`createWriteStream`), generated by
`scripts/gen-node-streams.mjs` from the pinned node's built-in sources (`process.binding("natives")`) into
`crates/frontend/node/stream/`: `primordials` becomes `require("node:internal/primordials")`, every `require('x')`
a `node:x` built-in, node's other internals map to small support modules (readable-stream 4.7.0's `errors`,
`primordials`, `validators`, `util`, `util/inspect`, MIT; `node/abort.js`: a minimal `EventTarget`/`AbortSignal`/
`AbortController`; `node/fs_support.js`), and a few checked patches (no `AsyncResource`, `FastBuffer` →
`Buffer.from`, no `err.stack` reads). Node's own code rather than readable-stream (node 18's) because node 24 is
the oracle: event order and messages already differed. Rerun the script after a node update. Built-in names are
node's exact `builtinModules` list (`process/` is the npm package). After a **CommonJS entry**'s code the
`nextTick` queue runs before the microtasks (`ir::Module::cjs_main`), as in node; after an ES module's, the
microtasks first. Function declarations whose binding is assigned are rewritten in the source
(`frontend::fn_assign`: `function f$fn…` plus `var f = f$fn` after the scope's directives), since declared
functions are called directly.)
(T61: node's **globals that are built-in modules' exports** are a table, `node::GLOBALS` (`Buffer` from T56, and
`WeakMap`/`WeakSet` from `node:internal/weak`, `AbortController`/`AbortSignal`/`EventTarget`/`Event` from
`node:internal/abort`): the linker binds a module's unresolved references to one like T56's `Buffer`. So
`WeakMap`/`WeakSet` are JS classes over a `Map` with private-field brand checks, not native: weakness isn't
observable without `WeakRef`/`FinalizationRegistry`. They aren't global-object properties (`globalThis.WeakMap`);
`process` is.)
(T64: more of them. `DataView` (`node:internal/dataview`) is a class over a `Uint8Array` view of its buffer and an
8-byte scratch buffer; `ArrayBuffer.isView` asks it through a hook the module sets (`$$setDataViewCheck`), so
programs without it keep their bytes. `TextEncoder`/`TextDecoder` (`node:internal/encoding`) are JS (UTF-8 with
the WHATWG decoder's replacement and streaming, UTF-16LE/BE, windows-1252). `URL`/`URLSearchParams` are node's
`internal/url.js` over `node:internal/url_binding`, a WHATWG URL parser written from the URL Standard that stands
in for node's C++ binding (ada): it returns the `href` and fills ada's component offsets, which node's getters slice;
the setters are the basic parser with a state override. IDNA is reduced (ASCII lower-casing, full-width forms,
`toLowerCase` + NFC, Punycode; no UTS #46 table). node's `url`, `querystring`, `internal/url` and
`internal/querystring` are node 24.21.0's lib ported by `tasks/T64-files/port-url.py` (primordials become local
functions, node's errors local ones). A `[util.inspect.custom]` method's string result is re-indented when nested,
as node does (`inspect.rs` `Reindent`).)
(T65: `crypto` is a global too, `node:crypto`'s `webcrypto`. **`node:crypto`** is JS: random bytes from
`wasi:random/random` `get-random-bytes` through the ES module `node:internal/wasi_random` (as T59's `wasi_fs`),
the hashes and AES over typed arrays (32-bit word pairs for SHA-512; AES T-tables built at load), CCM per
RFC 3610; node's error codes and OpenSSL's messages. **Load-only built-ins** (`http`, `https`, `http2`, `tls`,
`dgram` until T67, `dns`, `zlib`, `perf_hooks`, `module`, `child_process`) are hand-written with node 24's exports: what can
work without the facility does (`http.Agent`, header validation, `zlib.crc32`, `module._nodeModulePaths`), the rest
throws `<module>.<name>() isn't available in js2wasi3 (<why>)` from `node:internal/unavailable`. Their constants
come from `node:internal/builtin_data`, generated from node 24.21.0 by `tasks/T65-files/gen-data.mjs`.
`child_process` behaves as if no program existed (`spawn <file> ENOENT`, node's event order): WASI has no
processes. T70: a shell command (`exec`, `execSync`, `shell: true`) behaves as with a `/bin/sh` (dash) that finds no
program: it starts, writes `/bin/sh: 1: <program>: not found`, exits 127 after a turn of the event loop, and `exec`
calls back with node's `Command failed: …` error; other shells are missing programs. `createRequire` returns a `require` that throws `MODULE_NOT_FOUND`: modules are linked at build time.
Modules that build `module.exports` in loops list their names in `node::exports` for ES named imports.)
(T66: **`wasi:sockets/*` imports are non-blocking** in a command, unlike T59's other WASI imports: an `async func`
gives a promise, a returned `stream<u8>` is an async iterable of `Uint8Array`s (`$$StreamReader`, with a `cancel()`
for a read in progress: `__intrinsic_stream_cancel_read`), a `stream<u8>` argument is pumped from an async iterable,
a `future` is a promise. Such a command (`command_waits` in the backend) keeps a library's pending-call globals after
its host globals, waits in `run`'s waitable set, applies their events in its callback and waits while any are
pending; it also imports each boundary type's sync cancel-read. **`node:net`** is JS over the ES module
`node:internal/wasi_sockets`: `Socket` is node 24's `stream.Duplex` (writes go through a queue into the `send`
stream; the `receive` stream is pushed with backpressure), `Server` binds, listens and wraps each accepted handle;
`node:internal/net` has `isIP`, address formatting and node's errors for WASI error codes, and `lookupAll` for
`net` and `dns.lookup`.
T67: **`node:dgram`** follows node 24's lib/dgram.js over a JS `UDP` handle (node's `udp_wrap` shape) on
`node:internal/wasi_udp` (apart from `wasi_sockets`, so `net`/`dns` programs don't import UDP): a receive loop is
one pending `receive` call. A pending `async func` import call borrows its resource, so `close` first cancels it:
`__intrinsic_import_cancel(promise)` (`abi.rs` `import_cancel`) finds the call in the pending table, takes it out of
the waitable set and runs a sync `subtask.cancel`; a call that had returned is handled as its event, a cancelled
one's entry and count go and its promise never settles. Multicast, broadcast and buffer sizes aren't in WASI 0.3.)
(T71: **`http`'s server** is node 24.21.0's own `_http_common`, `_http_incoming`, `_http_outgoing`, `_http_server` and
`internal/http`, generated by `scripts/gen-node-http.mjs` like T60's streams: node's internals become
`node:internal/http_support` (no-op async_hooks/perf/diagnostics/trace, `FreeList`, timers) and
`node:internal/http_errors` (T60's errors + the HTTP codes, so stream programs keep their bytes); `Server` is a class
extending T66's class `net.Server`. `internalBinding('http_parser')` is **`node:internal/http_parser`**, a JS llhttp
written from RFC 9112 with the binding's interface (`initialize`/`execute`/`finish`, the `kOn*` callbacks, llhttp's
error codes, `ConnectionsList` for the server's timeouts), checked against node's real binding
(`node --expose-internals`, `tasks/T71-files/parser-diff.js`). The client half (`_http_client`, `_http_agent`)
isn't ported. **Web globals** `Headers`, `Request`, `Response`, `ReadableStream` (default streams), `Blob` are
`node:internal/fetch` (`node::GLOBALS`; loading it also defines them on the global object). **`globalThis.X`**: a
read of `globalThis.X` for X in `node::GLOBALS` loads X's module like a bare `X` and is rewritten to its binding
(not a write, not when the module declares `X` at its top level; srvx reads `globalThis.URL` at load).
**`__intrinsic_species_mode()`**: a gate like the prelude's `__intrinsic_cjs_mode()`, allowed in node's built-in
modules, true when the program outside them names `species` (`Buffer[Symbol.species]` for `ws`; dropped blocks may
hold no functions: they would renumber every function).)
(T72: **`fetch`** is `node:internal/fetch_client` (`node::GLOBALS`; it also defines `globalThis.fetch`) over the ES
module `node:internal/wasi_http`, `wasi:http@0.3.0`'s `types` and `client` (vendored from wasmtime v49.0.1, its
`use wasi:clocks/types@0.3.0` pointed at the vendored 0.3.1), non-blocking like `wasi:sockets`: `client.send` is a
promise, `consume-body`'s stream an async iterable; the sync `request.new`/`consume-body` with future arguments and
results needed nothing new. Apart from T71's classes so `Headers`/`Response`-only programs don't import `wasi:http`
(wasmtime needs `-S http` for a program that does). The host does the connection, TLS and the hop-by-hop headers;
the JS does undici's default headers (no `accept-encoding`: bodies aren't decoded), redirects (Fetch's
HTTP-redirect fetch), `AbortSignal` (`__intrinsic_import_cancel` of the pending `send`, then the body stream's read),
and undici's errors (`TypeError: fetch failed` whose `cause` is node's error for the `error-code`). The response body
calls `consume-body` on its first read, so an unread body leaves nothing for the event loop to wait for (node exits
with one too).)

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
(T36: `Reflect` is a namespace like `Math` (`CLASS_REFLECT`), all in `builtins/reflect.js`: `get`/`set` with a receiver
are the spec's OrdinaryGet/OrdinarySet walks over `Object.getOwnPropertyDescriptor`/`getPrototypeOf`; the writers that
return `false` convert the key, the descriptor and an array `length` first, then catch only the native rejection.
Intrinsics `call_list`, `construct_list` (a `new.target`), `set_proto`, `to_key`, `object_desc`. Built-in functions
as values (`$$…`) are `FnKind::Plain`: not constructors.)
(T35: prelude intrinsics `__intrinsic_is_constructor(v)` and `__intrinsic_named("x")`, a compile-time constant telling
whether the user's code names property `x`, so the prelude can skip paths that can't happen, like species.)
(T21 slice 4: `builtins/object.js` (`Object.assign`/`fromEntries`, the `Object.prototype` methods, installed over the
sentinels like the others, so ToPrimitive calls the prelude's `toString`) and `builtins/json.js` (`JSON.stringify`: the
spec's algorithm writing pieces into one array joined at the end; node's circular-structure message). The intrinsic
`define(o, k, v)` is CreateDataProperty. A static of a global the prelude has statics for but not this one
(`JSON.rawJSON`) is rejected by name.)
(T27: `RegExp` in `builtins/regexp.js`: a RegExp is a `$object` of class `CLASS_REGEXP` with its data `[source, flags,
code]` under the private `G_SYM_STATE` key and an own `lastIndex` (intrinsics `regexp_new`, `regexp_data`). The
pattern is parsed to an AST of arrays and compiled to a program of `i32`s run by a backtracking VM with an explicit
backtrack stack (choices, undo entries for captures and loop registers, lookaround barriers), never JS recursion;
programs are cached by flags and source. Regex literals are `$$RegExpCreate(source, flags)` per evaluation. Hot
prelude data lives in `$i32s` arrays through the intrinsics `i32_new/get/set/len` (typed, unboxed; prelude-only).
`G_REGEXP_EXEC` holds the installed `RegExp.prototype.exec`, so RegExpExec skips the lookup-and-call while it's
the built-in (`regexp_builtin_exec`). `console.log` prints `/source/flags`. `RegExp.prototype[Symbol.match]` etc.
are prelude methods named `$$RegExp_prototype_$match` (a `$name` key is installed as `Symbol.name`), linked from
RegExp creation only when the code names the string method; `String.prototype.split/replace` look up
`Symbol.split/replace` first. Unicode data comes from node itself (`scripts/gen-unicode.mjs`, node's Unicode
version): the `i` flag's case tables (`builtins/unicode-case.js`, always in the prelude) and the `\p{…}` sets
(`crates/lower/src/unicode_props.rs`), of which the prelude gets only those whose `\p{…}` text appears in the
source (`builtins::parse`). The frontend has oxc check regex literals (early SyntaxErrors).
T46: modifier groups `(?ims-ims:…)` are an AST node whose body is compiled between two `MODS` instructions setting
the VM's `i`/`m`/`s` bits (undo-logged on the backtrack stack); the parser and emitter track `i` too. All of it is in
`if (__intrinsic_regexp_mods()) { … }` blocks, kept only when the user's source contains `(?i`/`(?m`/`(?s`/`(?-`.
T39: `STAR` (a greedy loop over one CHAR/ANY/CLASS, forwards) also runs in `u`/`v` mode, a code point per step, and
gives back whole surrogate pairs; `$$reSearch` skips start positions the first instruction rejects; hot helpers take
and return numbers, not booleans (F14).)
(T26: `Date` in `builtins/date.js`: a Date is a `$object` of class `CLASS_DATE` with its data `[time value]` under the
private `G_SYM_STATE` key (intrinsics `date_new`, `date_data`); the setters mutate that array. All of `Date` is
prelude code on `f64`s (the spec's MakeDay/MakeTime/TimeClip, Hinnant's days <-> civil dates); `Date.parse` is the
Date Time String Format, then a subset of V8's legacy parser. `new Date(…)` calls `$$Date`; `Date(…)` calls
`$$DateCall` (a string), which is also the call entry of `Date` as a value (`Module::date_call`); the construct entry
passes the new target as `$$Date`'s `this` (subclasses, `Reflect.construct`). The runtime's ToPrimitive turns hint
default into string for a Date; `console.log` gets a Date's text from `$$dateInspect` (`Module::date_inspect`).
**Local time is UTC**: WASI 0.3.1's `wasi:clocks/timezone` is `@unstable` and wasmtime 49 doesn't implement it (a probe
fails to instantiate), so the local offset is 0, like node with `TZ=UTC` (`scripts/diff.sh` runs node with `TZ=UTC`).
The clock is `wasi:clocks/system-clock.now`, imported by commands only when used (its `instant` comes back in memory
at 16, below `abi::HEAP_BASE`); a library using `Date` is rejected. `toLocale*String` give node's `en-US` forms and
throw for other locales or options (no `Intl`).)
(T40: `builtins/bigint.js` has `BigInt(…)`, StringToBigInt (the runtime's string/BigInt comparisons call it:
`Module::string_to_bigint`), `asIntN`/`asUintN` and the prototype methods, linked only in a BigInt module. Prelude code
that needs a BigInt case in shared built-ins (`Number(1n)`, `JSON.stringify` of a wrapper) puts it in
`if (__intrinsic_bigint_mode()) { … }`: the lowering keeps the block's statements in a BigInt module and drops the
whole `if` otherwise, so the IR of other programs doesn't change.)
(T20: typed arrays and `ArrayBuffer`s are native `$typed`/`$buffer` objects (`crates/backend/src/typed.rs`: element
access, printing) with their constructors and methods in `builtins/typedarray.js` on `Intrinsic::Typed` operations.)
(T47: **resizable and detached buffers** exist only in a module whose user source contains `maxByteLength` or
`transfer` (`ir::Module::rab`, decided on the text like T46's modifiers); other programs keep their bytes. Then
`$buffer`'s `data` is mutable (`resize` allocates a new byte array and copies: node reserves `maxByteLength` up front,
but it can be 2^33 and more, and element access already reads `data` on every access, so moving it costs nothing
there; the byte length stays `array.len(data)`), plus `max` (a JS number, null: fixed-length) and `detached` (a detached
buffer's `data` is empty); `$typed` gets `tracking`. Every native reader of a typed array's length goes through
`typed::Length` (`Helper::Len`: IsTypedArrayOutOfBounds → 0, tracking → (bytes − offset) / size) instead of the
field; the prelude's checks are in `if (__intrinsic_rab_mode()) { … }` blocks (kept only then, like
`__intrinsic_bigint_mode`), and `new ArrayBuffer(n, options)` is `$$ArrayBuffer$resizable` (the name `$$ArrayBuffer` is
aliased to it, so other programs keep the one-parameter function). `%TypedArray%.prototype.values/keys/entries` are
then their own functions (ValidateTypedArray), and the array iterators throw for a view out of bounds.
Subclassing `Map`/`Set`/`ArrayBuffer`/typed arrays: their constructor values' construct entry
(`func::Helper::ConstructFrom`) runs the prelude code, then gives the result `newTarget.prototype` if it's an object;
only in a module with `extends` or the name `construct` (`ir::Module::subclassing`).)
(T48: **`BigInt64Array`/`BigUint64Array`** are kinds 9–10, only in a module whose code names one (`ir::Module::
big_typed`, which implies `bigint`): `Builtin::Typed(9|10)` are outside `Builtin::ALL`, their prototype and constructor
globals come after the stacks' (each group brings the ones before it), and the backend's loops over kinds go to 11
only then (`Cx::typed_kinds`). A read builds a `$bigint` from the 8 bytes; a write from user code calls the prelude's
`$$toBigInt` (`Module::to_bigint`) and stores the low 64 bits of its two's complement (`typed::Helper::SetBig`).
The prelude's BigInt paths (`fill`, `with`, content-type checks of `set` and the constructors) are in
`if (__intrinsic_big_typed_mode()) { … }` blocks.)
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
(T49: `Symbol` is a constructor value like `BigInt`: linked only when used as a value (`typeof Symbol` counts; calls
by name stay `Intrinsic::NewSymbol` and don't link it), its call entry is the prelude's `$$Symbol` and its construct
entry throws (`new Symbol`, `extends Symbol` + `super()`, `Reflect.construct`). Its well-known symbols are own
constants only for the names the code uses (`ir::Module::symbol_statics`, like T25's statics by name).
`Symbol.prototype[Symbol.toPrimitive]` is `$$Symbol_prototype_$toPrimitive`, linked by naming `toPrimitive`.)
(T51: a tagged template ``tag`a${x}b` `` is lowered like a spread call (`Lowerer::tagged_template`: a member tag's
object is `this`) to `CallList(tag, this, [templateObject, ...substitutions])`; the template object comes from
`$$templateObject(site, cooked, raw)` in `builtins/string.js` (GetTemplateObject: frozen arrays, `raw`
non-enumerable), cached by the lowering's site number, linked only when the user's source has a tagged template.
Cooked strings of invalid escapes are `undefined`; raw strings get TRV (CR/CRLF → LF). `String.raw` is prelude
code. `eval` code is compiled once (T42), so its sites are shared by every run of the same `eval` (F226).)
(T50: the `Promise` statics (`all`, `allSettled`, `any`, `race`, `resolve`, `reject`, `withResolvers`, `try`) are the
spec's steps in `builtins/promise.js` (NewPromiseCapability through `new C(executor)`, GetPromiseResolve, the iterator
protocol with IteratorClose; intrinsic `is_promise`); promises, `then` and the job queue stay native. They're linked
by a `Promise.x` reference, or by name when the code names `Promise`, `constructor` or the global object; the
backend's `Promise` value installs them (`ir::Module::promise_statics`, node's order) in place of the native ones,
which stay (same bytes) in programs that link none. `Promise.x(…)` is a direct call with `this` = `Promise`
(`direct_call_this`); `Promise.x` read as a value is the property. `build --unhandled-rejections=none`
(`Module::ignore_unhandled`) reports only the entry module's rejected evaluation promise (a top-level `await`, as
node does whatever the flag; `run` marks it with `handled` bit 2, `promise::MAIN`) and ignores the other unhandled
rejections; the test262 runner uses it, as hosts like d8 don't fail a test on one.)
(T53: `console` as a value is `$$Console()` in `builtins/console.js`, an object made on first use with the prelude
methods `$$Console_log`/`info`/`debug`/`warn`/`error` (rest parameters, the line from `$$consoleFormat`, written by the
prelude intrinsics `write_stdout`/`write_stderr`); not an `ir::Namespace`, whose global would shift every program's
global indices. It's linked only when `console` appears other than as `console.<method>(…)` without spread; the
direct path stays for those (same bytes). `console.log(...xs)` is a call of the object's method. A program that
writes a property of `console` (`Prelude::console_patched`) calls every method through the object. node's other
members (`console.table`, …) are build errors. `builtins/uri.js`: `encodeURI(Component)`/`decodeURI(Component)`
(the spec's Encode/Decode over UTF-8, V8's `URIError: URI malformed`) and Annex B `escape`/`unescape`.
`builtins/clone.js`: `structuredClone` in one pass with a native identity map (cycles, shared references, shared
buffers); a `DataCloneError` is an `Error` whose prototype is a small `DOMException.prototype` (`code` 25, own
`name`), printed as node's `DOMException [DataCloneError]: … could not be cloned.`; a program naming
`structuredClone` keeps function source text for that message.)
(T55: **`process`** and **`performance`** are objects made on first use like `console` (`$$Process()`,
`$$Performance()` in `builtins/process.js`; commands only), linked where the code names them (`process.nextTick` alone
stays T25's direct path); `require("process")` is the same object. `env`, `argv` and `cwd()` come from
`wasi:cli/environment` (`Intrinsic::Environment(k)`: one NUL-separated string per call, the imports only in programs
that use them); `argv` is [WASI arg 0, the entry's absolute path at build time, the WASI arguments after the first].
`platform` is `"linux"` (packages take their POSIX paths; node has no WASI platform), `arch` `"wasm32"`, `version`
node 24.21.0's. It has its own listeners (`on`, `once`, `emit`, …), not `EventEmitter`'s. `process.exit(code)` emits
`'exit'` then `Intrinsic::ProcessExit`: a second exception tag (`host::EXIT_TAG`, only in programs using `process`)
that no JS `catch`/`finally` sees (there's no `catch_all`), caught by `run` and the timer tick, which then run no
more timers and flush the output; `Module::process_at_exit` is the prelude's `$$processAtExit(status)`, which the
event loop calls once before flushing (`'exit'` listeners, `process.exitCode`). The exit status is 0 or 1 (`run`'s
`result`): wasmtime 49 has `wasi:cli/exit.exit-with-code` only behind `-S cli-exit-with-code`. Warnings become
`'warning'` events but aren't printed (node with `--no-warnings`, which the harness uses; node's text has the pid).
`performance.now()` and `process.hrtime()` read the monotonic clock (`Intrinsic::MonotonicNow`, ns).)
(T56: **`util.inspect.custom`**: in programs whose code has the string `"nodejs.util.inspect.custom"` (`util`,
`buffer`, or the user's), the formatter first calls the prelude's `$$inspectCustom(v, depth)` for every object
(`Module::inspect_custom`, `builtins/node.js`): node's formatValue: a method's string result is printed as is, another
value is formatted in its place, `v` itself means none; prototypes are skipped; the method gets the levels left, the
options and an `inspect`. `atob`/`btoa` are prelude globals (`builtins/base64.js`) with node's `InvalidCharacterError`
`DOMException` (`$$domException`, shared with `structuredClone`'s `DataCloneError`).)

### D6. The global object, `eval` and `Function` (T42)
Every file is an ES module (strict, top-level `this` is `undefined`, top-level declarations aren't global-object
properties) or, since T54, a CommonJS module (a function: top-level `this` is `module.exports`, also strict), so the global object is only observable through `globalThis`/`global`, `eval` and `Function`.
- **Gated**: `ir::Module::global_object` is set when the user's code names `globalThis` or `global` (unresolved), or
  (later slices) compiles `eval`/`Function` code; without it the output is byte-for-byte what it was.
- **Representation**: an ordinary `$object` made on first use (`$$GlobalObject()` in `builtins/global.js`), with
  `Object.prototype` as its prototype. Its properties are data properties (writable, non-enumerable, configurable;
  `NaN`/`Infinity`/`undefined` read-only and fixed) for the globals the program can observe: the implemented
  built-ins whose name appears in the user's code (an identifier, a property name or a string, the same rule that
  links the prelude, T25), plus `globalThis`/`global`. A global named nowhere isn't there (like F109).
  `console.log(globalThis)` and `Object.keys` are approximate (node has ~100 more).
- **Built-ins stay static**: `Array`, `Math.max(…)`, `new Map` keep their direct references, so a write that
  replaces a built-in through the global object (`globalThis.X = …`, `delete global.X`,
  `Object.defineProperty(globalThis, "X", …)` for a built-in `X`) is rejected at build time rather than half done
  (F201: a computed key or an alias isn't seen).
- **Undeclared names** (not declared anywhere and not a global node has): with the global object, a read is
  HasProperty + Get on it (else node's ReferenceError), `typeof x` gives `"undefined"` if absent, an assignment
  (strict) Sets an existing property and throws the ReferenceError otherwise. Without it, reads throw as before
  (F139) and writes throw the same ReferenceError at run time.
- **node's globals js2wasi3 lacks** (`navigator`, `URL`, `fetch`, `Blob`, … in `NODE_GLOBALS` but not
  implemented; T63, T54's rule for built-in modules): a build error only where the read always runs when its module
  does (top level, or a CommonJS module's wrapper, outside functions, classes, `try`, branches of
  `if`/`?:`/`switch`/`&&`/`||`/`??` and loop bodies: `lower` `may_skip`); elsewhere it throws
  `ReferenceError: node's global \`X\` isn't available in js2wasi3 (planned: T21)` when it runs, and `typeof X`
  is `"undefined"` (node: `"object"`/`"function"`), so feature tests take their fallback, as on a host without it.
  Likewise a missing static of a built-in namespace (`Reflect.decorate(…)`, `Math.nope(…)`, `Math.NOPE`): a build
  error where it always runs, else node's `TypeError: Reflect.decorate is not a function` when called (the
  arguments aren't evaluated) and `undefined` when read.
- **`eval`/`Function` of code known at build time** (string literals and substitution-free templates): compiled
  ahead of time. `frontend::eval` splices each module's calls into its text before the linker parses it (so the
  linker's renames reach the eval'd code): direct `eval("S")` → `((() => { S' })())`, strict eval code in the
  caller's scope (an arrow: own `var` scope; `this`, `super`, `new.target` kept), S' returning the completion value
  when the call's value is used (a fresh `let`, assigned by expression statements; `if`/loops/`switch`/`try` start
  from `undefined`; a `finally` block's values count only if it completes abruptly). S is parsed alone inside
  `class extends Object { constructor() { S } }` (so `super`/`new.target`/`arguments` parse; the body's span must be
  exactly S, `return` is found by a statement walk), then the module with S in place is parsed with its early
  errors, which checks the caller's context (PerformEval's rules come out the same); an error inside a call's code
  makes that call `throw new SyntaxError(…)` (oxc's message). `eval` calls in S are spliced in further rounds.
  `Function(P…, B)`/`new Function(…)` → `(function anonymous(P\n) {\nB\n})` in place (a new object per
  evaluation, the spec's source text); indirect eval (`(0, eval)(…)`, `globalThis.eval(…)`, `eval?.(…)`) →
  `(function () { S' }).call(globalThis)`. Both are sloppy in node: only the sloppy `this` is done (a non-strict
  `Function` body's `this` is the global object for `undefined`/`null`, else ToObject); a name in them that
  resolves to a module binding is a build error (node: only globals, F206). T44: non-strict indirect eval code's
  `var` declarations become assignments (their names are then undeclared, so global-object lookups) and its
  top-level function declarations named function expressions in a prologue that declares them all on the global
  object first (EvalDeclarationInstantiation: node's TypeErrors, then functions defined or set, then `var`s defined
  `undefined` where absent; configurable); `for (var … in/of …)`, destructuring `var`s and block-level functions
  are rejected. `EvalMap` takes offsets in spliced text back to the file for diagnostics. Code not known at
  build time is a build error pointing at F191; `eval` and `Function` as values are functions (`$$eval` returns
  non-strings, both throw an `EvalError` for code), and `Function` is a constructor value
  (`f instanceof Function`).

### D7. Explicit resource management (T45)
- **`using` is a source rewrite** (`frontend::using`, after the eval splicing, one more `EvalMap` layer), as
  TypeScript's downlevel emit: each statement list (block, function body, static block, `switch`, the module's top
  level) or loop with a `using` declaration gets `const env = { list, error, has }; try { … } catch (e) { record }
  finally { dispose }`, so `break`/`continue`/`return`/`yield` leave through the existing `finally` lowering. A
  declaration becomes `const x = v, r = add(env, x, async)` (names still inferred); `for (using x of it)` binds
  `const x` and its body gets the environment (per iteration). The helpers are plain JS generated once per module;
  with an `await using` the `finally` holds the disposal loop itself, so its `await`s are the spec's
  (DisposeResources, tick order checked against node). A top-level `using` in a module with imports/exports is
  rejected (the `try` can't hold them).
- **`SuppressedError`** is a native error kind outside `ErrorKind::ALL`, and **`DisposableStack`/
  `AsyncDisposableStack`** are `Builtin`s outside `Builtin::ALL`: their prototype and constructor globals come after
  the BigInt ones only when the code names them (`Module::suppressed`/`stacks`; each group brings the ones before it,
  `runtime::runtime_globals`), so other programs keep their bytes. `Symbol.dispose`/`asyncDispose` are well-known
  symbols 12–13 (the array has their slots only then). A stack is an object of class 14/15 with its data under the
  private `G_SYM_STATE` key (like a Date); its methods are in `builtins/dispose.js`.

## Testing
Test host: wasmtime 49.0.2 (`TOOLS.md`). JS oracle: `node`.

| Layer | Where | Checks |
|---|---|---|
| Unit | `cargo test` in each crate | naming, IR validation, runtime helpers, `unsupported` diagnostics (`crates/lower/tests/unsupported.rs`) |
| IR snapshots | `crates/lower/tests/snapshots.rs` | `tests/fixtures/x.js` lowers to `x.ir` (`UPDATE_SNAPSHOTS=1` rewrites them) |
| Components | `crates/js2wasi3/tests/build.rs`, `crates/backend/tests/emit.rs` | exports called with `wasmtime run --invoke` and compared with node's results (`x.out`, or values in the test); `console.js` against `console.out`/`.err` so `cargo test` covers a command without node |
| Commands | `scripts/diff.sh` | node vs js2wasi3 + `wasmtime run -Sp3`: stdout, stderr and exit status |
| Conformance | `scripts/test262.mjs` | a test262 subset (T23); only at the end of a task or on request |

**Differential harness** (T11): `scripts/diff.sh [-j jobs] [fixture.js | dir]...`
- The default is `tests/fixtures` and its subdirectories (T12: `exceptions/`), except `tests/fixtures/unsupported/`
  (rejection tests, not diff targets).
- It builds the compiler once, then runs the fixtures in parallel (10 s timeout each). It prints PASS/FAIL/SKIP with
  the time per fixture (SLOW at ≥ 2 s), the diffs or build diagnostics of failures, and a summary. Exit 1 on any failure.
- `x.skip` next to `x.js` skips it; its first line is the reason (e.g. libraries: exports, nothing to run).
- `x.args` next to `x.js` (T55): the program's arguments (one line of shell words), given to both; `x.env`:
  `NAME=value` lines, added to node's environment and passed to wasmtime with `--env` (wasmtime passes no other
  variables, so such a fixture prints only these).
- `x.uncaught` next to `x.js` (T12, F29): the program ends with an uncaught exception. node's stderr (source excerpt,
  stack) is reduced to `Uncaught <message>` by `scripts/node-uncaught.mjs` before the comparison.
- Fixtures are ES modules for node too (`tests/fixtures/package.json`: `"type": "module"`), so strict mode matches.
  T54: `tests/fixtures/commonjs/` has its own `package.json` without `"type"`; like `modules/`, each `<case>/main.js`
  there is a program's entry. T55: `tests/fixtures/node/` has node's built-in modules.
- A command fixture is self-checking: it prints its results, and node's output is the expectation, so no `.out`
  file is needed. Keep each one under 2 s. Constructs that aren't supported yet show up as build failures.
- `cargo test` doesn't run it (it needs node, F30).
- node runs with `--no-warnings`: process warnings (node 24's `TimeoutNegativeWarning`, …) start with `(node:<pid>)`
  and aren't JS behaviour.
- Timers (T19): node refreshes its clock whenever a timer is made (ms steps), so the order of near-tied deadlines,
  or of a timer made inside a callback against others due soon, depends on node's speed and the machine's load.
  Keep such deadlines far apart in fixtures (see `tests/fixtures/async/timers.js`).

**test262** (T23): `node scripts/test262.mjs [-j N] [--list] [--causes N] [--update] <dir | file | subset>...`
- test262 is a blobless sparse clone in `test262/` (gitignored) at the commit pinned in `TOOLS.md`: `scripts/fetch-test262.sh`.
- Each test is compiled as one ES module (always strict; T54: the work directory's `package.json` says `"type":
  "module"`, since node's syntax detection would make some tests CommonJS) after its harness (`print` shim, `sta.js`, `assert.js`,
  `doneprintHandle.js` for async, `includes`) and run with `wasmtime run -Sp3` (no compile cache, opt-level 0,
  single-pass register allocator: compiling dominates). Statuses: pass / fail / unsupported (build rejected with
  `unsupported:`) / crash (compiler panic) / timeout / skip (`noStrict`, `raw`, `_FIXTURE`, module resolution, `$262`,
  features node 24 lacks (`NODE_MISSING` in the script), script only: the build failed and `node --check` rejects the source as a module too).
- It prints counts per directory and the tests whose status differs from `tests/test262/baseline.json`; `--update`
  merges the run into it. Exit 1 if a test that passed there doesn't now. Details: `out/test262/results.tsv`.
- Known noise: modules, not scripts (top-level `this`, global-object `var`s, sloppy `eval`/`Function` code); an
  unhandled rejection is fatal like node's (e.g. `async-function/evaluation-body.js`). A script's function
  declarations are global-object properties: for async tests including `asyncHelpers.js` the runner adds
  `globalThis.$DONE = $DONE;` (T42).

## Working rules
- One task file = one session. Don't start the next task without approval.
- Each iteration runs only that task's tests, `wasm-tools validate` and one wasmtime smoke run.
  Broad suites run only at the end of a task or on request.
- Unsupported syntax reports the construct and its source location. It never miscompiles silently.
- No C, Rust or LLVM IR source is generated as an intermediate step. Wasm is emitted directly.
