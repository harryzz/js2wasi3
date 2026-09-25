# T18: Suspension transform: generators as state machines

Status: todo | Depends on: T17

## Goal
An IR pass (D3) that turns a function containing `Suspend` points into a resumable frame: a GC struct holding the live locals,
a state index and a dispatch on entry. Use it for `function*`: `next/return/throw`, `yield*`, and try/finally around `yield`.

## In scope
- The pass lives in `crates/ir` and is backend-neutral. Write down its algorithm (liveness, how `try` regions are split) in the crate docs.
- Generator objects work with T15's iterator protocol.

## Out of scope
async (T19). The stack-switching alternative (T22).

## Acceptance
- Fixtures `generators/*.js` match node, including `return()` running `finally` blocks.

## Notes / Result
