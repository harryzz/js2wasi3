# T11: Differential test harness (node vs wasmtime)

Status: todo | Depends on: T10

## Goal
One command runs a fixture through `node` and through js2wasi3 + `wasmtime -Sp3`, then diffs stdout and the exit status.

## In scope
- `cargo xtask diff <fixture|dir>` or `scripts/diff.sh`. Parallel runs, a summary, and failing diffs shown.
- Fixtures are self-checking where possible. A `.skip` sidecar holds the reason for a known gap.
- A run on one fixture must take under 2 s.

## Acceptance
- Every current fixture passes, or is skipped with a reason.

## Notes / Result
