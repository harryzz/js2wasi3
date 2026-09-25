# Porffor Direct Wasm + WASI 0.3 Component Task

## Objective

Extend the real Porffor compiler with a direct WebAssembly backend and package its output as a WASI 0.3 component.

Porffor is an AOT JavaScript engine/runtime:

```text
JavaScript -> Porffor parser -> semantic analysis -> Porffor IR -> C renderer
                                                    |-> NEW core-Wasm renderer
                                                          -> component wrapper
                                                          -> WASI 0.3 host
```

The new renderer must consume the existing Porffor IR. Do not build another JavaScript compiler.

## Clone, branch, and push

Use the user's fork as `origin`, but branch from current official upstream:

```sh
git clone https://github.com/harryzz/porffor.git
cd porffor
git remote add upstream https://github.com/CanadaHonk/porffor.git
git fetch upstream
git switch -c js2wasip3/porffor-component upstream/main
git push -u origin js2wasip3/porffor-component
```

Verify before editing:

```sh
git remote -v
git status --short --branch
git log -1 --oneline upstream/main
```

- Pull from `upstream/main`.
- Push only to `origin/js2wasip3/porffor-component`.
- Never push to `CanadaHonk/porffor`.
- Do not base the branch on the fork's `main` when it is behind upstream.
- Record the exact upstream commit and pinned tools.

When this task was written, upstream `main` was
`de4eb588264885b3a1596f75010e371a2052033f`. Recheck it.

## Porffor code that must be reused

Read first:

- `compiler/index.js`: pipeline orchestration.
- `compiler/parse.js`, `compiler/parser/index.js`: parser.
- `compiler/semantic.js`: semantic analysis.
- `compiler/codegen.js`: AST-to-IR compilation and JS lowering.
- `compiler/ir.js`: the real structured Porffor IR.
- `compiler/render.js`: current C renderer and native runtime.
- `compiler/builtins.js`, `compiler/builtins/`: built-ins and precompiled IR.
- `compiler/prefs.js`: target preferences.

The IR already contains numeric/jsval operations, memory, structured control,
direct/dynamic calls, `Try`, `Throw`, `ThrowNew`, `Await`, `Yield`,
`Alloc`, and `GcBarrier`. Consume them.

Some IR operands contain C-specific operator/type strings, and `RawC` exists.
Make the smallest backend-neutral changes needed and keep the C renderer working.

## Hard rules

- No second parser, AST walker, semantic layer, IR, built-in library, runtime, or independent pipeline under any name.
- No reduced JavaScript subset presented as Porffor.
- No C, Rust, Zig, LLVM IR, or other source-language intermediate in the direct-Wasm backend.
- No reimplementation of semantics already handled by Porffor.
- Unsupported IR must report its kind, function, and source context.
- No automatic progression to another milestone.
- No full test suite on every iteration.

A temporary C-to-Wasm artifact is allowed only as a behavior oracle. It must not
become the direct backend.

## Keep three layers separate

1. **Core-Wasm renderer**: existing Porffor IR to a core module.
2. **Component adapter**: selected entry points to/from WIT through the Canonical ABI.
3. **WASI 0.3 integration**: P3 imports, async tasks, streams, futures, and host.

Do not spread WASI calls through JS code generation. Porffor runtime host
operations should use a small internal ABI implemented by the component adapter.

## WasmGC: decision and design

### Decision

Do **not** require WasmGC for the first working P3 component. Start with
Porffor's existing pointer/value model in one core Wasm linear memory.
WasmGC is an optional later backend mode.

### Why

- Porffor heap references are 32-bit arena offsets. Its IR has explicit
  `Load`, `Store`, `MemCopy`, `MemFill`, `Alloc`, and `GcBarrier`.
  This maps naturally to linear memory.
- WasmGC supplies low-level structs, arrays, and references. It does not supply
  JavaScript shapes, dictionaries, prototypes, strings, weak collections,
  finalization, or JS object semantics.
- Migrating object representation while creating a backend and a P3 boundary
  combines three independent high-risk changes.
- The current Component Model Canonical ABI is based on its established core
  value/linear-memory lowering. Direct Canonical ABI use of WasmGC values remains
  separate design work.
