# T70: `fs.promises` settle in a later event-loop turn, like node's thread pool (F300)

Status: todo (handoff written 2026-10-03) | Depends on: T59, T66

## Goal
F300: on a zigbee2mqtt restart, the old socket's `close` (herdsman's `Port closed`) is logged after the new socket's
`Opening TCP socket` (node: before the restart's first log line). Found cause (below): `fs.promises` calls settle in
the same turn, so a chain of awaited file operations never lets the event loop run, and `net.Socket`'s `close`
(a `setImmediate` after `destroy()`) waits until the next real I/O wait. Make `fs.promises` (and `FileHandle`'s
methods) settle after a turn of the event loop, as the callback API already does, so that reproducer and the
zigbee2mqtt restart order equal node's. Programs that don't use `fs.promises` keep their bytes.

## What we know (at handoff)
- Reproducer (2026-10-03, `tests/fixtures` style; a TCP server and client in one program, a file `data.txt`):
  `sock.destroy()`, then three `await fs.readFile("data.txt", "utf8")` (from `node:fs/promises`), logging each, then
  `await new Promise((r) => setImmediate(r))`. node: `destroyed`, `close`, `readFile 0`, `readFile 1`,
  `readFile 2`, `after setImmediate`. js2wasi3: `destroyed`, `readFile 0..2`, `close`, `after setImmediate`.
- `crates/frontend/node/fs.js`: `callbackify` runs the sync call at once and delivers the result with `setImmediate`
  (T59's comment: node runs it on its thread pool); `promisify` is `async (...args) => sync(...args)`, so the promise
  is already settled when returned. `FileHandle` (same file, after `// fs.promises' FileHandle`) and
  `promises.open`/`opendir` are `async` functions too.
- `net.js` `_destroy` emits `close` with `setImmediate` once the socket was opened (T66: "after the jobs and ticks",
  approximating libuv's close callback); that part is right once file promises yield.
- node's order for one `fs.promises.readFile`: it takes several loop turns (open, fstat, read, close each on the pool),
  so `close` came before `readFile 0`. One `setImmediate` per call is the smallest change; whether multi-step calls
  (`readFile`, `writeFile`, `appendFile`, `copyFile`, `mkdir` recursive) need more than one turn to match node in the
  reproducer and in the fs fixtures is to be checked, not assumed.

## Plan
1. Fixture `tests/fixtures/node/fs-promises-turn.js` (`.fs/` dir as other fs fixtures, see `scripts/diff.sh`): the
   reproducer above, plus orderings that node makes deterministic: a `setImmediate` and a `setTimeout(0)` scheduled
   before an awaited `fs.promises.stat`, a callback-API call racing a promise call. Check with node several times
   that each ordering is stable before keeping it.
2. `promisify`, `FileHandle` methods, `open`/`opendir`: resolve or reject after `setImmediate` (errors too: node's
   `syscall` errors reject later, argument errors still reject at once? check node: `fs.promises.readFile(1n)`).
3. `scripts/diff.sh` on `tests/fixtures/node` (all fs fixtures, `net.js`, `dgram.js`), `cargo test`, bytes check.
4. zigbee2mqtt restart run (`tasks/T68-files/restart-run.sh node|wasm`, test topics only, clear retained test topics
   after): `Port closed` where node has it; record any other order change.

## Read first
- `tasks/T59-fs.md` (Notes / Result), `crates/frontend/node/fs.js` (callbacks and promises), `crates/frontend/node/net.js`
  (`_destroy`), `tasks/T68-capture-stack-trace.md` (Progress: the restart run).

## Tests
The new fixture and `tests/fixtures/node` with `scripts/diff.sh`; `cargo test`; `wasm-tools validate` + wasmtime; bytes
check of the command fixtures against HEAD (only programs using `fs.promises` may change).

## Acceptance
- [ ] The fixture equals node (the reproducer's order included).
- [ ] zigbee2mqtt's restart logs `Port closed` where node does, or the remaining difference is explained.
- [ ] Follow-ups updated (F300 ticked).

## Notes / Result
(to be written)

## Checklist
- [ ] fixture
- [ ] fs.promises after a turn
- [ ] zigbee2mqtt restart, docs, follow-ups

## Progress
- 2026-10-03: handoff written after T67 (`main` at `0bf597c`); cause found with the reproducer above.
