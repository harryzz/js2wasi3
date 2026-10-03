# Pinned tools

Checked 2026-09-25 against the latest GitHub releases (node: 2026-09-26, latest LTS; wasmtime: 2026-10-03). Rule: always the latest
version of every tool. Verify with `scripts/check-tools.sh`.

| Tool | Version | Used for |
|---|---|---|
| wasmtime | 49.0.2 | the only test host (WASI 0.3 via `-Sp3`; 49.0.2 since 2026-10-03: security fixes, among them GC rooting across `try_call`, F224) |
| wasm-tools | 1.259.0 | validate, print, `component embed/new/wit` |
| wasm crates | 0.259.0 | `wasm-encoder`, `wasmparser`, `wit-parser`, `wit-component` (same code as wasm-tools 1.259) |
| oxc crates | 0.151.0 | JS/TS parser and semantic analysis |
| wkg | 0.16.1 | fetch WASI WIT packages (`wkg wit fetch`) |
| WASI | 0.3.1 | `wasi:cli`, `wasi:clocks`, ... `@0.3.1` |
| wit-bindgen | 0.62.0 | reference only (naming/ABI questions) |
| wac | 0.11.0 | composing components in tests (later) |
| binaryen (wasm-opt) | 133 | optional optimisation experiments only |
| rustc/cargo | 1.98.1 | pinned in `rust-toolchain.toml` |
| node | 24.21.0 | differential oracle for JS behaviour (latest LTS; was 20.19.2 until 2026-09-26) |
| test262 | `7ab7fafa0003f73fc85c1b95d88094d33f7eb8bd` | conformance subset (`scripts/fetch-test262.sh`, `scripts/test262.mjs`; `main` on 2026-09-27) |

## Wasm feature status on wasmtime 49.0.2 (verified by `probes/run.sh`; same output as on 49.0.1)

| Feature | Default? | Notes |
|---|---|---|
| GC (structs, arrays, i31, casts) | on | |
| Exceptions (`try_table`, `exnref`) | on | works inside async callbacks |
| Component Model async (callback lift, streams, futures) | on | WASI 0.3 host needs `-Sp3` |
| Stackful async lift (🚟) | off | `-W component-model-async-stackful=y` |
| Component Model threads (🧵) | off | `-W component-model-threading=y`; ~0.75 µs per switch |
| Stack switching (`cont.new`/`resume`/`suspend`) | off | `-W stack-switching=y`; ~25× or more faster than 🧵 |
| GC Canonical ABI option (🛸) | off | scalars only; strings/lists **not implemented** |
| Timers (`monotonic-clock` `[async-lower]wait-until`, `[subtask-cancel]`) | on | probe 8: concurrent waits overlap; a subtask must leave its waitable set (`waitable.join h 0`) before `subtask.cancel`, else trap |

## Syntax gotchas (wasm-tools 1.259)

- `(canon lift ... async (callback (core func $i "cb")))`: the `core` keyword is required.
- `(canon thread.new-indirect $ft (core table $i "t"))`.
- Async core names from wit-component: `[async-lower]f`, `[async-lift]iface#f`,
  `[callback][async-lift]iface#f`, `[task-return]f`, `[stream-new-0]f`,
  `[async-lower][stream-write-0]f`, `[stream-drop-writable-0]f`, `$root` / `[waitable-set-new]` etc.
  Print them with `wasm-tools component embed wit --dummy-names legacy --async-callback -t`.

When a tool is upgraded, rerun `probes/run.sh` and update this file first.