- Internal heap representation is private. A linear-memory engine can expose
  normal WIT values and resources.

### Initial linear-memory plan

First use the Porffor jsval and object layouts with a deliberately simple,
bounded allocator. For tiny tests collection may be disabled, but exhaustion
must fail explicitly; never claim GC works.

Then port Porffor's existing GC semantics. Do not copy C text from
`render.js`. Move reusable runtime behavior toward backend-neutral IR or
implement small Wasm runtime helpers preserving the same layouts and semantics.

The C runtime can conservatively inspect native stack state. A portable Wasm
module cannot inspect the engine value stack. Therefore linear-memory GC needs
explicit roots:

- allocate shadow-root slots for live reference/jsval locals;
- spill live roots before calls that may allocate, throw, or suspend;
- restore root depth on normal and exceptional exits;
- include globals, closures, suspended generators, promises, component resources,
  and pending exceptions;
- preserve `GcBarrier` write barriers;
- test collection in nested calls and `try/catch`.

Do not implement the full collector before the core module, component boundary,
and native exception path work on non-collecting smoke programs.

### Later optional `--wasm-gc` investigation

Only after the linear-memory component works, write and benchmark a WasmGC design
covering:

- jsval representation: numbers, immediates, refs, symbols;
- object shapes, prototypes, dictionary fallback, identity;
- arrays, typed arrays, strings, and Canonical ABI conversion;
- closures/environments, weak refs, finalizers;
- accepted GC features in the P3 component toolchain and host;
- size, allocation rate, throughput, and compatibility.

Keep it feature-gated. Do not replace the linear backend without correctness and
measured benefit. WasmGC and the Component Model solve different problems.

## WebAssembly exception handling

### Decision

Use native WebAssembly exception handling for internal JavaScript exceptions.
Implement it after basic numeric/control emission and before claiming useful JS
support.

Porffor already emits `Try`, `Throw`, and `ThrowNew` IR. Lower those nodes;
do not reconstruct exception semantics from the AST.

### Internal mapping

- Define one private Wasm exception tag carrying a Porffor jsval.
- If jsval is packed `i64`, use an `i64` payload. Otherwise document and test
  the exact selected signature.
- `Throw`: throw the private tag.
- `ThrowNew`: use existing Porffor error creation, then throw its jsval.
- `Try`: use the current EH encoding supported by pinned tools, preferably
  `try_table`/`exnref`, not a new setjmp/longjmp runtime.
- JS `catch` catches only the private JS tag. Wasm traps such as out-of-bounds
  memory remain traps and are not catchable JS values.
- Preserve nested catches, rethrow, catch binding, `finally`, and control flow
  crossing `finally`.

Validate exact EH opcodes with pinned `wasm-tools` and Wasmtime before renderer
work. Do not copy obsolete EH examples.

### Component boundary

Never expose the private tag, an `exnref`, or a Porffor pointer through WIT.
For each export choose one documented policy:

- library: catch and return WIT `result<T, js-error>`;
- command: report diagnostic and return failure;
- explicitly fatal API: convert to a component trap.

Use a small WIT error record containing name, message, and optional stack text.
An imported WIT `result` is a normal value; the adapter decides whether a
particular host error becomes a returned JS value or a thrown JS Error.

### EH with GC and async

- A pending exception payload is a GC root.
- Restore shadow-root depth on every exceptional edge.
- Async JS exceptions reject the Promise; they do not escape synchronously.
- P3 cancellation is not automatically a JS exception.
- Do not let an exception cross suspension. Store rejection/throw state in the
  promise or task object before suspension.

Do not implement a non-EH fallback unless a required target lacks native EH.

## WASI 0.3 plan

WASI 0.3 provides native component concurrency with async functions, streams,
and futures. It does not implement the JavaScript event loop or Promise queue.

Start with:

1. synchronous scalar/string WIT export;
2. synchronous command-style entry;
3. one `async func` or `future<T>`;
4. then streams, cancellation, and concurrent tasks.

Maintain Porffor's microtask queue and define when it drains relative to
component calls, suspension, and completion. Do not equate JS Promise with WIT
future without ownership, cancellation, and wakeup rules.

Pin:

