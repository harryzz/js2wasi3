# T146: wasmtime 49.0.3 → 50.0.0 (F436)

Status: in progress | Depends on: T123 | Local + a-03 (user, 2026-10-21: "setup and pin here. when tests finish setup on a-03 also and rerun tests")

## Goal
wasmtime 50.0.0 was released on 2026-10-20. T123 measured 50.0.0-rc.1 on the same output (pidigits −26.4%, splay −9.1%,
binarytrees −7.2%, test262 no changes). Pin the final release on the laptop and a-03, rerun the checks, and take a
fresh baseline (every speed gate compares builds on one host, so the numbers move with it).

## Plan
- [ ] Laptop: install 50.0.0 (`~/.wasmtime/bin`, 49.0.3 kept as `wasmtime-49.0.3`), `scripts/check-tools.sh`, `probes/run.sh`,
  `TOOLS.md`, smoke fixtures.
- [ ] a-03: after T145's test262 finishes, install 50.0.0 (`~/.local/bin`, 49.0.3 kept), rerun full test262.
- [ ] A fresh baseline of the 18 programs on 50.0.0 (ask: `scripts/v8bench.sh` or the gate) and record it.

## Notes / Result

## Progress
- 2026-10-21: started (user: "here i see wasmtime 50 is released. can you setup and pin here. when tests finish setup on
  a-03 also and rerun tests"). a-03 is busy with T145's test262: untouched until it finishes.
- 2026-10-21: laptop: wasmtime 50.0.0 (`ebe5e8df1 2026-10-20`, sha256 checked against GitHub's digest) in `~/.wasmtime/bin` (49.0.3 kept as `wasmtime-49.0.3`); `scripts/check-tools.sh` pins 50.0.0: all ok; `probes/run.sh` exit 0 (`out/probes-t146.log`, output = 49.0.3's apart from timings); all fixtures (`scripts/diff.sh -j 4`): **281 passed, 0 failed**, 19 skipped (also fetch/ws/http under load). `TOOLS.md` and CLAUDE.md pin 50.0.0. Release notes read: CM-async spec updates (cancellable removed, task.cancel traps, dropped handles trap), GC alias regions, 49.0.4's security fixes included.
- 2026-10-21: `cargo test --release -p js2wasi3 --test build` (34 tests, the wasi fixtures among them): 33 passed + 1 timing flake (`concurrent_timers_overlap`, while the laptop was busy), then 34/34 on two reruns on 50.0.0 and 34/34 twice on 49.0.3; the failing test passes alone (3.4 s). Waiting for T145's test262 on a-03 to finish before setting up a-03.
- 2026-10-21: a-03: wasmtime 50.0.0 installed (`~/.local/bin`, 49.0.3 kept as `wasmtime-49.0.3`, sha256 = the laptop's); T145's test262 (49.0.3) had finished: no changes. Full test262 on 50.0.0 started (`~/t262-t146.log`).
