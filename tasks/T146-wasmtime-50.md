# T146: wasmtime 49.0.3 → 50.0.0 (F436)

Status: done (2026-10-21): pinned on the laptop and a-03; fresh baseline not taken | Depends on: T123 | Local + a-03 (user, 2026-10-21: "setup and pin here. when tests finish setup on a-03 also and rerun tests")

## Goal
wasmtime 50.0.0 was released on 2026-10-20. T123 measured 50.0.0-rc.1 on the same output (pidigits −26.4%, splay −9.1%,
binarytrees −7.2%, test262 no changes). Pin the final release on the laptop and a-03, rerun the checks, and take a
fresh baseline (every speed gate compares builds on one host, so the numbers move with it).

## Plan
- [x] Laptop: install 50.0.0 (`~/.wasmtime/bin`, 49.0.3 kept as `wasmtime-49.0.3`), `scripts/check-tools.sh`, `probes/run.sh`,
  `TOOLS.md`, smoke fixtures.
- [x] a-03: after T145's test262 finishes, install 50.0.0 (`~/.local/bin`, 49.0.3 kept), rerun full test262.
- [ ] A fresh baseline of the 18 programs on 50.0.0 (ask: `scripts/v8bench.sh` or the gate) and record it.

## Notes / Result
- **What changed:** wasmtime 50.0.0 (`ebe5e8df1 2026-10-20`) on the laptop (`~/.wasmtime/bin`) and a-03 (`~/.local/bin`), 49.0.3 kept
  as `wasmtime-49.0.3` on both; `scripts/check-tools.sh`, `TOOLS.md` and `CLAUDE.md` pin 50.0.0. 49.0.4's security fixes are
  inside 50.0.0 (wasi-http content-length and trailers, aarch64 `br_table`, stream/future type checks).
- **Result:** `probes/run.sh` exit 0, output = 49.0.3's apart from timings (`out/probes-t146.log`); all fixtures 281 passed,
  0 failed, 19 skipped (`scripts/diff.sh -j 4`); `cargo test --release -p js2wasi3 --test build` 34/34 (one timing flake in
  the first run); full test262 on a-03 with 50.0.0 (`out/t262-t146.log`, 1,526 s): **no changes**, 28,123 of 28,984 run pass
  (97.0%). The component-model async spec changes (no `cancellable`, `task.cancel` and dropped-handle traps) broke nothing.
- **Fresh baseline** (a-03, `scripts/v8bench.sh`, `out/v8bench-t146-a03.md`, `out/v8bench-t146/v8bench.tsv`; all 18 outputs = node):
  our components on **wasmtime 50.0.0 run 6.98× node** (geomean of the 18; 7.46× in `out/v8bench-t137-a03.md`) and on node's V8 4.88×
  (5.04×). Wasmtime times, new/old: geomean **0.9124**; pidigits −26.6%, fannkuchredux −23.5%, richards −20.4%, splay −12.7%,
  earley-boyer −10.8%, binarytrees −9.5%, spectralnorm −13.8%, crypto −6.3%, regexp −5.9%, raytrace −4.5%, navier-stokes −4.1%,
  fasta/knucleotide −4.0%, mandelbrot −2.9%, nbody −2.6%, deltablue −2.3%; regexredux +0.2%, revcomp +2.5%.
  **This mixes two changes** (the compiler tasks T141–T145 and wasmtime 49.0.3 → 50.0.0): the previous table is T137's build on 49.0.3.
  The release candidate alone gave pidigits −26.4%, splay −9.1%, binarytrees −7.2% (T123), so pidigits and binarytrees are
  essentially wasmtime's; fannkuchredux, richards, spectralnorm and earley-boyer are mostly the compiler tasks.

## Progress
- 2026-10-21: started (user: "here i see wasmtime 50 is released. can you setup and pin here. when tests finish setup on
  a-03 also and rerun tests"). a-03 is busy with T145's test262: untouched until it finishes.
- 2026-10-21: laptop: wasmtime 50.0.0 (`ebe5e8df1 2026-10-20`, sha256 checked against GitHub's digest) in `~/.wasmtime/bin` (49.0.3 kept as `wasmtime-49.0.3`); `scripts/check-tools.sh` pins 50.0.0: all ok; `probes/run.sh` exit 0 (`out/probes-t146.log`, output = 49.0.3's apart from timings); all fixtures (`scripts/diff.sh -j 4`): **281 passed, 0 failed**, 19 skipped (also fetch/ws/http under load). `TOOLS.md` and CLAUDE.md pin 50.0.0. Release notes read: CM-async spec updates (cancellable removed, task.cancel traps, dropped handles trap), GC alias regions, 49.0.4's security fixes included.
- 2026-10-21: `cargo test --release -p js2wasi3 --test build` (34 tests, the wasi fixtures among them): 33 passed + 1 timing flake (`concurrent_timers_overlap`, while the laptop was busy), then 34/34 on two reruns on 50.0.0 and 34/34 twice on 49.0.3; the failing test passes alone (3.4 s). Waiting for T145's test262 on a-03 to finish before setting up a-03.
- 2026-10-21: a-03: wasmtime 50.0.0 installed (`~/.local/bin`, 49.0.3 kept as `wasmtime-49.0.3`, sha256 = the laptop's); T145's test262 (49.0.3) had finished: no changes. Full test262 on 50.0.0 started (`~/t262-t146.log`).
- 2026-10-21: full test262 on a-03 with wasmtime 50.0.0 (`out/t262-t146.log`, 1,526 s): no changes (28,123). Task done.
- 2026-10-21: user: yes. Pushed 0da6cd8. `scripts/v8bench.sh -o ~/v8bench-t146` started on a-03 with wasmtime 50.0.0 (`~/v8bench-t146.md`).
- 2026-10-21: fresh baseline on 50.0.0 (above). Pushed 0da6cd8.
