# T07: Component v0: a library world from ES exports

Status: done | Depends on: T06

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
- 2026-09-25: baseline ok. `check-tools.sh` all ok; `cargo test` all ok (IR 5, frontend 6, lower 2+8, backend 5); `fib(20)` core smoke = 6765.
- 2026-09-25: re-checked WIT identifier rules on wasm-tools 1.259: later words may start with a digit (`x-2y` ok), `%` works
  before any identifier, including package/world names (`package %js2wasi3:%sum-odd`). The keyword list is in `wit-parser-0.259.0/src/ast/lex.rs`.
- 2026-09-25: **decisions, agreed with the user, written up in `docs/wit-names.md`** (and linked from design D4):
  1. Name mapping: `crates/component` computes JS→WIT names; the backend takes the map and emits core exports under the WIT names.
  2. Conversion: split at `_`/`$`/case boundaries, lowercase, join with `-`, always write `%` in the generated WIT text. Errors (a diagnostic, never a panic)
     for an empty result, a first word without a leading letter, non-ASCII names, and case-insensitive collisions. Escape hatch: `export { f as "wit-name" }`
     (the string is used as is and must be a valid WIT name). Params never cause an error: invalid or duplicate ones become `p<index>`.
  3. World: `js2wasi3:<stem>/<stem>` from the file name (`.` treated like `_`, falls back to `app`); `--world ns:pkg/world` overrides it.
- 2026-09-25: note for implementation: lowering already resolves `export { a as b }` from `p.exports` (lower pass 3), and `ir::Export.name`
  holds the JS name. The string-literal export form needs a flag from the frontend saying the name was a string.
- 2026-09-25: plan. The frontend `Export` gets `string_name` (the name was a string literal). `ir::Export` gets `pos` + `string_name`.
  `crates/component` gets its own plain input (`ExportSig`: name, string flag, param names, f64 only, line/col), so it doesn't depend on
  the frontend or the IR. The CLI joins them: plan the names, rename `ir.exports[*].name` to the WIT names (so the backend needs no change),
  emit, then `package`.
- 2026-09-25: frontend `Export.string_name` (checks for a quote at the name span; `js2wasi3 parse` prints such names quoted) and
  `ir::Export { string_name, pos }` are in. `crates/component`: `names::{to_wit, is_wit_name}`, `WorldName::{from_path, parse}`, `world()`, `package()`.
  `cargo test -p js2wasi3-component`: 6 ok. Next: CLI wiring + acceptance.
- 2026-09-25: CLI wired (`--world`, `plan_world`). Acceptance: `js2wasi3 build tests/fixtures/add.js -o add.wasm` → `wasm-tools validate` ok →
  `wasmtime run --invoke 'add(1, 2)' add.wasm` prints `3`. `names.js` with `--world acme:math/calc` → `sum-odd(10, 0)` = 25, `times-ten(4)` = 40, `default(5)` = -5.
- 2026-09-25: found that `wasm-tools component wit` always shows `root:component`: the world name isn't stored in component binaries
  (`wit-parser` decoding.rs:299). Documented in `docs/wit-names.md`; writing the WIT next to the output → F12.
- 2026-09-25: fixed the help text (it suggested `export { _1 as ... }`; it now uses the local name: `export { f as "my-name" }`).
  Added CLI tests `crates/js2wasi3/tests/build.rs` (4). Full `cargo test` ok, `cargo clippy --all-targets` clean.
  wit-component validates with `WasmFeatures::all()`, so GC/EH code in T08+ won't be rejected by the encoder.

## Notes / Result

**Done.** `js2wasi3 build x.js -o x.wasm` produces a component whose world is generated from the ES exports (sync lift, `f64` only).

What changed:
- `crates/component`: `names.rs` (JS → WIT names), `lib.rs` (`WorldName`, `world()`, `package()` via wit-parser `push_source` +
  `embed_component_metadata` + `ComponentEncoder`). Unit tests: 6.
- `crates/frontend`: `Export.string_name`; `js2wasi3 parse` prints string export names quoted.
- `crates/ir`: `Export { string_name, pos }`. The IR printer didn't change.
- `crates/lower`: fills them in.
- `crates/js2wasi3`: `build --world <ns:pkg/world>`; `plan_world` maps naming errors to source diagnostics and renames IR exports to WIT names.
- Fixtures: `tests/fixtures/names.js`, `tests/fixtures/unsupported/bad-names.js`. CLI tests: `crates/js2wasi3/tests/build.rs` (4).
- Docs: `docs/wit-names.md` (all naming rules and decisions), linked from design D4.

Commands run: `cargo test -q` (all ok), `cargo clippy -q --all-targets` (clean), `wasm-tools validate add.wasm`,
`wasmtime run --invoke 'add(1, 2)' add.wasm` → `3`.

Unsupported / limits:
- Only `f64` params/results. `plan_world` rejects other IR types with an error (none can reach it today).
- The world name isn't visible in the output binary (`root:component`); see F12.
- `--world` takes no package version (`@1.0.0`).

Next smallest step: T08 (JS values on WasmGC). At the boundary, `JsVal` params/results will need a WIT type mapping (F13).
