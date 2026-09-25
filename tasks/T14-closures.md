# T14: Functions as values, closures and `this`

Status: todo | Depends on: T13

## Goal
Function objects (a GC struct holding a funcref and an environment), captured variables (boxing only the captured ones),
arrow functions, `this` in methods, `call/apply`, `arguments` (non-strict objects stay out), a call via `call_ref`, and TypeError when calling a non-function.

## Acceptance
- Fixtures `closures/*.js` (counters, loops capturing `let` per iteration, methods, arrows) match node.

## Notes / Result
