# T19: async/await, the P3 event loop and timers

Status: todo | Depends on: T18

## Goal
`async function` and `await` built on T18's transform. Top-level `await`. The full callback event loop (D3): drain microtasks,
then `WAIT` on the waitable set, and in the callback settle the matching promise and loop again. `setTimeout` via
`wasi:clocks/monotonic-clock@0.3.1#wait-for` (async-lowered subtask).

## Read first
`probes/cmd.wat` (WAIT/callback codes, subtask state packing), `probes/cmd2.wat`.

## Acceptance
- Fixtures `async/*.js` (ordering, await in loops, rejected await in try/catch, two concurrent timers) match node.
- Two concurrent 50 ms timers finish in about 50 ms, not 100 ms.

## Notes / Result
