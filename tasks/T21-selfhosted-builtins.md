# T21: Self-hosted built-ins

Status: todo | Depends on: T16 (can start after T13)

## Goal
A pipeline that compiles built-ins written in JS/TS (with `__intrinsic_*` calls into the backend runtime) together with
user code, keeping only what's used. First batch: `Math`, `String.prototype` basics, `Array.prototype` basics, `JSON.stringify` for primitives, arrays and objects.

## Acceptance
- The built-ins compile with the same frontend/IR/backend (no separate compiler), and fixtures using them match node.
- A build of `hello.js` pulls in no unused built-in (check the size in the Result).

## Notes / Result
