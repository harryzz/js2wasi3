# T05: Lower the numeric subset from AST to IR

Status: todo | Depends on: T03, T04

## Goal
Lower real JS (via the frontend) into IR for number-only programs: `function` declarations, params, `let/const`,
arithmetic and comparisons, `if/while/for`, `return`, direct calls, `export function`.

## In scope
- For this task every value is `f64`. Anything else (strings, objects, closures, `this`, ...) gives an
  "unsupported: <construct> at file:line:col" diagnostic. Never miscompile silently.
- `js2wasi3 ir <file>` prints the IR.

## Out of scope
Dynamic values (T08), recursion into other modules, and optimisations.

## Acceptance
- `tests/fixtures/{add.js, fib.js, loops.js}` lower, and their IR snapshots match.
- A fixture that uses a string literal fails with a precise "unsupported" diagnostic.

## Notes / Result
