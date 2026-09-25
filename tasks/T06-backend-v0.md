# T06: Backend v0: IR to core Wasm

Status: todo | Depends on: T04 (T05 for real inputs)

## Goal
Emit a valid core module from IR with `wasm-encoder` 0.259: functions, f64/i32 locals, structured control, direct calls, exports.

## Read first
`crates/backend/src/lib.rs`, `probes/p.wat` (target feature shapes), wasm-encoder docs.

## In scope
- Map the IR's labelled break/continue onto Wasm `block`/`loop`/`br` depths.
- Validate the output with `wasmparser` inside the unit tests (all features on).
- `js2wasi3 build --core` writes the core module only.

## Out of scope
The component (T07), GC types (T08), exceptions (T12).

## Acceptance
- `fib.js` → core module → `wasmtime run --invoke 'fib' out.wasm 20` prints `6765`. For core modules, pass the args after the file.
- `wasm-tools validate` passes.

## Notes / Result
