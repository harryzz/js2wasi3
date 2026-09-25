# T12: Exceptions with native Wasm EH

Status: todo | Depends on: T11

## Goal
Implement D2: `throw`, `try/catch` (with and without a binding), `finally` (including break/continue/return inside it),
nesting and rethrow, plus the internal TypeError/ReferenceError helpers used by the runtime.

## In scope
- Lower `finally` in the IR (duplicate or dispatch). Use `try_table` + `exnref`, with one tag carrying a `jsval`.
- Boundary policy: a library export returns `result<T, js-error>` (`record js-error { name: string, message: string, stack: option<string> }`).
  A command prints `Uncaught <name>: <message>` to stderr and returns `err`.

## Acceptance
- Fixtures `exceptions/*.js` (≥8 cases, including finally+return and a throw from a nested call) match node.
- A Wasm trap, such as an explicit `unreachable` in a test helper, is **not** caught by JS `catch`.

## Notes / Result
