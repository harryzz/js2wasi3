# js2wasi3

Compiler: JavaScript → a WASI 0.3 Wasm component (WasmGC, native EH, P3 async). Written in Rust.

## Start of every session
1. Read `docs/design.md` (decisions D1–D5) and `tasks/README.md` (task index and status).
2. Work on **one** task file, `tasks/Txx-*.md`. Read its "Read first" list and nothing unrelated.
3. Run `scripts/check-tools.sh`. If tools changed, rerun `probes/run.sh` and update `TOOLS.md`.

## Rules
- Stay inside the task's scope. List follow-ups in the task file under "Notes" and don't implement them.
- Tests per iteration: the task's own tests plus `wasm-tools validate` plus one `wasmtime` smoke run. Never the whole suite.
- At the end of a task: tick its checklist, set its status in `tasks/README.md`, and write a short
  "Result" section (what changed, commands run, what's unsupported, the next smallest step).
- The only test host is wasmtime 49.0.1. The JS oracle is `node`.

## Layout
- `crates/frontend`: oxc parse + semantic
- `crates/ir`: IR + lowering
- `crates/backend`: core Wasm
- `crates/component`: WIT + packaging
- `crates/js2wasi3`: CLI
- `tests/fixtures/*.js`: small programs, each with an expected output (`*.out`) where relevant
- `probes/`: hand-written WAT proving toolchain features (`probes/run.sh`)
