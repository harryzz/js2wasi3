# js2wasi3

Compiler: JavaScript → a WASI 0.3 Wasm component (WasmGC, native EH, P3 async). Written in Rust.

## Start of every session
1. Read `docs/design.md` (decisions D1–D5) and `tasks/README.md` (task index and status).
2. Work on **one** task file, `tasks/Txx-*.md`. Read its "Read first" list and nothing unrelated.
3. Run `scripts/check-tools.sh`. If tools changed, rerun `probes/run.sh` and update `TOOLS.md`.

## Cloud sessions (Claude Code on the web)
No cloud batch is open (`tasks/CLOUD-BATCH.md`: the T83 batch is finished and merged). User, 2026-10-05: cloud
sessions use the same plan limits, so speed tasks run locally now.

**Local sessions**: shapes (T83–T86) were reverted (branch `shapes`). T87–T98 are done (T91: `Map`/`Set` hashes; T92/T93: object tests without `is_subtype`; T94: `string_hash` memo;
2026-10-08/09). T95 (computed-key caches) was measured and dropped, no code kept. T96 (generic `get`/`set` without `is_subtype`) is done. T97 (typed-array `.length` fast path) and T98 (16 MiB GC heap reserved at start, `build --gc-reserve`) are done.
T99 (prototype-hit cache, F359) was measured and dropped on 2026-10-10 (replay 20.64 s vs 20.28 s), no code kept.
T100 (string constants as one GC root, F322) was measured and dropped on 2026-10-10 (benchmarks 3–7% slower), no code kept.
T101 (the Node.js compatibility page, `scripts/compat.mjs`, https://harryzz.github.io/js2wasi3/) is done: re-run
`node scripts/compat.mjs` after tasks that add node APIs. T102 (watchpoint flags, F390: iteration sees a replaced
`next`, `regexp_plain` one bit) is done. T103 (`var x;` written before every read stays `f64`, F389: mandelbrot 15.6× faster) is done. T104 (F276: a `var` read before its initializer is `undefined`) is done. T105 (F372: a per-site entry guess for
`set_named`: nbody −14%, richards −11%) is done. T106 (`let x;` stays `f64`: spectralnorm −32%) is done. T107 (F391: number arrays
unboxed: spectralnorm −49%, navier-stokes −19%) is done. T108 (F401: element reads in a `+`
under a numeric consumer unboxed: navier-stokes −47%) is done. T109 (F392, object layouts per
allocation site: splay −27%, binarytrees −26%, earley-boyer −24%, replay −1.2%) is done. Next task: not chosen (F405, F406).

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
