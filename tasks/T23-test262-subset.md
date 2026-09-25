# T23: test262 subset runner

Status: todo, run only with approval | Depends on: T12+

## Goal
Run a chosen subset of test262 (by directory or feature list) through js2wasi3 + wasmtime and report the pass rate per directory.
Store the results so a later run only shows the tests whose result changed.

## Rules
Never run it after every edit. Only at the end of a task or on request. Ask before any run expected to take more than 10 minutes.

## Notes / Result
