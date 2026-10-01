# T59: node's `fs` over `wasi:filesystem` (F238)

Status: done (2026-10-01) | Depends on: T19/T20 (the P3 callback loop, WIT streams), T55
(built-in modules), T56 (`Buffer`)

## Goal
Programs and packages that use `fs` behave like node 24.21.0 for files under wasmtime's preopened directories
(`wasmtime run --dir host::guest`):
1. The sync API zigbee2mqtt uses (docs/zigbee2mqtt-feasibility.md): `readFileSync`, `writeFileSync`,
   `appendFileSync`, `existsSync`, `renameSync`, `unlinkSync`, `rmSync`, `readdirSync` (also `withFileTypes`),
   `mkdirSync` (`recursive`), `copyFileSync`, `symlinkSync`, `readlinkSync`, `statSync`/`lstatSync` (`Stats`, `isFile()`…),
   `openSync`/`readSync`/`writeSync`/`fsyncSync`/`closeSync`, `accessSync`, `realpathSync`.
2. node's errors: `ENOENT: no such file or directory, open 'x'` with `errno`/`code`/`syscall`/`path`.
3. `fs.promises` / `node:fs/promises` and the callback forms of the same functions (on the event loop).
4. Encodings through `Buffer` (T56): a string with an encoding, otherwise a `Buffer`.
Programs that don't use `fs` keep their bytes (the imports exist only when used, like the clock).

## What we know (at handoff)
- WIT: `crates/component/wit/wasi-filesystem-0.3.1.wit`: every `descriptor` method is an **`async func`**
  (`open-at`, `stat-at`, `create-directory-at`, `rename-at`, `unlink-file-at`, …); file contents go through
  `read-via-stream(offset) -> (stream<u8>, future<result>)` / `write-via-stream(stream<u8>, offset)`;
  `read-directory() -> (stream<directory-entry>, future)`; `preopens.get-directories()` is sync.
- Commands are lifted with the callback ABI (design D3; `crates/backend/src/host.rs`): output streams are written
  asynchronously from the event loop, never blocking.
- node's sync API must block. **Open question, probe first**: can a callback-lifted `run` task call an async import
  synchronously in wasmtime 49 (a sync-lowered `async func`, or `[async-lower]` + `waitable-set.wait`), and read a
  `stream<u8>` to its end with blocking `stream.read`? If not, the sync API needs a different shape (e.g. the
  whole program run as a non-callback async task, or stackful `thread`s: see the probes and memory notes on
  stack switching).
