# T10: Command world: `console.log` over WASI 0.3 stdout

Status: todo | Depends on: T09

## Goal
A file without exports compiles to a `wasi:cli/run@0.3.1` command. Top-level code runs, and `console.log` writes
to stdout through `write-via-stream` (`stream<u8>`).

## Read first
`probes/cmd.wat` (exact import names, stream write, callback loop), `probes/wit/` (WASI 0.3.1 deps via `wkg wit fetch`).

## In scope
- The export is async-lifted with a callback (D3). v0 may run top-level code in one go, then flush writes
  by returning `WAIT` on blocked writes. Await the stdout `future` before `task.return`.
- `console.log` takes primitives joined by spaces (formatting objects comes later). `console.error` goes to stderr.
- Keep the WASI calls inside a small runtime "host ABI" module in the backend. JS lowering calls only its intrinsics.

## Out of scope
Promises and timers (T17/T19), stdin, env, exit codes other than ok/err.

## Acceptance
- `js2wasi3 build tests/fixtures/hello.js -o hello.wasm && wasmtime run -Sp3 hello.wasm` prints `hello from js2wasi3`.

## Notes / Result
