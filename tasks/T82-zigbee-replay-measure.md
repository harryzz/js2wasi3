# T82: measure T80 and T81 on the T79 replay (local: a-03)

Status: todo (handoff written 2026-10-05) | Depends on: T80, T81 | **Not for cloud sessions**: needs a-03, the
recording and the zigbee2mqtt build tree outside the repo (see `tasks/CLOUD-BATCH.md`).

## Goal
One replay run per finished change (T80, then T80+T81) of T79's recorded production session, against the existing
baseline. **Don't re-run the baseline or node** (user): they're measured.

## Baseline (T79, a-03, the 903 s after zigbee2mqtt's start, 548 device frames; already measured)
| | CPU | vs node | RSS | run dir on a-03 |
|---|---|---|---|---|
| node 24.21.0 | 8.66 s | 1× | 289 MB | `~/js2wasi3-prod-run/t79/out-node-2` |
| js2wasi3, 512 MiB GC heap (the default since F347) | 39.1 s | 4.5× | 765 MB | `t79/out-gc-c512m` |
| js2wasi3, 1 GiB | 35.9 s | 4.1× | 1.3 GB | `t79/out-gc-c1g` |

Profile at 1 GiB (`t79/out-perf-c1g`): UTF-8 encoding ~36% (T80), RegExp ~9% (T81), wasmtime libcalls ~7% (F350),
property-access helpers ~27% self, `box_number` 3%, GC ~3%.

## How (T79's tools)
1. Merge or check out the branch with T80/T81 (`tasks/CLOUD-BATCH.md`: branch `speed-batch`), sync the repo to a-03
   (memory "a-03": rsync, never push to get code there), then on a-03 `~/js2wasi3-prod-run/build-p.sh` (release
   js2wasi3, the zigbee2mqtt build with all device modules, `wasmtime compile` with 512 MiB: ~5 min, 15 GB).
2. One run: `nohup ~/js2wasi3/tasks/T79-files/run-replay.sh ~/js2wasi3-prod-run/rec-20261004-1941 wasm t79/out-<name>`
   (15 min) → `result.txt` (`cpu_steady_s`), RSS in `cpu.csv`, first minute from `cpu.csv`; publishes in `z2m.log`
   (13,476 expected for js2wasi3; node 13,567: F317).
3. Profile only if the result needs explaining: `PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf` on that same run.

## Then decide (with the new numbers)
Next large item, or none: F350 (wasmtime libcalls), F323/F334 (unboxed numbers), F321 (shapes), F352 (one-byte
strings). WasmGC limits that make them larger: struct layouts fixed at compile time (shapes as arrays of slots), no
interior pointers (shared substrings need our own slice type), i31 is 31 bits.

## Acceptance
- [ ] T80, T80+T81 measured (one run each), table in `docs/zigbee2mqtt-feasibility.md`.
- [ ] The next item decided and recorded as a task or follow-ups.

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written (the measurement part of the earlier T80 plan; F347's 512 MiB scripts are untested
  until this task's first build).
