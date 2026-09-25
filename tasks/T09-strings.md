# T09: Strings and the string boundary

Status: todo | Depends on: T08

## Goal
JS strings as `(array i16)`: literals (from data or `array.new_fixed`), `.length`, concatenation with `+`,
`===`, number→string (shortest round-trip, like `String(1.5)`), and WIT `string` params/results.

## In scope
- The Canonical ABI string lift/lower through linear memory: `cabi_realloc`, UTF-8 ↔ UTF-16 conversion, and lone surrogates → U+FFFD (document this).
- The number→string algorithm must match JS exactly. Test it against node for a fixed list of 200 values.

## Out of scope
String methods (later, self-hosted in T21), ropes.

## Acceptance
- `export function greet(name) { return "hi " + name; }` → `wasmtime run --invoke 'greet("x")'` prints `"hi x"`.

## Notes / Result
