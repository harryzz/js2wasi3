# T08: JS values on WasmGC

Status: todo | Depends on: T07

## Goal
Implement D1's `jsval` (`(ref null eq)`) and the dynamic semantics needed by untyped code:
numbers (i31 fast path + boxed f64), `undefined`, `null`, booleans, `typeof`, `===`, `==` (primitives only),
JS `+ - * / %` and comparisons on numbers, ToBoolean, ToNumber (primitives).

## In scope
- The GC type section layout (a single `rec` group) and a list of runtime helpers emitted by the backend.
- Type inference stays trivial: `f64` where obviously numeric, otherwise `jsval`. Record the remaining work in Notes.
- The boundary converts `f64` ↔ `jsval`.

## Out of scope
Strings (T09), objects (T13).

## Acceptance
- Fixtures `values.js` (typeof table, equality table, truthiness) return the expected numbers through an f64 export.
- The core module validates and uses `i31ref` for small integers (visible in `wasm-tools print`).

## Notes / Result
