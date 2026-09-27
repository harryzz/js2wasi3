# T20: WIT async and user worlds

Status: in progress (slices 1–4 done; next: slice 5, imports) | Depends on: T19

## Goal
Accept a user `.wit` world. Map exported and imported functions with D4's conventions, including `async func` ↔ async JS functions,
`future<T>` ↔ Promise, `stream<u8>` ↔ async iterable of `Uint8Array`, records, variants, options, results and lists.
Imported interfaces become ES imports (`import { f } from "pkg:ns/iface"`).

## Acceptance
- A library world with 1 async export and 1 async import, composed with a test component via `wac`, runs on wasmtime.
- A stream export echoes input bytes.

## Notes / Result

### Slice 1: `for await`, async generators, typed arrays (done 2026-09-26)
- **`for await` (1a, F72):** `patterns.rs` lowers it like `for-of` with `Intrinsic::AsyncIterOpen` (the
  `Symbol.asyncIterator` iterator as `[iterator, next]`, else the sync `$iter`), `AsyncIterNext`/`AsyncIterResult`
  (await `next()`, TypeError for a non-object) and, on early exit, `AsyncIterReturn` (awaited; errors ignored when
  leaving by a throw). A sync iterable follows CreateAsyncFromSyncIterator's ticks: each value is awaited plus one
  more tick, a rejected value closes the sync iterator first, closing takes 1 tick (2 with a `return` method).
  Top-level `for await` makes `main` async (`Lowerer::await_value`). node's messages (`x is not async iterable`,
  `Iterator result 1 is not an object`, `number 5 is not a function`).
- **Async generators (1b, F72):** `ir::FnKind::AsyncGenerator`, `Intrinsic::NewAsyncGenerator`,
  `GenSuspend { awaiting }`; the lowering awaits `yield`'s operand, `return`'s operand and a `return()`
  resumption's value. `crates/backend/src/asyncgen.rs`: the request queue (the frame's extra slot), the driver
  (`Step`: `await` points wait like an async function's, `yield`s fulfill the head and continue with the next
  request), `%AsyncGeneratorPrototype%` `next`/`return`/`throw`/`[Symbol.asyncIterator]`, draining and
  AwaitReturn after completion; `asyncfn`'s step hands async-generator frames over (gated on `$asyncgen`). Printed
  `Object [AsyncGenerator] {}` / `[AsyncGeneratorFunction: f]`, tags. `yield*` inside one is rejected (F115).
- **Suspension transform fix:** every suspension point used to inline "save all locals" (code = points × locals:
  five `for await` loops made 182 KB and took wasmtime 4 s to compile). Now each point sets the value, point and
  kind and breaks to one shared saving tail per resume function (71 KB, 0.3 s). The frames still save every local
  (F117).
- **Typed arrays (1c):** `$buffer`/`$typed` runtime types, `crates/backend/src/typed.rs` (little-endian element
  reads/writes with ToInt32 / clamping / `f32` rounding, `TypedOp`s, printing like node), fast paths in
  `object::get`/`set`/`has`/`own_keys` and `builtin_tag` (gated on `$typed`), `builtins/typedarray.js` (the nine
  constructors, `from`/`of`, `%TypedArray%.prototype` methods and getters, `ArrayBuffer`), `BYTES_PER_ELEMENT`,
  native `values`/`keys`/`entries`/`[Symbol.iterator]`.
- Tests: `async/for-await.js`, `for-await-order.js` (tick-exact), `async-generators.js`, `builtins/typed-arrays.js`,
  `typed-arrays-more.js`; `unsupported.rs`, IR tests for the transform. `cargo test` ok; `scripts/diff.sh -j 4`:
  97 passed, 0 failed, 11 skipped.
- `hello.js`: 55715 bytes (+348 over 55367): always-declared global slots of the new built-ins (the async
  generator prototype, `ArrayBuffer`, `%TypedArray%`, nine typed arrays) and two struct types; no new code (F113).

**Not done (follow-ups):** F115 (`yield*` in async generators), F116 (typed array leftovers), F117 (frames save
every local at each suspension).

**Next smallest step:** slice 2, library runtime: exports drain the job queue and report unhandled rejections
(F65), then library top-level code (F11).

