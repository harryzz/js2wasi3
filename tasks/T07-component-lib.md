# T07: Component v0: a library world from ES exports

Status: todo | Depends on: T06

## Goal
Turn the core module into a component whose WIT world is generated from the entry's exports
(`export function add(a, b)` becomes `export add: func(a: f64, b: f64) -> f64`).

## Read first
The **Handoff** section below first. Then `docs/design.md` D4, `probes/run.sh` step 2 (embed/new flow), the `wit-component` 0.259 API (`embed_component_metadata`, `ComponentEncoder`).

## In scope
- Build a `wit_parser::Resolve` in memory, embed its metadata, encode with `ComponentEncoder`, validate.
- Sync lift for now. Every param and result is `f64`.
- `js2wasi3 build add.js -o add.wasm`, then `wasm-tools component wit add.wasm` shows the world.

## Out of scope
Strings and records (T09), async (T20), user-supplied `.wit` (T20).

## Acceptance
- `wasmtime run --invoke 'add(1, 2)' add.wasm` prints `3`.

## Handoff (written 2026-09-25 at the end of the T03–T06 session)

**State:** `main` at commit `0bab81b` (T06), clean tree. T01–T06 are done (see `tasks/README.md`).
The pipeline today: `frontend::parse` → `js2wasi3_lower::lower` → `js2wasi3_backend::emit` (core module) →
`js2wasi3_component::package` (**stub, this task**). `js2wasi3 build --core` skips the stub.

**Run first (baseline, about 1 min):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'       # all ok (IR 5, frontend 6, lower 2+8, backend 5)
cargo run -q -- build --core tests/fixtures/fib.js -o /tmp/fib.wasm && wasmtime run --invoke fib /tmp/fib.wasm 20   # 6765
```

**Verified by hand at handoff (CLI only, no code):**
- The current core output for `add.js` + the WIT `package js2wasi3:app; world app { export add: func(a: f64, b: f64) -> f64; }`
  → `wasm-tools component embed` + `component new` → `wasmtime run --invoke 'add(1, 2)' add.wasm` prints **`3`**.
  Scalar-only exports need **no memory and no `cabi_realloc`**, so no backend change is needed for them.
- **WIT names must be kebab-case.** `export sumOdd: func...` fails to parse ("invalid character in identifier 'O'"), and
  the core export must carry the WIT name: a core export `sumOdd` + WIT `sum-odd` fails with
  "failed to find export of function `sum-odd`". So this task must map JS names → kebab-case (`sumOdd` → `sum-odd`,
  params too) and emit the core exports under the kebab name (or rename them when packaging).
- `default` is a valid WIT name (`export default` → `default: func...`).

**API pointers (wit-component / wit-parser 0.259, checked in their sources):**
- `wit_parser::Resolve::default()`, `resolve.push_source(path: &str, contents: &str) -> Result<PackageId>`,
  `resolve.select_world(...)` (check its exact signature in `wit-parser-0.259.0/src/resolve/mod.rs:1416`).
- `wit_component::embed_component_metadata(&mut core_bytes, &resolve, world_id, StringEncoding::UTF8)`.
- `wit_component::ComponentEncoder::default().module(&bytes)?.validate(true).encode()?`.
- Add `wit-parser.workspace = true`, `wit-component.workspace = true` to `crates/component/Cargo.toml` (they're pinned in the root `Cargo.toml`).
- Simplest route: generate the WIT **text**, then `push_source` it. It's easy to snapshot and matches what `wasm-tools component wit` prints.

**Decisions to make (and log in Progress):**
1. Where the name mapping lives. Suggestion: `crates/component` computes the WIT names, and the backend takes an export-name map
   (or `component` renames exports in the core module before embedding). Keep JS semantics out of `component`.
2. Names that can't be mapped (e.g. `_x`, `$x`, `x2Y` where a segment would start with a digit, two JS names that map to the same WIT name) → a precise diagnostic, not a panic.
3. The package/world name for generated worlds (suggestion: `js2wasi3:app/app`, or derived from the file name).

**Pitfalls seen so far:**
- wasm-tools 1.259 text syntax differs from older docs (see `TOOLS.md`, "Syntax gotchas").
- Backend tests shell out to `wasmtime`. Keep new tests tiny (one invoke each), per `CLAUDE.md`.
- Remember the rules in `CLAUDE.md`: set `in progress` in `tasks/README.md`, **append to `## Progress` after every step**, and update the Follow-ups table at the end.

## Progress

## Notes / Result