- `wasm-tools` for validation, printing, WIT inspection, and component encoding;
- Wasmtime as the first WASI 0.3 host;
- official WASI 0.3 WIT packages for the chosen world.

Do not use `wasi-sdk` for the direct backend. It is only an optional C-backend
comparison or adapter experiment.

## Milestones

### M0 — executable design

- Clone and branch exactly as specified.
- Pin upstream and tool versions.
- Classify every real IR kind: direct, runtime helper, later, or C-specific.
- Inventory every `RawC` producer and plan its incremental Wasm replacement.
- Write a one-page ABI note: jsval, calls, memory, data, imports/exports,
  exception tag, roots, component boundary.
- Make a tiny probe for core Wasm + native EH + component packaging + P3 host.
- Stop and report.

Acceptance: probe validates and runs; design names real Porffor files/IR; no
parallel compiler exists.

### M1 — real Porffor IR to core Wasm

- Add backend selection after existing `codegen(program)`.
- Add a binary renderer consuming that actual result.
- Support constants, locals/globals, numeric operations/conversions, structured
  control, direct calls, memory/data, and return.
- Compile one real Porffor program needing no allocation, GC, dynamic calls, or EH.
- Validate and execute it; stop and report.

### M2 — minimal component

- Add Canonical ABI bindings for one small WIT world.
- Package M1 output as a WASI 0.3 component.
- Run it on pinned Wasmtime.
- Keep wrapping separate from core emission; stop and report.

### M3 — native JS exceptions

- Implement `Try`, `Throw`, `ThrowNew` using native Wasm EH.
- Test nested catch/rethrow/finally and boundary conversion.
- Keep collection disabled for tiny tests but root pending exceptions.
- Stop and report.

### M4 — layouts and allocation

- Implement existing jsval, object/string/array layouts, dynamic calls, helpers.
- Use a bounded non-collecting allocator first.
- Stop before collection and report.

### M5 — GC and roots

- Add shadow roots and safepoints.
- Port existing Porffor collection semantics incrementally.
- Test EH and suspension during allocation/collection.
- Stop and report.

### M6 — P3 async

- Connect promises/microtasks to one P3 async import/export.
- Define wakeups, cancellation, ownership, and exception-to-rejection.
- Add streams/futures only after one path works.
- Stop and report.

### M7 — optional WasmGC prototype

- Begin only after approval.
- Keep feature-gated and compare with linear memory.
- Do not block or destabilize the working P3 path.

## Testing and credit control

Each small iteration runs only:

- the directly relevant renderer/encoder unit test;
- one tiny source fixture;
- one to three focused behavior/differential tests;
- `wasm-tools validate`;
- the selected Wasmtime smoke command.

Never run full Test262, all Porffor tests, all hosts, or a full feature matrix on
each edit.

A broad suite is allowed only at an approved milestone end, before merge/release,
after a shared layout/runtime/GC change, or when explicitly requested.

Ask before any command expected to take over ten minutes or generate a large paid
workload. Stop rerunning an unchanged failure and diagnose it.

## Required Codex reports

Before code:

- checked-out commit and branch;
- exact files for this milestone;
- exact IR kinds in scope;
- tool versions and feature probes;
- targeted tests.

After an iteration:

- files and behavior changed;
- commands/tests and results;
- unsupported items found;
- next smallest step.

Do not start the next milestone without approval.

## Primary references

- Porffor: <https://github.com/CanadaHonk/porffor>
- Wasm feature status: <https://webassembly.org/features/>
- Wasm GC: <https://github.com/WebAssembly/gc>
- Wasm EH: <https://github.com/WebAssembly/exception-handling>
- Component Model: <https://github.com/WebAssembly/component-model>
- P3 concurrency: <https://github.com/WebAssembly/component-model/blob/main/design/mvp/Concurrency.md>
- Canonical ABI: <https://github.com/WebAssembly/component-model/blob/main/design/mvp/CanonicalABI.md>
- WASI releases: <https://github.com/WebAssembly/WASI/releases>
- wasm-tools: <https://github.com/bytecodealliance/wasm-tools>
- Wasmtime: <https://github.com/bytecodealliance/wasmtime>

Use current primary specs and pinned tool behavior, not old blog examples.
