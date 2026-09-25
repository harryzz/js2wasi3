# js2wasi3

Compiler: JavaScript → a WASI 0.3 Wasm component (WasmGC, native EH, P3 async). Written in Rust.

## Start of every session
1. Read `docs/design.md` (decisions D1–D5) and `tasks/README.md` (task index and status).
2. Work on **one** task file, `tasks/Txx-*.md`. Read its "Read first" list and nothing unrelated.
3. Run `scripts/check-tools.sh`. If tools changed, rerun `probes/run.sh` and update `TOOLS.md`.

## Rules
- Stay inside the task's scope. Record anything left for later (in the task file and in the Follow-ups table of `tasks/README.md`); don't implement it.
- Tests per iteration: the task's own tests plus `wasm-tools validate` plus one `wasmtime` smoke run. Never the whole suite.
- At the end of a task:
  1. tick the task file's checklist and write its "Notes / Result" (what changed, commands run, what's unsupported, the next smallest step);
  2. set the status in the table in `tasks/README.md`;
  3. add every deferred item to the **Follow-ups** table in `tasks/README.md` (next free `F<n>`, found in, pick up in),
     and tick any follow-up this task resolved.
- The only test host is wasmtime 49.0.1. The JS oracle is `node`.

## Layout
- `crates/frontend`: oxc parse + semantic
- `crates/ir`: IR + lowering
- `crates/backend`: core Wasm
- `crates/component`: WIT + packaging
- `crates/js2wasi3`: CLI
- `tests/fixtures/*.js`: small programs, each with an expected output (`*.out`) where relevant
- `probes/`: hand-written WAT proving toolchain features (`probes/run.sh`)
