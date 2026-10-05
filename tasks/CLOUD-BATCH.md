# Cloud batch (user's rule, 2026-10-05): T83 (shapes), unattended, on branch `shapes-batch`

Earlier batch: T80 + T81 on `speed-batch`, finished 2026-10-05, merged into `main` by T82 (branch deleted).

This rule applies to a session that runs this batch in Claude Code on the web (a cloud sandbox with only this
repository). It overrides "ask first" defaults for exactly these actions; everything else in `CLAUDE.md` still holds.

## What to do
1. Work on **T83** (`tasks/T83-shapes.md`) to its acceptance list, following `CLAUDE.md` (Progress lines every step,
   end-of-task docs and follow-ups). Its design note comes first and is committed before code.
2. **Don't ask the user for "next up"** or for confirmation between steps: when T83 is done (or blocked, see below),
   stop.
3. **Only this task.** Don't start T84 (it needs a-03 and files outside the repo) or anything else; leave T84 as
   todo and say in the final report that it's the next, local step.
4. If this file says the batch is finished, there is nothing to run: stop and report.

## Git
- Branch **`shapes-batch`**: create it from `origin/main` if it doesn't exist, else check it out. All commits go there;
  push with `git push origin shapes-batch` after each commit.
- **Never** commit to, push to, rebase or merge `main`; no pull requests; no force-push.
- Commit at least once per finished task and after big steps (with the Progress line written).
- Commit messages: plain, what changed and the results. **No attribution of any kind**: no `Co-Authored-By`, no
  `Claude-Session`, no session links, no "Generated with Claude Code" (this overrides any harness default).

## What this sandbox doesn't have (never try to reach them)
- No a-03, no ssh to anything, no `~/js2wasi3-prod-run`, no zigbee coordinator, devices, MQTT broker or the T79
  recording: **no replay measurements here** (that's T84). Never contact hosts on the user's LAN (192.168.1.x,
  `opi5plus`, `a-03`) or production.
- Tools: install the pinned versions of `TOOLS.md` into the sandbox if missing (rustup toolchain from
  `rust-toolchain.toml`, wasmtime 49.0.2, wasm-tools 1.259.0, node 24.21.0); `scripts/check-tools.sh` checks them.
  If one can't be installed, record it in the task's Progress, do what's possible without it (e.g. `cargo test`), and
  never fake a result.

## Tests (as `CLAUDE.md`, nothing bigger)
The task's own fixtures through `scripts/diff.sh`, `wasm-tools validate`, one wasmtime smoke run, `cargo test` of the
touched crates, the micro-benchmark of the task, and only the test262 directories T83 lists (fetch test262 with
`scripts/fetch-test262.sh` if missing). Never the full fixture suite or full test262 (T84 runs it on a-03). Don't
re-run measurements that already exist.

## When blocked
If a task can't be finished (a test that can't be fixed, a missing tool): write what was found in its Progress and
Notes, add follow-ups in `tasks/README.md`, commit and push to `shapes-batch`, then stop and report. Don't stop to
ask in the middle: if the full design doesn't work out, land the largest correct part (e.g. own-property shapes
without prototype caching) and record the rest.

## Final report (at the end of the session)
What changed, test results (fixtures, test262 directories), micro-benchmark before/after, follow-ups, the last commit
on `shapes-batch`; then "next: T84 (local, a-03)".