### Slice 2: library runtime (done 2026-09-27)
- **Library init (F11):** a library's top-level code (anything beyond functions, exports and literal globals) is
  lowered like a command's `main` into `ir::Module::init` (same globals, TDZ flags, classes). The export adapter
  calls `abi::Helper::Init` (`module_init`) first: it runs `init` once (state and error in two library-only
  globals after the bump pointer); an `init` that threw throws the same error on every call, like a failed module
  evaluation. Every export of such a library goes through the adapter. A library with `init` links the JS
  runtime (its globals come after the runtime's). Top-level `await` in a library is rejected (F120).
- **Jobs (F65):** `ir::can_queue` (module-wide: an `async` function or generator, `Promise`, `queueMicrotask`,
  timers) sets `AbiSig::jobs` on every export, and `throws` with it. `abi::Helper::Settle` (`settle_jobs`) runs
  after `init` and after each call: drain the job queue, then throw the reason of the first unhandled rejection,
  so the call returns `err` (an error's name and message; a non-error: `name: ""` and its `console.log` text). An
  exception thrown by a job fails the call the same way.
- Tests: `tests/fixtures/library/{init,init-throws,jobs}.js` (`.skip`: libraries) checked in
  `crates/js2wasi3/tests/build.rs` (`library_init_runs_first`, incl. a numbers-only library;
  `library_calls_run_their_jobs`); `unsupported.rs` (`module_level_rules`, `classes`) updated. `cargo test` ok;
  `scripts/diff.sh -j 4`: 97 passed, 0 failed, 14 skipped; `hello.js` 55715 (unchanged: commands don't change).
- Commands run: `cargo build --release`, `cargo test`, `scripts/diff.sh -j 4`, `wasm-tools validate` and
  `wasmtime run --invoke` on the fixtures.

**Not done (follow-ups):** F118 (state across calls on one instance is untested: `--invoke` makes one call per
instance), F119 (every export of a library that can queue jobs or whose init may throw returns `result`), F120
(top-level `await` in a library). F83 (`Math.random`/`wasi:random` in libraries) and `console`/timers in
libraries stay open: they need imports in library worlds, and timers need async exports.

**Next smallest step:** slice 3, async exports: `export async function f` → `async func` in the generated world,
lifted with the async callback ABI (reuse `host::Helper::Loop` for timers and jobs), `task.return` of the settled
value or `js-error`; test with `wasmtime run --invoke`.

### Slice 3: async exports (done 2026-09-27)
- **`export async function f`** is a WIT `async func` (`AbiSig::is_async`; `ExportSig::is_async`), always
  returning `result<T, js-error>` (the promise can reject). `T` is what the promise settles to: JSDoc
  `@returns {Promise<string|number|void>}` (or a plain `{string}`/`{number}`), else the kinds of the `return`
  values (`Slot::Settled`: none → `_`, strings → `string`, else `f64` by ToNumber). `Promise<…>` on a non-async
  function is an error. The `$export` wrapper converts only params; the backend converts the settled value.
- **Lift (callback ABI, D3):** per async export the core module has its adapter in `abi::Mode::Start` (runs
  `init`, calls the function, keeps the promise in the library globals, now `abi::LIB_GLOBALS` = 7), a
  `Mode::Finish` adapter (the `result` in a return area, via `abi::Helper::Await`), `[async-lift]f`
  (`abi::async_lift`) and `[callback][async-lift]f` (`abi::async_callback`), and imports `[export]$root`
  `[task-return]f` (the return area loaded as flat values: `result<f64, js-error>` is `i32 i64 i32×6`, the others
  `i32×8`). `abi::Helper::Step` runs the jobs (and `nextTick`s); the call is done when its promise settled, a job
  threw, a rejection went unhandled, or nothing is left to wait for (then `err`: "the promise of an async export
  never settled …"); otherwise it arms one `wait-until` subtask for the earliest timer (like `host::Helper::Loop`),
  raises backpressure (no other call starts while it waits: they would share the timers) and returns `WAIT`. The
  callback (`Event` → `Tick`) runs the due timers; an exception there or an unhandled rejection fails the call
  (node would crash). `End` cancels the wait and lowers backpressure; timers still pending run in a later async call.
- **Timers in libraries:** allowed when the library has an async export (else rejected with the reason). Host
  imports are now looked up through `Cx::imports()` (`host::Imports`; a library imports its `task.return`s,
  then `host::LIBRARY_TIMER_IMPORTS` if it uses timers). A world with an async export imports
  `wasi:clocks/monotonic-clock` (wit-component drops it when unused).
- Tests: `tests/fixtures/library/async.js` (values, strings, void, rejections, a pending promise, a stray
  rejection, `export default async`), `library/async-timers.js` (sleep, interval, immediate + nextTick order, a
  throwing timer, `Promise.race`), checked in `crates/js2wasi3/tests/build.rs` (`async_exports_await_their_promise`,
  `async_exports_wait_for_timers`; results as node's `await m.f()`); `unsupported.rs` and the component unit test
  updated. `cargo test` ok; `scripts/diff.sh -j 4`: 97 passed, 0 failed, 16 skipped; `hello.js` 55715 (commands
  unchanged).
- Commands run: `cargo build --release`, `cargo test`, `cargo clippy --all-targets` (one old warning),
  `scripts/diff.sh -j 4`, `wasm-tools validate`, `wasm-tools component wit`, `wasmtime run --invoke` on the fixtures.

**Not done (follow-ups):** F120 (top-level `await` in a library) stays open; F121 (timers left after an async call,
and timers set by sync exports, run only during a later async call), F122 (async export calls ignore
cancellation), F123 (never-settling promise: `err` instead of node's hang / exit 13). F118 and F119 apply to
async exports too.

**Next smallest step:** slice 4, user worlds: `--wit <file|dir> --world <name>` parsed with `wit-parser`; check
the module's exports against the world (sync vs `async func`, parameter count), map the scalar WIT types first
(`bool`, integers, `f32`/`f64`, `char`, `string`), then `list`, `option`, `result`, `record`, `variant`, `enum`,
`flags`, `tuple` (F13, F21, F55); the world decides which exports return `result` (F34/F60/F119).

### Slice 4a: user worlds with scalar types (done 2026-09-27)
- **CLI:** `js2wasi3 build m.js --wit <file|dir> [--world <name>]` (`--world` selects the world in those files: a
  plain name or `ns:pkg/world`). `component::load_world` pushes the vendored WASI 0.3.1 packages, then the user's
  path (`Resolve::push_path`), and lists the world's function exports (type aliases resolved); non-scalar types
  and interface exports are errors naming the function and type. `match_world` (in `main.rs`) pairs each world
  export with the JS export whose name converts to it (or a string export name): a world export with no JS
  export, or a JS export not in the world, is a diagnostic. The component is encoded against the user's
  `Resolve` (`World::user`); a module with timers whose world lacks `wasi:clocks/monotonic-clock` gets a hint.
- **Lowering:** `js2wasi3_lower::lower_with(program, Some(&Boundary))` (`WorldSig` by JS export name). Pass 1
  checks sync vs `async func` and the parameter count, and observes each parameter's kind from its WIT type (so
  strings/chars are strings, bools booleans, the rest numbers). `wrap_exports` takes the world's signature
  (`throws` = the world has a `result`; none in 4a, so a throw traps, as in jco) and converts through
  `convert_any` / `to_boundary` (ToNumber, ToInt32 for the integers, ToBoolean, ToString).
- **IR:** `ir::AbiType` has every WIT scalar; `param_type` (numbers are `f64`, `bool` is `Bool`, `char`/`string`
  JS strings) and `result_type` (integers up to 32 bits are `i32` from ToInt32).
- **Backend adapter** (`abi::adapter`, rewritten): per-type lifting (`f64.convert_i32_s/u`, `i64`, `f32`
  promote, `char` → a one-code-point string, `abi::Helper::FromCodePoint`) and lowering (narrowing with
  `extend8_s`/`and 0xff`/…, `i64.trunc_sat_f64_s/u`, `f32.demote`, `char` = first code point with U+FFFD for a
  lone surrogate, `Helper::CodePointAt0`); `result<T, js-error>` return areas and joined `task.return` params for
  any scalar `T`; async exports without `result` return their value flat. Helpers and linear memory are only
  used when needed (`abi::needs_memory`): a numbers-only `u32` library has neither the JS runtime nor memory.
  Every export whose types aren't all `f64` goes through an adapter.
- Tests: `tests/fixtures/library/scalars/{world.wit,scalars.js}` (every scalar, a type alias, a string export
  name, async `u32`/`char`, a trap) and a numbers-only world, in `crates/js2wasi3/tests/build.rs`
  (`user_world_scalars`, `user_world_mismatches`: missing/extra exports, async and parameter-count mismatches, a
  `list` type, the missing clock import). `cargo test` ok; `scripts/diff.sh -j 4`: 97 passed, 0 failed, 16
  skipped; sizes unchanged for commands and generated worlds (`hello.js` 55715, `add.js` 289), `library/async.js`
  +80 bytes (adapter locals).
- Commands run: `cargo build --release`, `cargo test`, `cargo clippy --all-targets` (the old warning only),
  `scripts/diff.sh -j 4`, `wasm-tools validate`, `wasmtime run --invoke` on the fixtures.

**Not done (follow-ups):** F124 (scalar conversions differ from jco: 64-bit numbers not BigInt, wrapping instead
of range checks, `char` from the first code point), F125 (interface exports; exact parameter count), F126 (a WIT
dir with its own WASI `deps/`). F13 is done for scalars in user worlds; F34/F60/F119 are resolved by a user world.

**Next smallest step:** slice 4b: `result<T, E>` in user worlds (a throw is `err`: decide how a JS exception
maps to `E`: `string` = its message, a record like `js-error`, …), then `option<T>` (↔ `undefined`), `tuple`,
`record` (↔ plain object), `enum` (↔ string), `list<T>` (`list<u8>` ↔ `Uint8Array`), `variant`, `flags`; these
need the canonical ABI in memory (lists, > 16 flat params, F21).

### Slice 4b-1: fixed-size compound types (done 2026-09-27)
- **Types:** `ir::WitType` (a tree: `Scalar`, `Option`, `Result`, `Tuple`, `Record`, `Enum`, `Flags`) referred
  to by `AbiType::Compound(i)` into `AbiSig::types`; `AbiSig::err` (`ErrType::JsError` for generated worlds,
  `Empty`, `Of(t)`). `component::Ty` / `boundary_type` read them from wit-parser (aliases resolved; `list`,
  `variant`, `future`/`stream`, resources and flags > 32 are errors naming the slice that adds them); a top-level
  `result<T, E>` becomes `throws` + `err`. The CLI converts record fields and flags to camelCase JS keys (`wit_type`).
- **JS shapes (jco):** record / flags ↔ object (camelCase keys), tuple ↔ array, option ↔ the value or
  `undefined` (`null` lowers to `none` too), enum ↔ its WIT case name, a `result` below the top ↔
  `{ tag: "ok" | "err", val }`. A top-level `result` is return or throw, the thrown value made `E` by jco's
  `getErrorPayload` (own `payload`, else an Error traps, else the value; decided with the user). A value that
  doesn't fit (an enum string that isn't a case, a `tag` that's neither) traps.
- **Backend:** `crates/backend/src/wit.rs`: `flatten` (with variant joins), `size_align` / `variant_layout`,
  `lift_flat` (params → JS values), `lower_mem` (JS values → memory), `flat_load` (memory → flat values, per case
  in an `if`, coerced to the joined types). The adapter is built in a buffer with `wit::Gen` locals; every
  result goes through one path: the return area, then its one flat value or the pointer. An async export's
  `Mode::Finish(task_return)` adapter now calls `task.return` itself (flat values loaded from the return area, a
  pointer past 16), so the lift and callback shrink to `step` / `finish` / `end` (the hand-written `js-error`
  loads are gone).
- Tests: `tests/fixtures/library/compound/{world.wit,shapes.js}` (every type, nested ones, `result<_, enum>`,
  `result` with no payload, a thrown Error with `payload`, an async `result<record, string>`, two traps) in
  `crates/js2wasi3/tests/build.rs` (`user_world_compound_types`). `cargo test` ok; `scripts/diff.sh -j 4`: 97
  passed, 0 failed, 16 skipped; `hello.js` 55715, `add.js` 289; `library/async.js` 76153 (+174: the generic
  `js-error` load in 11 `finish` adapters), `library/jobs.js` +12.
- Commands run: `cargo build --release`, `cargo test`, `cargo clippy --all-targets`, `scripts/diff.sh -j 4`,
  `wasm-tools validate`, `wasmtime run --invoke`.

**Not done (follow-ups):** F127 (`option<option<T>>`: `some(none)` and `none` are both `undefined`), F128 (a JS
value that doesn't fit its WIT type traps; jco throws a TypeError, which a `result` export would return).

**Next smallest step:** slice 4b-2: `list<T>` (params: lift from memory into an array; `list<u8>` ↔ `Uint8Array`
with the typed-array runtime; results: allocate with `cabi_realloc`, lower each element), then `variant`
(`{tag, val}`, like the nested `result`), then parameters in memory past 16 flat values (F21).

### Slice 4b-2: lists, variants, params in memory (done 2026-09-27)
- **Types:** `ir::WitType::{List, Variant}`, `component::Ty::{List, Variant}` (fixed-length lists are rejected).
  `WitType::typed_kind`: a `list` of `s8`/`u8`/`s16`/`u16`/`s32`/`u32`/`f32`/`f64` is the matching typed array,
  as in jco (`list<u8>` ↔ `Uint8Array`); 64-bit lists are plain arrays of numbers (F124). Other lists are arrays;
  a variant is `{ tag, val }` with the case's WIT name, and no `val` for a case without a payload (jco's
  variant; a nested `result` keeps `val: undefined`).
- **Backend (`wit.rs`):** lists flatten to `(ptr, len)`, `(8, 4)` in memory; variants like `result` with
  `n` cases. Lifting: `lift_mem` (= `flat_load` + `lift_flat`) per element; a typed array is made with
  `TypedOp::New` and its bytes copied. Lowering: `cabi_realloc` for the elements; a typed array of the same kind
  is copied byte by byte, any other value as an array-like (`length`, then each index). `result` and `variant`
  share `lift_cases`/`lower_cases` (a `tag` that isn't a case traps). The lowering sets `$typed` when a world
  has typed lists (element access and printing).
- **Params past 16 flat values (F21):** the export takes one pointer to the params' tuple (`abi::flat_params`);
  the adapter loads the flat values from it into locals, then lifts as usual (before resetting the bump
  allocator). The backend's rejection is gone.
- Tests: `tests/fixtures/library/lists/{world.wit,lists.js}` (lists of numbers/bytes/strings/records/lists,
  `f32`/`u64` lists, variants with and without payloads, a `result<variant, string>`, 16 + 3 and 9-string
  params, an async `list<u8>`, a trap) in `crates/js2wasi3/tests/build.rs` (`user_world_lists_and_variants`;
  results checked against node calling the module with jco-shaped arguments); `user_world_mismatches` now uses
  `future<u8>` as its unsupported type. `cargo test` ok; `scripts/diff.sh -j 4`: 97 passed, 0 failed, 16 skipped;
  sizes unchanged (`hello.js` 55715, `add.js` 289, `library/async.js` 76153, `library/jobs.js` 65430).
- Commands run: `cargo build --release`, `cargo test`, `cargo clippy --all-targets` (the old warning only),
  `scripts/diff.sh -j 4`, `wasm-tools validate`, `wasmtime run --invoke`.

**Not done (follow-ups):** F129 (lists are copied one byte / element at a time), F55 (rest params in exports)
stays open: a WIT signature can't take a variable number of params; a `list` param would be a different JS
signature. F127, F128 retargeted to later.

**Next smallest step:** slice 5, imports: `import { f } from "ns:pkg/iface"` for a user world's imported
functions (sync first: the adapters in reverse, lowering JS values into the flat params / memory and lifting the
result), then `async func` imports via `[async-lower]` + a subtask in the waitable set, surfaced as a Promise.
Acceptance: a library world with one async export and one async import, composed with a test component via `wac`.

## Slices (agreed 2026-09-26)
1. **JS prerequisites for streams** (F72 and typed arrays; the user chose to do these first):
   a. `for await (x of it)` in async functions and top-level code: `Symbol.asyncIterator`, else the sync iterator
      with each value awaited (CreateAsyncFromSyncIterator); `return()` on early exit; the TypeErrors node gives.
   b. Async generators (`async function*`, methods, `yield`, `await`, `return`, `yield*` if it fits):
      `%AsyncGeneratorPrototype%` `next`/`return`/`throw` returning promises, with the request queue.
   c. Typed arrays: `Uint8Array` first (constructor from a length / array-like / iterable / `ArrayBuffer`, indexing
      with wrap-around, `length`, `subarray`/`slice`/`set`/`fill`, iteration, `console.log` like node), then the
      other element types if cheap; `ArrayBuffer` as far as `Uint8Array` needs it.
2. Library runtime (the handoff's slice 1). 3. Async exports. 4. User worlds. 5. Imports. 6. `future`/`stream`.

## Handoff (written 2026-09-26, end of the T25 session)

**State:** written after T25; updated 2026-09-26 after slice 1 (`for await`, async generators, typed arrays: see
Notes / Result). The slice numbers below are the pre-slice-1 ones: "Suggested slices" 1–5 are now slices 2–6 (see
"Slices"). Commit/push only when the user asks; no attribution lines.

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo build -q --release && cargo test -q --no-fail-fast 2>&1 | grep -E 'FAILED|panicked'   # nothing
scripts/diff.sh -j 4 | tail -1                              # 97 passed, 0 failed, 16 skipped (after slice 3)
target/release/js2wasi3 build tests/fixtures/hello.js -o /tmp/h.wasm && stat -c %s /tmp/h.wasm   # 55715
```

**Read first:** `docs/design.md` D3 (event loop: callback lift, `Helper::Loop`) and D4 (WIT conventions),
`docs/wit-names.md`, `TOOLS.md` (async core names from wit-component, probe results), probes 2, 5, 8
(`probes/cmd.wat`, `cmd2.wat`, `timers.wat`: hand-written P3 callback loops), then the code:
- `crates/component/src/lib.rs`: generates the world from `ExportSig`s (params/results only `F64`/`String`,
  `result<T, js-error>` for throwing exports), embeds it, encodes with wit-component. `--world ns:pkg/world` names it.
- `crates/backend/src/abi.rs`: library export adapters (flat params, strings through `cabi_realloc` + a bump
  allocator reset per call, return area, `js-error`). Sync lifts only.
- `crates/backend/src/host.rs`: a command's `run` export (async callback lift), the event loop (`Loop`, `Tick`,
  `Callback`), stdout/stderr streams; `timers.rs`, `promise.rs` (job queue, `Drain`, `Unhandled`).
- `crates/lower/src/lib.rs`: export collection and every `"T20"` rejection (`console`, timers, `Math.random`,
  `process.nextTick` in a module with exports; grep `"T20"`).

**Follow-ups picked up in T20** (rows in `tasks/README.md`): F11 (library top-level code / module init), F12
(`--emit-wit`, optional), F13 (types beyond `f64`/`string`), F15 (wasmtime `--invoke` keyword names: CLI-only),
F21 (> 16 flat params), F26 (callback ignores `TASK_CANCELLED`; output pumping), F34/F60 (`ir::may_throw`: a user
world decides `result`), F55 (rest params in exports), F65 (library exports never drain jobs / report unhandled
rejections), F83 (`wasi:random` in library worlds).

**Suggested slices (smallest first; like T25, log each in Progress; agree the split with the user first):**
1. **Library runtime:** exports drain the job queue after the call and report unhandled rejections (F65);
   libraries run top-level code once before the first export (F11); `wasi:random` and stdout/stderr imports in
   library worlds so `Math.random`/`console`/timers stop being command-only (F83; timers in a library need the
   async lift of slice 2).
2. **Async exports:** `export async function f` → `async func` in the generated world, lifted with the callback
   ABI: run until the promise settles, `task.return` its value (or `js-error`), with the same loop as `run`
   (timers, microtasks). Test with `wasmtime run --invoke`.
3. **User worlds:** `--wit <file|dir> --world <name>` parsed with `wit-parser` (already a dependency): check the
   module's exports against the world, map WIT types per D4 (bool, integers, `f32`/`f64`, `char`, `string`,
   `list<T>`, `option`, `result`, `record`, `variant`, `enum`, `flags`, `tuple`; F13, F21, F55); the world then
   decides which exports return `result` (F34/F60). Generated worlds keep working without `--wit`.
4. **Imports:** `import { f } from "ns:pkg/iface"` (today every `import` is rejected: `docs/keywords.md`) → lowered
   imports with the adapters in reverse; `async func` imports via `[async-lower]` + a subtask in the waitable
   set, surfaced as a Promise (like `wait-until` in `host.rs`). Acceptance: a library world with one async export
   and one async import, composed with a test component via `wac` (installed: `wac-cli 0.11.0`), runs on wasmtime.
5. **`future<T>` / `stream<u8>`:** future ↔ Promise; `stream<u8>` ↔ async iterable of `Uint8Array` (`for await`,
   async generators and typed arrays exist since slice 1; `yield*` in async generators doesn't: F115). Acceptance:
   a stream export echoes its input bytes.

**Pitfalls carried over:**
- Wasm `select` picks its **first** operand when the condition is non-zero; check every `select`/`typed_select`.
- Size gates: anything not always needed is gated (`ir::Module::names` incl. the `$timers`/`$immediates`/`$ticks`
  markers the lowering adds, `builtins::GATED`, `has_maps`, `has_tags`, …); every runtime global is declared in
  every module (F113), so new globals cost hello.js bytes: say so in Notes.
- `git stash` + rebuild for size comparisons leaves a stale `target/release` binary: rebuild after `stash pop`.
- Fixtures are ES modules (`tests/fixtures/package.json`); scratch files outside run as CommonJS in node, which
  changes `process.nextTick` vs microtask order.
- Subtasks must leave their waitable set (`waitable.join h 0`) before `subtask.cancel` (probe 8).

## Progress
- 2026-09-26: plan: slice 1 = for await (1a), async generators (1b), typed arrays (1c), before the WIT slices (user's choice). Status in progress.
- 2026-09-26: 1a done: `for await` (Intrinsic::AsyncIterOpen/IsAsyncIter/AsyncIterNext/AsyncIterResult/AsyncIterReturn/IterHasReturn; iter.rs helpers; patterns.rs for_await_step/for_await_close; generators.rs await_value marks main async). Sync fallback takes node's 2 ticks per step (value await + one more; a rejected value closes the sync iterator first), close: 1 tick, 2 with a sync `return`; async close awaits `return()`, ignoring its errors when leaving by a throw. async/for-await-order.js (tick-exact vs node), for-await.js PASS; async 12/0; unsupported test updated.
- 2026-09-26: 1b done: async generators. IR: FnKind::AsyncGenerator, Intrinsic::NewAsyncGenerator, GenSuspend { awaiting } (yielded 3); lowering: `yield v` awaits v, `return v` awaits v, a `return()` resumption awaits its value, `yield*` in them rejected (follow-up); backend `asyncgen.rs` (request queue in the frame's extra slot, Step/Method/Drain/AwaitReturn/Settle; asyncfn::Step hands async-generator frames over, gated on `$asyncgen`), `%AsyncGeneratorPrototype%` (next/return/throw, [Symbol.asyncIterator]), CLASS_ASYNC_GEN, KIND_ASYNC_GENERATOR (`[AsyncGeneratorFunction: f]`, `Object [AsyncGenerator] {}`, tags). async/async-generators.js matches node tick for tick.
- 2026-09-26: found while testing: the for-await fixtures timed out: ir::suspend inlined "save all locals" at every point (code = points × locals; 5 for-await loops → 182 KB, 4 s wasmtime compile). Now every point breaks to one shared saving tail per resume function (only the GenSuspend kinds used): 71 KB, 0.3 s. IR tests updated. cargo test ok; diff.sh 95/0/11; hello.js 55390 (+23: the AsyncGenerator prototype's global slots).
- 2026-09-26: 1c done: typed arrays. IR Builtin::{ArrayBuffer, TypedArray, Typed(k)}, TYPED_NAMES/SIZES, Intrinsic::Typed(TypedOp); runtime types $buffer/$typed (T_BUFFER 25, T_TYPED 26); backend typed.rs (element get/set with node's conversions, ops, printing `Uint8Array(n) [ … ]` / `ArrayBuffer { [Uint8Contents]: <…>, [byteLength]: n }`, names); object get/set/has/own_keys and builtin_tag handle typed arrays (gated on `$typed`, set by the lowering); prelude builtins/typedarray.js (constructors, from/of, %TypedArray%.prototype methods and getters, ArrayBuffer); BYTES_PER_ELEMENT; native values/keys/entries/[Symbol.iterator] on %TypedArray%.prototype. builtins/typed-arrays.js, typed-arrays-more.js PASS; cargo test ok; diff.sh 97/0/11; hello.js 55715 (+325: 22 more always-declared global slots and 2 types, F113).
- 2026-09-26: slice 1 done: Notes / Result, README (F72 ticked, F115–F117, Next up), design.md D3/D5, keywords.md, README.md.
- 2026-09-26: handoff refreshed for slice 2 (state, baseline 97/0/11, hello.js 55715).
- 2026-09-26: slice 2 started (F65, F11). Baseline: cargo test ok, diff.sh 97/0/11, hello.js 55715. Design: a library's top-level code becomes `ir::Module::init` (lowered like a command's `main`); the export adapter runs it once before the first call (state + error in 2 library-only globals after the bump pointer: an init that threw rethrows its error on every call, like a failed ES module evaluation), drains the job queue after init and after each call, and fails the call with the first unhandled rejection's reason. `AbiSig::jobs` (drain after the call) = the module can queue jobs && `ir::may_queue(f)`; `throws` also covers jobs and a throwing/queuing init. Top-level `await`, console, timers stay rejected in libraries (async exports: slice 3).
- 2026-09-27: F11 + F65 implemented. IR: `Module::init`, `AbiSig::jobs`, `ir::can_queue` (module-wide: implicit calls can run any function); lowering: library top-level code goes through the command path into `init` (top-level `await` in a library rejected); backend: `abi::Helper::Init` (`module_init`: state + error globals, only in libraries with init) and `Helper::Settle` (`settle_jobs`: drain, throw the first unhandled rejection's reason), every export of a library with init goes through the adapter. Fixtures `library/{init,init-throws,jobs}.js`: all `--invoke` results as node gives (init's Map/class/JSON, init error on every call, jobs drained before returning, rejection in a job → err).
- 2026-09-27: libraries with only literal globals keep the old path (no `init`: `static_globals`); a numbers-only library with `init` now links the JS runtime (its globals were misnumbered: emit test). unsupported tests updated. cargo test ok; diff.sh 97/0/14; hello.js 55715.
- 2026-09-27: slice 2 done: Notes / Result, README (F11, F65 ticked; F118–F120; Next up: slice 3), design.md D4, lower docs.
- 2026-09-27: slice 3 started (async exports). Checked: `wasmtime run --invoke` runs an `[async-lift]` callback export without flags; wit-component names `[async-lift]f`, `[callback][async-lift]f`, import `[export]$root` `[task-return]f` (flat params: `result<f64, js-error>` = i32 i64 i32×6, `result<string|_, js-error>` = i32×8). Plan: 3a = microtasks only: `AbiSig::is_async`; the lift calls the usual adapter (init, call, then `abi::Helper::Await`: drain, mark the promise handled, its reason → err, still pending → err, other unhandled rejection → err, else convert the value), loads the return area into the flat `task.return` params, returns EXIT (the callback is never called yet). Result type: JSDoc `@returns {Promise<string|number|void>}`, else the kinds `return` values have (new `Slot::Settled`). 3b = timers in libraries (clock imports, the loop, WAIT), if 3a lands.
- 2026-09-27: 3a done. IR `AbiSig::is_async` (validate: an async export's function returns the promise); lowering: async exports allowed, result from JSDoc `@returns {Promise<T>}` (`Promise<…>` on a non-async function is an error) or `Slot::Settled` (the kinds of `return` values), `$export` wrappers convert params only; component: `async func`; backend: `abi::Helper::Await` (`await_export`), the adapter awaits + converts (ToNumber/ToString), `abi::async_lift` (return area → flat `task.return` params, EXIT), `abi::async_callback` (unreachable), `[task-return]<name>` imports in libraries. Fixture `library/async.js` (all results as node's `await m.f()`), test `async_exports_await_their_promise`; unsupported test updated. cargo test ok; diff.sh 97/0/15; hello.js 55715.
- 2026-09-27: 3b done: timers in libraries with an async export. Backend: host imports are looked up through `Cx::imports()` (`host::Imports`: a library's `task.return`s come first, then `LIBRARY_TIMER_IMPORTS`: waitable set, clock, wait-until, subtask drop/cancel, backpressure inc/dec); an async export is 3 functions + its adapter: `Mode::Start` (init + call, promise in `LIB_GLOBALS`, now 7), `abi::Helper::Step` (jobs + nextTicks, done when settled/failed/unhandled/nothing to wait for, else arm `wait-until`, backpressure up, `WAIT`), `Event`/`Tick` (run due timers; exception or unhandled rejection fails the call), `Mode::Finish` (the `result` via `Await`), `End` (disarm, backpressure down). Found: `promise::Helper::Unhandled` forgets what it looked at, so Step keeps the reason. World: `import wasi:clocks/monotonic-clock` when an export is async (dropped when unused). Lowering: timers allowed in libraries with an async export. Fixture `library/async-timers.js` (as node; a throwing timer fails the call where node crashes). cargo test ok; diff.sh 97/0/16; hello.js 55715.
- 2026-09-27: slice 3 done: Notes / Result, README (Next up: slice 4; F120 retargeted; F121–F123), design.md D3/D4, keywords.md, README.md; handoff baseline 97/0/16.
- 2026-09-27: slice 4 started. Split: 4a = `--wit`/`--world`, exports checked against the world (names, sync vs async, param count), scalar types (bool, s8–u64, f32/f64, char, string), the world decides `result` (without it a throw traps, like jco); 4b = result/option/list/record/variant/enum/flags/tuple.
- 2026-09-27: 4a implemented: `ir::AbiType` = all WIT scalars (`param_type`/`result_type`), `lower_with(program, Some(&Boundary))` (world sigs by JS export name; pass 1 checks sync/async and param count, observes param kinds; `wrap_exports` converts via `convert_any`/`to_boundary`), backend adapter rewritten (per-type lifting/narrowing, `result` return areas and joined `task.return` params for any scalar, `FromCodePoint`/`CodePointAt0`, conversion helpers gated), component `load_world` (vendored WASI + `push_path`, aliases resolved, non-scalars and interface exports rejected), CLI `--wit` + `match_world` (missing / extra exports). Fixture `library/scalars/{world.wit,scalars.js}`: every result as ToInt32 + narrowing; a throw without `result` traps.
- 2026-09-27: found while testing: a numbers-only library with a `u32` world had no JS runtime but its adapter requested the string/allocator helpers and the bump global (invalid module): helpers and memory are now requested only when needed (`abi::needs_memory`). Tests added.
- 2026-09-27: 4a done: Notes / Result, README (Next up: 4b; F124–F126; F13/F34/F60/F119 notes), design.md D4, wit-names.md, README.md.
- 2026-09-27: 4b agreed with the user: a throw becomes `err` by jco's convention (a thrown value's own `payload` property, else a thrown Error traps, else the value itself, lowered as `E`; a mismatch traps). Split: 4b-1 = `result`, `option`, `tuple`, `record`, `enum`, `flags` (≤ 16 flat params, results in a return area, JS shapes as jco: option ↔ value/undefined, tuple ↔ array, record/flags ↔ object with camelCase keys, enum ↔ its WIT name, a nested `result` ↔ `{tag, val}`); 4b-2 = `list<T>` (`list<u8>` ↔ Uint8Array), `variant`, params in memory (F21).
- 2026-09-27: 4b-1 plan: IR `AbiType::Compound(id)` into `Module::wit_types` (a `WitType` tree), `AbiSig::err` (js-error / none / a type); backend `wit.rs`: `flatten`/`size_align` (joins), `lift_flat` (params → JS values), `lower_mem` (JS value → memory), `flat_load` (memory → flat values, for `task.return`), with locals allocated while generating (body built in a buffer, then `Function::raw`).
- 2026-09-27: 4b-1 done: `ir::WitType`/`ErrType`, `backend/src/wit.rs`, adapter rebuilt around the return area (+ `Finish(task_return)`), component `Ty`, CLI camelCase keys. Size: first +1.9 KB on library/async.js (the flat load was emitted in both the ok and err paths of every `finish`); one delivery point and merged local declarations brought it to +174. Fixture `library/compound`, test `user_world_compound_types`. cargo test ok; diff.sh 97/0/16; hello.js 55715.
- 2026-09-27: 4b-2 started: list<T> (list<u8> ↔ Uint8Array), variant, params in memory past 16 flat values (F21).
- 2026-09-27: 4b-2 implemented: `ir::WitType::{List, Variant}` (`typed_kind`: numeric lists up to 32 bits and `f64` are typed arrays, as in jco; 64-bit lists plain arrays), component `Ty::{List, Variant}`; backend `wit.rs`: flatten/layout, `lift_mem` (flat_load + lift_flat), lists lifted from memory (typed arrays by byte copy, others element by element) and lowered with `cabi_realloc` (a typed array of the same kind: byte copy; any array-like: `length` + indices), variants and nested results share `lift_cases`/`lower_cases` (a variant case without payload has no `val`, like jco); params past 16 flat values come as a pointer to their tuple (F21; the backend's bail removed). Lowering sets `$typed` when a world has typed lists. Fixture `library/lists/{world.wit,lists.js}`: every call as expected on the first build.
- 2026-09-27: 4b-2 done: user_world_lists_and_variants passes; cargo test ok, clippy (old warning only), diff.sh 97/0/16, sizes unchanged (hello.js 55715, add.js 289, library/async.js 76153). Notes / Result, README (Next up: slice 5; F21 ticked; F127/F128 retargeted; F129), design.md D4, README.md.
