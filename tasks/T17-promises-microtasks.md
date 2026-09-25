# T17: Promises and the microtask queue (no suspension yet)

Status: todo | Depends on: T16

## Goal
`Promise` (constructor, resolve/reject, `then/catch/finally`, `Promise.resolve/all`), `queueMicrotask`, and the job queue.
In the command world, drain the queue after top-level code and before `task.return`. Unhandled rejections go to stderr and return `err`.

## Read first
`docs/design.md` D3 (the event loop shape).

## Acceptance
- Fixtures `promises/*.js` (ordering tests) print the same order as node.

## Notes / Result