- Paths: node resolves relative paths against `process.cwd()` (T55: from `wasi:cli/environment`); WASI needs a
  preopen and a path relative to it: pick the longest preopen prefix of the absolute path (wasi-libc's rule).
- Errors: `wasi:filesystem` `error-code` → node's `code`/`errno` (Linux values) and message.
- Built-in modules are CommonJS files in `crates/frontend/node/` (T55); WASI calls would be prelude intrinsics
  (`__intrinsic_*`), lowered to backend helpers in `host.rs` (or a new `fs.rs`).

## Design questions (decide first, record in docs/design.md D3/D4)
1. Blocking from the callback-lifted task (the probe above): which mechanism, and its cost to programs without `fs`.
2. The ABI surface: a few generic intrinsics (open, read-all, write-all, stat, readdir, mutate-at(op, path…)) vs one
   per WIT function; where descriptors live (a JS object around an `i32` handle; dropping them).
3. The async forms: real async (subtasks in the event loop's waitable set, like `wait-until`) or the sync calls
   wrapped in a resolved promise / `process.nextTick` callback (node's ordering differs; measure what tests see).
4. `Stats`: which fields `descriptor-stat` gives (no `uid`/`gid`/`ino`/`mode` bits?): node's values where WASI has
   none.

## Plan (slices; each ends with fixtures) — revised 2026-10-01 after the probe and the design decision
1. **WASI imports in commands.** Without `--wit`, a command may `import { f } from "wasi:<pkg>/<iface>@0.3.1"` for
   the vendored WASI interfaces (`crates/component/wit`): the CLI builds a `Boundary` from them (like
   `match_world`), and the command's world imports what the core module uses. An `async func` import in a command
   is **sync-lowered: it blocks** and returns its value (libraries keep T20's promises). Fixture: a command calling
   `wasi:random/random` / `wasi:clocks` functions directly; then `preopens.getDirectories()`.
2. **Resources, low-level.** A handle (`own`/`borrow`) is a JS number; `[method]r.m`, `[static]r.m`,
   `[constructor]r` and `[resource-drop]r` are importable by their WIT names as string imports
   (`import { "[method]descriptor.open-at" as openAt } from "wasi:filesystem/types@0.3.1"`), `self` first. jco's
   resource classes for user worlds: follow-up.
3. **Streams and futures in blocking calls.** In a command, a `stream<u8>`/`stream<T>` the callee returns is read to
   its end (async `stream.read` + `waitable-set.wait`, probe 9) → a `Uint8Array`/array, then each returned future
   is read → its value; a stream argument is given as a `Uint8Array`/array, written fully after the call and
   dropped. Order: the call, written streams, read streams, then futures.
4. **`node:fs` sync API** in JS over (1)–(3): a built-in ES module (`node:internal/wasi`?) importing the WASI
   functions, required by `node/fs.js`; preopens + path resolution, `error-code` → node's errors, `Stats`.
5. `fs.promises`/callbacks (over the sync calls first), real packages, gating, docs, follow-ups.

## Read first
- `crates/component/wit/wasi-filesystem-0.3.1.wit`, `crates/backend/src/host.rs` (module doc: the loop), `abi.rs`
  (stream/future built-ins, async-lower adapters for user worlds: T20 slice 5b/6).
- `tasks/T55-node-builtins.md` (Notes / Result: how a built-in module reaches the host), `tasks/T56-buffer.md`.
- `docs/design.md` D3 (async), D4 (T55 built-ins); `probes/run.sh`; `docs/zigbee2mqtt-feasibility.md`.

## Tests
Fixtures `tests/fixtures/node/fs*.js` via `scripts/diff.sh` (needs a preopened scratch directory: extend `diff.sh`
with `x.dirs` or a fixed `--dir`, node running in the same directory); `cargo test`; `wasm-tools validate` +
wasmtime; gating (programs without `fs` keep their bytes).

## Acceptance
- [x] The sync API above behaves like node on files and directories under a preopen (fixtures).
- [x] node's error objects for the common failures (ENOENT, EEXIST, ENOTDIR, EISDIR, ENOTEMPTY, EACCES).
- [x] `fs.promises` and callbacks for the same functions.
- [x] Programs without `fs` compile to the same bytes.

## Notes / Result
**What changed.**
- *WASI imports in commands* (`component::wasi_world`, `COMMAND_WASI_IMPORTS`; CLI `import_boundary(…, blocking)`):
  a command may `import { f } from "wasi:<pkg>/<iface>@0.3.1"` (clocks/types, random/random, random/insecure-seed,
  filesystem/types+preopens, sockets/types+ip-name-lookup); only the interfaces its `import`s name enter its boundary
  (`frontend::Program::import_sources`), so other programs keep their bytes.
- *Blocking calls* (`ir::AbiSig::blocking`): `async func`s are sync-lowered; returned streams are read to their end
  (`$$StreamReadAll`, `StreamReadSync`), returned futures to their value (`FutureLiftSync`), stream arguments written
  after the call (`StreamLowerSync`, `$$StreamDefer`/`$$StreamFlush`, `StreamWriteSync`); `BlockOn` waits on a fresh
  waitable set (`[waitable-set-wait]`/`[waitable-set-drop]` imported only then). Probe 9 (`probes/fs.wat`).
- *Resources* as numbers: `TypeDefKind::Handle` → `u32`; `[method]`/`[static]`/`[constructor]` functions and a
  synthetic `[resource-drop]r` importable by WIT name (user worlds' imports too; exported resources still rejected).
- *`node:fs`* (`node/fs.js`, `node/wasi_fs.mjs` as `node:internal/wasi_fs`), `fs/promises`, `constants`;
  `node:internal/…` built-ins are ES modules. `diff.sh`: `x.fs/` directories.

**Commands run.** Probes 9 (with and without 🚝); `cargo test` (new `commands_import_wasi_interfaces`: random,
filesystem resources, streams); `scripts/diff.sh` on `node/` (16 pass incl. `fs.js`, `fs-async.js`) and on all of
`tests/fixtures` (see Progress); `cmp.sh` (HEAD's compiler vs this one, every fixture: 237 same after each slice);
packages: `mkdirp` (sync + promise) matches node; `fs-extra` needs `graceful-fs` → `stream`; `rimraf@3` needs
`console.trace` (F253).

**Unsupported** (F251–F253): re-exporting a WIT binding; `createReadStream`/`createWriteStream`/`watch` (stream);
partial reads read the rest of the file; `fs` in libraries; permissions/owners/inodes; `opendir`, `cp`; callbacks
and promises run the call at once; non-`u8` streams not joined; future arguments; promise-returning WASI calls in
commands.

**Next smallest step:** `stream` (F238): a task file; candidates node's `lib/internal/streams` or `readable-stream`
(npm, MIT; what `graceful-fs`/`fs-extra` and zigbee2mqtt's packages use), then `fs.createReadStream` over it.

## Checklist
- [x] probe + design recorded
- [x] slice 1: WASI imports in commands (blocking `async func`s)
- [x] slice 2: resources as handles
- [x] slice 3: streams/futures in blocking calls
- [x] slice 4: `node:fs` sync API
- [x] slice 5: promises/callbacks, packages, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T58 (`main` at `d43ad15`). `stream` (and `createReadStream`) is the next task.
- 2026-10-01: probe 9 (`probes/fs.wat`, world `cmdfs` in `probes/wit/world.wit`, `probes/run.sh`): from the
  callback-lifted `run`, **sync-lowered `async func`s block** (`wait-for`, `get-directories`, `open-at`: plain import
  names, no `[async-lower]`), and `read-via-stream`'s `stream<u8>` is read to its end. Sync `stream.read`/`write`
  need wasmtime's `component-model-more-async-builtins` (🚝, off by default); instead `[async-lower]` stream ops that
  return BLOCKED (-1) are joined to a fresh waitable set and `[waitable-set-wait]` blocks until the event (payload =
  the op's result). Works under plain `wasmtime run -Sp3 --dir host::/`. So the sync API needs no new
  architecture: design question 1 is answered (sync lowering + `waitable-set.wait`, only in programs using `fs`).
- 2026-10-01: decision (with the user): **generic WIT imports**, not fs-specific helpers. T20's import machinery
  (`import { f } from "ns:pkg/iface"`, Canonical ABI lowering of records/results/lists/enums/flags) is extended so
  `node/fs.js` is plain JS over `wasi:filesystem`: WASI interfaces importable in commands from the bundled WIT
  (`crates/component/wit`), resources (handles, `[method]`, `[resource-drop]`), and a blocking mode for `async func`s
  and streams (probe 9). `wasi:sockets` (net) and `wasi:random` (crypto) reuse it later.
- 2026-10-01: slice 1 implemented. `component::wasi_world()` (a generated world importing
  `COMMAND_WASI_IMPORTS`: clocks/types, random/random, random/insecure-seed, filesystem/types+preopens,
  sockets/types+ip-name-lookup; `wasi:clocks/timezone` is feature-gated in the vendored WIT, left out; the
  interfaces the runtime imports itself are left out to avoid duplicate core imports), `command_world` imports them
  too (unused ones drop out of the component). CLI: `import_boundary(…, blocking)` split out of `match_world`; a
  command (`--command` or no exports) lowers with that boundary, its `async func`s sync-lowered (plain import name,
  `is_async: false`), functions with streams/futures rejected for now (before their types are recorded, so the
  stream prelude isn't pulled in). Works: `getRandomBytes(8)` → `Uint8Array(8)`; `resolveAddresses("localhost")`
  (an `async func`) blocks and returns `[{ tag: 'ipv4', … }]`, an `err` throws with `payload` `{ tag:
  'invalid-argument' }` (`wasmtime run -Sp3 -S inherit-network,allow-ip-name-lookup`).
- 2026-10-01: slice 1 gating: the boundary's types decide some of the runtime (a `list<u8>` anywhere enabled `$typed`: 121 fixtures changed), so the command's boundary has only the interfaces its `import`s name (`frontend::Program::import_sources`). Then `cmp.sh` (HEAD's compiler vs this one, every fixture): 237 same, 0 different.
- 2026-10-01: slice 2 done: a handle type is `Ty::Scalar(U32)` (a JS number); an interface's resource functions are
  importable by their WIT names (`[method]descriptor.open-at`, `self` first; JS name = WIT name) and each resource
  gets a synthetic `[resource-drop]r`; exported interfaces still reject resource functions. Fixtures
  `tests/fixtures/wasi/{random,filesystem}.js` (`.skip` for `diff.sh`: node can't run them), run by
  `crates/js2wasi3/tests/build.rs::commands_import_wasi_interfaces` with a preopened temp dir: preopens, `stat-at`
  (`{ tag: 'regular-file' }`, size, timestamps), `open-at`/`stat`/drop, `create-directory-at` twice (`{ tag: 'exist' }`),
  `remove-directory-at`. `cargo test` passes.
- 2026-10-01: slice 3 done. `ir::AbiSig::blocking` (set for a command's WASI imports); `component::function` allows
  streams/futures in a sync function of the WASI world; future *arguments* stay unsupported. Backend (`abi.rs`):
  `BlockOn(h)` (a fresh waitable set, `[waitable-set-wait]`, `[waitable-set-drop]`: new host imports only in commands
  with such imports), `FutureLiftSync`/`StreamReadSync` (the async helpers' read, then `BlockOn` if BLOCKED, the same
  `…Done` lifting into a promise, then its settled value: a rejection is thrown and marked handled),
  `StreamWriteSync` (lower the chunk, write until all written or the reader drops), `StreamLowerSync` (new stream,
  value deferred). `wit::Gen::blocking` picks them; the import adapter calls `$$StreamFlush` between the call and
  lifting its results. Prelude (`builtins/stream.js`): `$$StreamReadAll` (joins `Uint8Array` chunks; other typed
  arrays not yet), `$$StreamDefer`, `$$StreamFlush` (state made on first use: the prelude's top-level values must
  be literals in libraries); the async reader/pump and the blocking functions are separate prelude extras (the
  async ones use a library's globals: invalid in a command). Pitfalls hit: prelude functions without a result
  (`drop` only when there is one). Fixture `wasi/streams.js` in `commands_import_wasi_interfaces`: read a file,
  write 200,000 bytes, read back, `read-directory`, `unlink-file-at`.
- 2026-10-01: slice 3 gating: `cmp.sh` 237 same, 0 different.
- 2026-10-01: slice 4: `crates/frontend/node/fs.js` (sync API, node's errors from `error-code`, `Stats`, `Dirent`, an
  fd table from 3, `constants`), `node/wasi_fs.mjs` (`node:internal/wasi_fs`, an ES module: `node:internal/…`
  built-ins are parsed as ES modules, the others stay CommonJS; it imports the WASI functions under `$` aliases and
  re-binds them with `export const`: `export { x }` of an imported WIT binding gives nothing to the namespace, a
  linker gap → follow-up). `diff.sh`: `x.fs/` is copied fresh per run, node runs in its copy, wasmtime preopens its
  copy as `/`. Fixture `node/fs.js` (+ `fs.fs/`): whole files, flags (`wx`, `a`), encodings, copy (+ EXCL), stat/
  lstat/exists/access, readdir (`withFileTypes`, `recursive`), mkdir (`recursive` result), rmdir/rm (`recursive`,
  `force`, node's `ERR_FS_EISDIR`), rename, symlink/readlink/realpath, unlink, fds (open/read/write/fstat/ftruncate/
  fsync/close, EBADF), truncate: **equal to node on the first full run**. All 14 `node/` fixtures pass. The build
  takes ~6 s (debug compiler; `buffer` is most of it); `fs.js` no longer requires `util` (only for messages).
- 2026-10-01: final checks: `cargo test` passes (`module_errors_are_diagnostics` used `node:fs` as the unimplemented built-in: now `node:net`); `scripts/diff.sh` (all fixtures): 197 pass, 0 fail, 19 skipped (3 `wasi/`); `cmp.sh`: 237 same, 0 different (the 2 new `fs` fixtures don't build with HEAD). Done.
