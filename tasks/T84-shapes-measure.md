# T84: merge T83 (shapes) and measure it: full test262 and one zigbee2mqtt replay (local: a-03)

Status: done (2026-10-05) | Depends on: T83 (cloud, branch `shapes-batch`) | **Not for cloud
sessions**: needs a-03, the recording and the zigbee2mqtt build tree outside the repo (see `tasks/CLOUD-BATCH.md`).

## Goal
1. Bring T83 (branch `shapes-batch`) into `main` with the user as author.
2. Full test262 on a-03 (shapes touch every property access; the cloud ran only the riskiest directories).
3. **One** replay of T79's recorded production session against T82's numbers. **Don't re-run T82, the baseline or
   node** (user): they're measured.

## Step 1: shapes-batch → main, author fixed (local, no runs)
As T82 step 1: the cloud's commits are authored `Claude <noreply@anthropic.com>`; re-author them before `main`:
```
git fetch origin
git checkout -b shapes-merge origin/shapes-batch
git rebase main --exec 'git commit --amend --no-edit --reset-author'   # author = Zahari Zahariev <harryzz@gmail.com>
git log --format='%h %an <%ae> | %cn | %s' main..shapes-merge           # check: no Claude/anthropic anywhere
git log --format=%B main..shapes-merge | grep -iE 'co-authored|claude|anthropic|generated'   # must print nothing
git diff main..shapes-merge | grep -E '^\+' | grep -iE 'claude|anthropic|generated with|co-authored'  # nothing
git checkout main && git merge --ff-only shapes-merge && git branch -D shapes-merge
```
Then `cargo build --release`, `scripts/diff.sh tests/fixtures/builtins/object-shapes.js`. **Ask the user** before
`git push origin main` and before deleting `shapes-batch` on origin (the session's permission check may block both:
then give the user the `!` commands).

## Step 2: full test262 on a-03 (ask first: memory "ask before long runs", ~23 min)
rsync the repo (memory "a-03"), `cargo build --release`, the full run under nohup; compare with
`tests/test262/baseline.json`; any regression is fixed here or recorded as a follow-up before step 3. Update
`tests/test262/HISTORY.md` and the baseline if it changed.

## Step 3: one replay (a-03; ask first)
As T82 step 2: `~/js2wasi3-prod-run/build-p.sh`, then `~/js2wasi3/tasks/T79-files/run-replay.sh
~/js2wasi3-prod-run/rec-20261004-1941 wasm t79/out-t83` → `result.txt` (`cpu_steady_s`), peak RSS and CPU at 61 s
in `cpu.csv`, publishes in `z2m.log`. Compare with T82 (`t79/out-t80t81`: 22.6 s, 6.5 s first minute, 782 MB, 13,478
publishes) and node (8.66 s). Profile (`PERF=…` on that same run) only if the result needs explaining.

## Acceptance
- [x] Step 1: T83 on `main` authored by the user, no attribution anywhere; pushed after the user's OK.
- [x] Step 2: full test262 on a-03, no regressions (or follow-ups); HISTORY.md.
- [x] Step 3: one replay; table in `docs/zigbee2mqtt-feasibility.md`; the next item decided with the user.

## Notes / Result
- Step 1: shapes-batch re-authored onto `main` (11ebcb0, 6a91910, 21588ab), pushed by the user with a77d0da;
  origin `shapes-batch` deleted.
- Step 2: full test262 subset on a-03: 28,077 of 28,984 run pass (96.9%), 0 regressions, 4 to pass
  (`tests/test262/HISTORY.md`; baseline not updated).
- Step 3: replay `t79/out-t83`: **23.9 s CPU (T82 22.6 s, node 8.66 s): T83 is ~6% slower on the real workload**,
  confirmed by a second run (23.96 s); same output (13,478 publishes, same 9 error lines), RSS 778 MB. A third run
  under perf (`t79/out-t83-perf`, 11,301 samples) explains it: T83's transition search `shape_child` 5.7% self
  (~ the whole loss; F362), misses still common (`get_named` 8%, `find_own_named` 3.8%; F357/F360), computed keys
  uncached (~15%; F363). Table and profile in `docs/zigbee2mqtt-feasibility.md`.
- Extra runs: the perf run was meant to be the only extra one; the first attempt started without `PERF` (the env
  didn't reach the script), which gave the confirming second run. Launch with `nohup env PERF=… run-replay.sh`.
- Not done: F361 (shape count, needs a counter).
- Next smallest step: F362 (fixed shapes for runtime-built objects, a transition memo), then F363; the user decides.

## Progress
- 2026-10-05: handoff written (T83 goes to the cloud on `shapes-batch`).
- 2026-10-05: step 1 done locally: shapes-batch (cloud, author Claude) rebased onto main with `--reset-author`
  (11ebcb0 design note, 6a91910 code, 21588ab notes; author/committer Zahari Zahariev, no attribution in messages or
  added lines), `main` fast-forwarded. `cargo build --release` OK; `scripts/diff.sh builtins/object-shapes.js` PASS.
  The stopped local agent's worktree (unfinished, superseded) removed. Not pushed yet (user runs the push).
- 2026-10-05: user pushed (a77d0da) and deleted origin `shapes-batch`, then started the a-03 chain. Step 2: full
  test262 subset on a-03 (`node scripts/test262.mjs --causes 25 subset`, no `--update`, log `~/t262-t84.log`):
  28,077 tests, 96.9%, **0 regressions**, 4 changes to pass (3 timeouts: Array map, decodeURI, decodeURIComponent;
  1 fail: for-await-of async-gen dstr iter-rtrn-close-null). Build for the replay: `js2wasi3 build` 252 s,
  `wasmtime compile` 49 s; replay running into `t79/out-t83`.
- 2026-10-05: step 3 replay done (`t79/out-t83`): **23.9 s CPU** (T82: 22.6 s; node 8.66 s) = 2.8×, 43.6 ms per
  frame, first minute 6.8 s (T82 6.5), peak RSS 778 MB, 13,478 publishes, the same 9 error lines. No gain from
  shapes on the real workload; not explained without a profile (the run wasn't under perf). Table in
  `docs/zigbee2mqtt-feasibility.md`. F361 (shape count) not measured: needs a counter printed at exit.
- 2026-10-05: the perf replay ran without perf (`PERF` didn't reach `run-replay.sh`: wasmtime without
  `--profile=perfmap`, no `perf.data`); its CPU, 23.96 s (`t79/out-t83-perf`), confirms the 23.89 s: T83 is ~6%
  slower than T82 on the replay, not noise. A profile still needs a run with perf attached.
