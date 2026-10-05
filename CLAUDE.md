# js2wasi3

Compiler: JavaScript → a WASI 0.3 Wasm component (WasmGC, native EH, P3 async). Written in Rust.

## Start of every session
1. Read `docs/design.md` (decisions D1–D5) and `tasks/README.md` (task index and status).
2. Work on **one** task file, `tasks/Txx-*.md`. Read its "Read first" list and nothing unrelated.
3. Run `scripts/check-tools.sh`. If tools changed, rerun `probes/run.sh` and update `TOOLS.md`.

## Cloud sessions (Claude Code on the web)
If this session runs in a cloud sandbox (no `~/js2wasi3-prod-run`, no a-03), follow **`tasks/CLOUD-BATCH.md`**: it
says which tasks to run unattended (T80, then T81), on which branch (`speed-batch`, never `main`), and what the
sandbox can't do. (That batch finished 2026-10-05.)

**Local sessions**: the cloud batch is merged (T82, 2026-10-05). The next task is **T83** (`tasks/T83-shapes.md`, F321:
shapes and shape-keyed inline caches).

## Rules
- Stay inside the task's scope. Record anything left for later (in the task file and in the Follow-ups table of `tasks/README.md`); don't implement it.
- **Write progress every iteration, not only at the end.** After each step (build/test run, design choice,
  blocker), append a dated line to the task file's `## Progress` log. Set the task's status to `in progress`
  in `tasks/README.md` when you start it. The goal: a new session can pick up the task from the files alone.
- Tests per iteration: the task's own tests plus `wasm-tools validate` plus one `wasmtime` smoke run. Never the whole suite.
- At the end of a task:
  1. tick the task file's checklist and write its "Notes / Result" (what changed, commands run, what's unsupported, the next smallest step);
  2. set the status in the table in `tasks/README.md`;
  3. add every deferred item to the **Follow-ups** table in `tasks/README.md` (next free `F<n>`, found in, pick up in),
     and tick any follow-up this task resolved.
- The only test host is wasmtime 49.0.2. The JS oracle is `node` 24.21.0 (latest LTS; pinned in `TOOLS.md`).
- Differential tests: `scripts/diff.sh [fixture.js | dir]` runs node and js2wasi3 + wasmtime and compares
  stdout, stderr and exit status (default: all of `tests/fixtures`; `x.skip` skips `x.js` with a reason).
  Run it on the fixtures you touch; a new command fixture needs no expected-output file (node is the oracle).
  See "Testing" in `docs/design.md`.

## Layout
- `crates/frontend`: oxc parse + semantic
- `crates/ir`: IR + lowering
- `crates/backend`: core Wasm
- `crates/component`: WIT + packaging
- `crates/js2wasi3`: CLI
- `tests/fixtures/*.js`: small programs. Libraries have node's results in `*.out` where relevant; commands are checked by `scripts/diff.sh`
- `probes/`: hand-written WAT proving toolchain features (`probes/run.sh`)
