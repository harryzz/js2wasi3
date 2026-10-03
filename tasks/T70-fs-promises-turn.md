# T70: `fs.promises` settle in a later event-loop turn, like node's thread pool (F300)

Status: done (2026-10-03) | Depends on: T59, T66

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
- [x] The fixture equals node (the reproducer's order included).
- [x] zigbee2mqtt's restart logs `Port closed` where node does, or the remaining difference is explained.
- [x] Follow-ups updated (F300 ticked).

## Notes / Result
**What changed**
- `crates/frontend/node/fs.js`: `later(run)` runs the sync call at once and settles in a later turn (`setImmediate`),
  as the callbacks already did; errors of the call itself (`syscall`) reject later, argument errors at once. Used by
  `promisify` (all of `fs.promises`), every `FileHandle` method, `promises.open`/`opendir`.
- The handoff's cause was only part of it: zigbee2mqtt's restart doesn't await `fs.promises` between the stop and
  the next start. node runs `index.js`, whose `start()` first awaits `checkDist()` → `exec("git rev-parse …")`, real
  process I/O, so the loop turns and the old socket's `close` runs; T62's `main.js` skipped that (T62 had no
  `child_process`). node's `import()` doesn't turn the loop (checked).
- `tasks/T62-files/main.js` now awaits `index.js`'s `currentHash()` (it gets "unknown", as node does outside a git
  checkout).
- `crates/frontend/node/child_process.js` (user's decision): a shell command (`exec`, `execSync`, `execFile`/`spawn`/
  `spawnSync` with `shell: true`, `/bin/sh` or `sh`) runs as on a system whose `/bin/sh` (dash) finds no program:
  `pid`, `spawn`, then after a turn stderr `/bin/sh: 1: <first word>: not found`, `exit` 127, unread pipes resumed,
  `close`; `exec` calls back with node's `Command failed: <cmd>\n<stderr>` (code 127, `killed`, `signal`, `cmd`);
  `execSync` throws it (and writes the line to stderr without `stdio`); `spawnSync` returns status 127. Other shells
  and programs run directly stay missing (`spawn <file> ENOENT` in a nextTick, as node).
- Fixtures: `node/fs-promises-turn.js` (+ `.fs/`), `node/child-process-shell.js`. Docs: README, design.md (T59 and
  T65 paragraphs), keywords.md, zigbee2mqtt-feasibility.md.

**Commands run**: the fixtures (node stable over 10 / 5 runs) and `diff.sh tests/fixtures/node` (37 pass),
`tests/fixtures/commonjs` (14 pass); `cargo test --release --no-fail-fast` (pass); zigbee2mqtt rebuilt three times
(fs.promises, driver, shell) and `restart-run.sh wasm` against the live rig with the 256 MiB heap: finally `Port
closed` between `Starting Zigbee2MQTT without watchdog.` and `Logging to console`, as in node's restart log; the
other differences are the known version stub and start-up timing. Retained test topics cleared (15). Bytes: programs
using `fs` or `child_process` change (their modules' code).

**Unsupported** (F305): shell built-ins and lists, `spawn("sh", …)` without `shell`, shells other than `/bin/sh`,
`close` timing of the emulated child, `Dir` iteration in the same turn.

**Next smallest step**: F288's order (the TLS client, F281); no task file yet.

## Checklist
- [x] fixture
- [x] fs.promises after a turn
- [x] zigbee2mqtt restart, docs, follow-ups

## Progress
- 2026-10-03: handoff written after T67 (`main` at `0bf597c`); cause found with the reproducer above.
- 2026-10-03: started; tools unchanged. Fixture `node/fs-promises-turn.js` (+ `.fs/data.txt`): the reproducer, a `setImmediate`/microtask before an awaited `stat`, a rejection after a `setImmediate`, a promise pending after one microtask; node stable over 10 runs; js2wasi3 fails as expected (close/immediates after the results, settled at once).
- 2026-10-03: fs.js: `later(run)` (sync call now, settle with `setImmediate`; syscall errors later, argument errors at
  once) for `promisify`, every `FileHandle` method, `open`, `opendir` (`Dir`'s `entries()` generator left). Fixture
  equals node; `diff.sh tests/fixtures/node` 36 pass, `commonjs` 14 pass; `cargo test` pass. zigbee2mqtt restart
  (rebuilt, 256 MiB heap): `Port closed` still after the new socket's `Opening TCP socket`. Cause of F300 is the
  driver: node runs `index.js`, whose `start()` awaits `checkDist()` → `exec("git rev-parse")` (process I/O: the loop
  turns, the old socket's `close` runs); T62's `main.js` skipped it (no `child_process` then). node's `import()`
  doesn't yield a turn (checked: new, required and imported modules all resolve before a `setImmediate`). `main.js`
  now awaits the same `currentHash()`.
- 2026-10-03: the driver's `exec("git …")` still didn't move `Port closed`: js2wasi3's `exec` failed like a system
  without `/bin/sh` (`spawn /bin/sh ENOENT` in a nextTick, T65's model); node's shell runs and exits 127 after the
  loop turns (`spawn`/`execFile` of a missing program are ENOENT in a nextTick in node too). User decision: emulate
  a shell that finds no program. `child_process.js`: `shell: true`/`/bin/sh`/`sh` (exec, execSync, spawn, spawnSync,
  execFile with `shell`) start (`pid`, `spawn`), write `/bin/sh: 1: <first word>: not found`, `exit` 127, resume
  unread pipes (node's `flushStdio`), `close`; `exec` calls back on `close` with node's `Command failed: …` error;
  `execSync` throws it (status, output, stdout/stderr Buffers) and writes the line to stderr without `stdio`;
  `spawnSync` returns status 127 with no `error`. Other shells (`shell: "/no/sh"`) stay missing programs. Shell
  built-ins are "not found" too (deviation). Fixture `node/child-process-shell.js` (stable over 5 node runs) equals
  node; `node/child-process.js` (T65) still passes.
- 2026-10-03: zigbee2mqtt restart with the shell emulation: `Port closed` where node has it. `diff.sh tests/fixtures/node` 37 pass, `cargo test` pass; retained test topics cleared. Docs, F300 ticked, F305. Done.
