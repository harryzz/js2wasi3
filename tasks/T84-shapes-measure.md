# T84: merge T83 (shapes) and measure it: full test262 and one zigbee2mqtt replay (local: a-03)

Status: todo (handoff written 2026-10-05) | Depends on: T83 (cloud, branch `shapes-batch`) | **Not for cloud
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
- [ ] Step 1: T83 on `main` authored by the user, no attribution anywhere; pushed after the user's OK.
- [ ] Step 2: full test262 on a-03, no regressions (or follow-ups); HISTORY.md.
- [ ] Step 3: one replay; table in `docs/zigbee2mqtt-feasibility.md`; the next item decided with the user.

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written (T83 goes to the cloud on `shapes-batch`).
