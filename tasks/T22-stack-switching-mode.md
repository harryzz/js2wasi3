# T22 (optional): Stack-switching backend mode + benchmark

Status: todo, needs approval | Depends on: T19

## Goal
A `--suspend=stack-switching` option lowers `Suspend` to `cont.new/resume/suspend` instead of the T18 state machine.
Compare code size, speed and memory on generator- and async-heavy benchmarks. Keep it behind the option.

## Read first
`probes/ss.wat`, `probes/cmd2.wat`, `probes/bench_*.wat`, `docs/design.md` D3.

## Acceptance
- The same fixtures pass in both modes. A table of benchmark results goes in the Result.

## Notes / Result
