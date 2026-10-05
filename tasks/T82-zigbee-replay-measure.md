# T82: measure T80 and T81 on the T79 replay (local: a-03)

Status: todo (handoff written 2026-10-05, updated after the cloud batch) | Depends on: T80, T81 (done on `speed-batch`) | **Not for cloud sessions**: needs a-03, the
recording and the zigbee2mqtt build tree outside the repo (see `tasks/CLOUD-BATCH.md`).

## Goal
1. Bring the cloud batch's work (T80, T81 on branch `speed-batch`) into `main` with the user as author.
2. **One** replay run of T80+T81 together (user, 2026-10-05: one run, not one per change) of T79's recorded
   production session, against the existing baseline. **Don't re-run the baseline or node** (user): they're measured.

## Step 1: speed-batch → main, author fixed (local, no runs)
The cloud batch (2026-10-05) left 3 commits on `origin/speed-batch` (2fa6e6d T80, 6a06faf T81, 2fbda8d docs). Their
messages are clean, but their **author is `Claude <noreply@anthropic.com>`** (the sandbox's git identity): that
breaks the user's no-attribution rule, so re-author them before they reach `main`:
```
git fetch origin
git checkout -b speed-merge origin/speed-batch
git rebase main --exec 'git commit --amend --no-edit --reset-author'   # author = Zahari Zahariev <harryzz@gmail.com>
git log --format='%h %an <%ae> | %s' main..speed-merge                  # check: no Claude/anthropic anywhere
git log --format=%B main..speed-merge | grep -iE 'co-authored|claude|anthropic|generated'   # must print nothing
git checkout main && git merge --ff-only speed-merge
```
The rebase replays them onto today's `main` (the handoff commits); no conflicts expected (the batch didn't touch
`CLAUDE.md`, `tasks/CLOUD-BATCH.md` or this file). Then **ask the user** before `git push origin main` (global rule),
and ask whether to delete `speed-batch` on origin (it keeps the old author); delete the local `speed-merge` after.
Optional, cheap, no long runs: `cargo build --release` and `scripts/diff.sh` on T80's/T81's fixtures
(`tests/fixtures/node/buffer-utf8.js`, `tests/fixtures/builtins/regex-observable.js`) to confirm the merge locally.

Results from the cloud (sandbox micro-benchmarks, not claims): T80 UTF-8 ops 20–50× faster (e.g. `Buffer.from` of
an ASCII MQTT-sized string 5,716 → 130 ms per 20,000; node 14 ms); T81 `split` 3.2×, global `match` 1.5×, `$&`
replace 1.4×. Details in `tasks/T80-utf8-native.md` and `tasks/T81-regexp-fast-paths.md` (after the merge).

## Baseline (T79, a-03, the 903 s after zigbee2mqtt's start, 548 device frames; already measured)
| | CPU | vs node | RSS | run dir on a-03 |
|---|---|---|---|---|
| node 24.21.0 | 8.66 s | 1× | 289 MB | `~/js2wasi3-prod-run/t79/out-node-2` |
| js2wasi3, 512 MiB GC heap (the default since F347) | 39.1 s | 4.5× | 765 MB | `t79/out-gc-c512m` |
| js2wasi3, 1 GiB | 35.9 s | 4.1× | 1.3 GB | `t79/out-gc-c1g` |

Profile at 1 GiB (`t79/out-perf-c1g`): UTF-8 encoding ~36% (T80), RegExp ~9% (T81), wasmtime libcalls ~7% (F350),
property-access helpers ~27% self, `box_number` 3%, GC ~3%.

## Step 2: one replay of T80+T81 (a-03; ask the user before starting: memory "ask before long runs")
1. After step 1, sync the repo to a-03 (memory "a-03": `rsync -az --delete --exclude /target/ --exclude /out/
   --exclude /test262/ ./ harry@a-03:js2wasi3/`, never push to get code there), then on a-03
   `~/js2wasi3-prod-run/build-p.sh` (release js2wasi3, the zigbee2mqtt build with all device modules into `p.wasm`,
   `wasmtime compile` with 512 MiB into `p.cwasm`: ~5 min, 15 GB; it was changed to 512 MiB on 2026-10-04, untested).
2. One run, under nohup (15 min): `~/js2wasi3/tasks/T79-files/run-replay.sh ~/js2wasi3-prod-run/rec-20261004-1941
   wasm t79/out-t80t81` → `result.txt` (`cpu_steady_s`), RSS in `cpu.csv`, first minute from `cpu.csv`; publishes in
   `z2m.log` (13,476 expected for js2wasi3; node 13,567: F317). Compare with `t79/out-gc-c512m` (39.1 s, 765 MB).
   Don't `pkill -f` over ssh with a pattern from your own command line (memory "a-03"): kill by PID.
3. Profile only if the result needs explaining (`PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf` on that same
   run, not a second one).

## Then decide (with the new numbers)
Next large item, or none: F350 (wasmtime libcalls), F323/F334 (unboxed numbers), F321 (shapes), F352 (one-byte
strings). WasmGC limits that make them larger: struct layouts fixed at compile time (shapes as arrays of slots), no
interior pointers (shared substrings need our own slice type), i31 is 31 bits.

## Acceptance
- [ ] Step 1: T80/T81 on `main` authored by the user, no attribution anywhere; pushed after the user's OK.
- [ ] Step 2: T80+T81 measured in one replay run; table in `docs/zigbee2mqtt-feasibility.md`.
- [ ] The next item decided and recorded as a task or follow-ups.

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written (the measurement part of the earlier T80 plan; F347's 512 MiB scripts are untested
  until this task's first build).
- 2026-10-05: cloud batch checked (local session): T80, T81 done on `origin/speed-batch` (3 commits, author Claude:
  re-author in step 1), `main` untouched, T82 left todo as the rule says. User: handoff for a new session with step 1
  (merge, author fixed) and step 2 (one replay of T80+T81).
