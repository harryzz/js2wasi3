# T122: wasmtime 49.0.2 → 49.0.3

Status: done | Depends on: — | Local + a-03 (user, 2026-10-16: "small task after finish current")

## Goal
wasmtime 49.0.3 (2026-10-14) is a patch release of the pinned 49.0.2: security and panic fixes, among them
Component Model async (`call_async` cancelled, async-lifted calls via `call`, pending calls after a trap, streams
and futures viewed as the wrong type, zero-length stream writes), exception tags (unaligned host data), dropping long
chains of recursive types, a WASI filesystem symlink escape. No speed changes listed. Pin it on the laptop and a-03.

## Plan
- [x] Install 49.0.3 on the laptop (`~/.wasmtime/bin`, 49.0.2 kept as `wasmtime-49.0.2`) and on a-03 (`~/.local/bin`).
- [x] `scripts/check-tools.sh`, `probes/run.sh`, `TOOLS.md`; smoke fixtures; full test262 on a-03 (ask).

## Progress
- 2026-10-16: task created.
- 2026-10-16: 49.0.3 (`wasmtime-v49.0.3-x86_64-linux.tar.xz` from the GitHub release via `gh`) installed on the laptop
  (`~/.wasmtime/bin/wasmtime`; 49.0.2 kept as `wasmtime-49.0.2`) and a-03 (`~/.local/bin/wasmtime`; 49.0.2 kept as
  `wasmtime-49.0.2`); both report `49.0.3 (f96eabaa1 2026-10-14)`. `scripts/check-tools.sh` pins 49.0.3: all ok.
- 2026-10-16: `probes/run.sh` stopped at probe 11 (`/usr/bin/time` isn't installed on the laptop) and probe 8 failed
  ("multiple worlds in `probe:cmd`": `component embed` needs `--world cmd`, as probe 9's `--world cmdfs`); the same
  output on 49.0.2, so script problems, not the version. Fixed (bash's `time` with `TIMEFORMAT`; `--world cmd`): all
  probes run, exit 0 (`out/probes-t122.log`); output = 49.0.2's (timings aside). `TOOLS.md` updated.
- 2026-10-16: fixtures `async` 15, `promises` 14, `exceptions` 14 (2 skip), `generators` 4, `node` 42 pass
  (`fetch.js` and `fs-promises-turn.js` timed out under the parallel run, pass alone; `fs-promises-turn.js` alone:
  21.9 s on 49.0.2, 23.6 s on 49.0.3, mostly compile time); `wasi` 3 skip (as before).
- 2026-10-16: user: yes (and: `time` is a shell builtin, no path: the probe fix uses it). Full test262 on a-03 with 49.0.3 started (`~/t262-t122.log`).
- 2026-10-16: test262 (a-03, `out/t262-t122.log`, 1,530 s): no changes since the baseline (28,105 pass). Task done.

## Notes / Result
- **What changed:** wasmtime 49.0.3 on the laptop and a-03 (49.0.2 kept as `wasmtime-49.0.2` on both);
  `scripts/check-tools.sh` and `TOOLS.md` pin it. `probes/run.sh`: probe 8 embeds with `--world cmd`, probe 11 times
  with bash's `time` (no `/usr/bin/time`); all probes run, output = 49.0.2's.
- **Result:** probes unchanged; fixtures `async`, `promises`, `exceptions`, `generators`, `node` pass; full test262 no
  changes. No compiler change, so no gate.
- **Next smallest step:** measure wasmtime 50.0.0 when final (its GC field/array alias regions, #14351/#14353) on the 18
  programs and micro-benchmarks; then choose among F420, F427, F404.

