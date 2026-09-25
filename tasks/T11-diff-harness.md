# T11: Differential test harness (node vs wasmtime)

Status: done | Depends on: T10

## Goal
One command runs a fixture through `node` and through js2wasi3 + `wasmtime -Sp3`, then diffs stdout and the exit status.

## In scope
- `cargo xtask diff <fixture|dir>` or `scripts/diff.sh`. Parallel runs, a summary, and failing diffs shown.
- Fixtures are self-checking where possible. A `.skip` sidecar holds the reason for a known gap.
- A run on one fixture must take under 2 s.

## Acceptance
- [x] Every current fixture passes, or is skipped with a reason.

## Progress
- 2026-09-25: Start. Tools unchanged. Decisions: `scripts/diff.sh` (bash, `xargs -P` for parallelism; no new crate).
  Compared: stdout, stderr and the exact exit status. Node runs fixtures as ES modules (`tests/fixtures/package.json`
  with `"type": "module"`), matching what js2wasi3 compiles (strict mode). Library fixtures (with exports: nothing to
  run) get a `.skip` with the reason; their semantics stay covered by the `--invoke` tests in `crates/js2wasi3/tests/build.rs`.
  `tests/fixtures/unsupported/` isn't a diff target (rejection tests). F27: keep `err` when stdout's reader goes away
  (the write failed; WASI semantics), and the harness doesn't cover it.
- 2026-09-25: `scripts/diff.sh` written. First run: console/hello pass, the 6 library fixtures fail as expected
  (`no exported instance named wasi:cli/run`) → `.skip` sidecars. Added `tests/fixtures/semantics.js` (values, typeof,
  truthiness, `===`/`==` tables, number formatting and arithmetic, strings, control flow, recursion); first draft used an
  array literal and `String` (unsupported, reported as a build failure by the harness), removed. Passes.
  Negative check: a `%` format string at run time shows the stdout, stderr and status diffs, exit 1.
  Full run: 3 passed, 6 skipped, 0.46 s wall; each fixture 250–300 ms.

## Notes / Result
**Done.** `scripts/diff.sh [-j jobs] [fixture.js | dir]...` (default `tests/fixtures`, not recursive) builds the compiler
once, then for each fixture in parallel (`xargs -P`, `nproc` jobs) runs `node` and `js2wasi3 build` + `wasmtime run -Sp3`
(10 s timeout each) and compares stdout, stderr and the exact exit status. It prints PASS/FAIL/SKIP per fixture with
its time (flagged SLOW at ≥ 2 s), the unified diffs of what differs (or the build diagnostics), a summary, and exits 1 on
any failure. `x.skip` next to `x.js` skips it with the reason on its first line.

Other changes: `tests/fixtures/package.json` (`"type": "module"`) so node runs fixtures as ES modules (strict mode), like
js2wasi3; `.skip` for the 6 library fixtures; new `tests/fixtures/semantics.js` (the T05–T09 subset as a command).

Commands run: `scripts/diff.sh` (3 passed, 6 skipped), a negative check (a run-time trap is reported as a diff),
`scripts/diff.sh -j 2 <file> <file>`. No Rust changes. shellcheck isn't installed, so the script wasn't linted.

Decisions: F27 is closed. When stdout's reader goes away, `run` returns `err`, because the write failed (WASI
semantics). node exits 0 there. The harness doesn't cover that case. F15 (`--invoke` keyword names) doesn't
concern the harness, which runs commands, so it moves to T20.

Not covered / later: library fixtures (no harness for exports: their `--invoke` tests stay), the harness is not wired into
`cargo test`, uncaught exceptions will need a stderr comparison mode (node prints a stack trace) — see F29/F30.

Next smallest step (T12): lower `throw` to `throw $js` and `try/catch` to `try_table`, with a first command fixture that
throws and catches a primitive, checked with `scripts/diff.sh`.
