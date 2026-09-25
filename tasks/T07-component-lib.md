# T07: Component v0: a library world from ES exports

Status: todo | Depends on: T06

## Goal
Turn the core module into a component whose WIT world is generated from the entry's exports
(`export function add(a, b)` becomes `export add: func(a: f64, b: f64) -> f64`).

## Read first
`docs/design.md` D4, `probes/run.sh` step 2 (embed/new flow), the `wit-component` 0.259 API (`embed_component_metadata`, `ComponentEncoder`).

## In scope
- Build a `wit_parser::Resolve` in memory, embed its metadata, encode with `ComponentEncoder`, validate.
- Sync lift for now. Every param and result is `f64`.
- `js2wasi3 build add.js -o add.wasm`, then `wasm-tools component wit add.wasm` shows the world.

## Out of scope
Strings and records (T09), async (T20), user-supplied `.wit` (T20).

## Acceptance
- `wasmtime run --invoke 'add(1, 2)' add.wasm` prints `3`.

## Notes / Result
