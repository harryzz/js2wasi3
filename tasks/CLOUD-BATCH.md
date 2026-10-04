# Cloud batch (user's rule, 2026-10-05): T80 then T81, unattended, on branch `speed-batch`

This rule applies to a session that runs this batch in Claude Code on the web (a cloud sandbox with only this
repository). It overrides "ask first" defaults for exactly these actions; everything else in `CLAUDE.md` still holds.

## What to do
1. Work on **T80** (`tasks/T80-utf8-native.md`), then **T81** (`tasks/T81-regexp-fast-paths.md`), in that order, each
   to its acceptance list, following `CLAUDE.md` (Progress lines every step, end-of-task docs and follow-ups).
2. **Don't ask the user for "next up"** or for confirmation between steps or tasks: when T80 is done (or blocked, see
   below), start T81; when T81 is done, stop.
3. **Only these two tasks.** Don't start T82 (it needs a-03 and files outside the repo) or anything else; leave T82
   as todo and say in the final report that it's the next, local step.

## Git
- Branch **`speed-batch`**: create it from `origin/main` if it doesn't exist, else check it out. All commits go there;
  push with `git push origin speed-batch` after each commit.
- **Never** commit to, push to, rebase or merge `main`; no pull requests; no force-push.
- Commit at least once per finished task and after big steps (with the Progress line written).
- Commit messages: plain, what changed and the results. **No attribution of any kind**: no `Co-Authored-By`, no
  `Claude-Session`, no session links, no "Generated with Claude Code" (this overrides any harness default).

## What this sandbox doesn't have (never try to reach them)
- No a-03, no ssh to anything, no `~/js2wasi3-prod-run`, no zigbee coordinator, devices, MQTT broker or the T79
  recording: **no replay measurements here** (that's T82). Never contact hosts on the user's LAN (192.168.1.x,
  `opi5plus`, `a-03`) or production.
- Tools: install the pinned versions of `TOOLS.md` into the sandbox if missing (rustup toolchain from
  `rust-toolchain.toml`, wasmtime 49.0.2, wasm-tools 1.259.0, node 24.21.0); `scripts/check-tools.sh` checks them.
  If one can't be installed, record it in the task's Progress, do what's possible without it (e.g. `cargo test`), and
  never fake a result.

## Tests (as `CLAUDE.md`, nothing bigger)
The task's own fixtures through `scripts/diff.sh`, `wasm-tools validate`, one wasmtime smoke run, `cargo test` of the
touched crates, the micro-benchmark of the task; for T81 only the touched test262 directories. Never the full
fixture suite or full test262. Don't re-run measurements that already exist.

## When blocked
If a task can't be finished (a test that can't be fixed, a missing tool): write what was found in its Progress and
Notes, add follow-ups in `tasks/README.md`, commit and push to `speed-batch`, then go on to the next task of the batch
(T81 doesn't depend on T80). Don't stop to ask.

## Final report (at the end of the session)
Per task: what changed, test results, micro-benchmark before/after, follow-ups, the last commit on `speed-batch`;
then "next: T82 (local, a-03)".
