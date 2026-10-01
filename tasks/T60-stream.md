# T60: node's `stream` module (F238)

Status: done (2026-10-01) | Depends on: T55 (built-in modules: `events`, `util`,
`process`), T56 (`Buffer`, `string_decoder`), T59 (`fs`)

## Goal
Programs and packages that use `stream` behave like node 24.21.0:
1. `require("stream")` / `node:stream`: `Readable`, `Writable`, `Duplex`, `Transform`, `PassThrough`, `Stream`
   (legacy, `.pipe`), `pipeline`, `finished`, `addAbortSignal`?, `Readable.from`, async iteration of a
   `Readable`, object mode, `highWaterMark`/backpressure, `'data'`/`'end'`/`'error'`/`'close'`/`'finish'` events in
   node's order (they ride `process.nextTick` and microtasks: compare with node, not by inspection).
2. `stream/promises` (`pipeline`, `finished`).
3. `fs.createReadStream` / `fs.createWriteStream` over T59's `fs` (F252), so `graceful-fs` and `fs-extra` build.
Programs that don't use `stream` keep their bytes (it's a built-in module: only loaded when required).

## What we know (at handoff)
- node's own `stream` lists: `Readable Writable Duplex Transform PassThrough duplexPair pipeline addAbortSignal
  finished destroy compose setDefaultHighWaterMark getDefaultHighWaterMark promises Stream isDestroyed isDisturbed
  isErrored isReadable isWritable` (+ `_isUint8Array`, `_uint8ArrayToBuffer`, `_isArrayBufferView`).
- Candidate source: the **`readable-stream`** npm package (v4.7.0, MIT): node's `lib/internal/streams` as a package,
  what browserify/webpack ship. Its deps: `buffer`, `events`, `process`, `string_decoder` (js2wasi3 has all as
  built-ins: map them to `node:x`) and `abort-controller` (→ `event-target-shim`). js2wasi3 has **no
  `AbortController`/`AbortSignal`/`EventTarget`** (F241) and **no `WeakMap`** (F98; `event-target-shim` uses one).
  Options: a minimal `AbortController`/`EventTarget` in the prelude or a built-in; or strip `addAbortSignal`/`signal`
  support from the copy. Decide after trying to build `readable-stream` as is (T56 did the same with `buffer`:
  copy into `crates/frontend/node/`, then patch to node 24's behaviour where fixtures show a difference).
- Alternative: node's `lib/internal/streams/*.js` directly (uses `primordials` and internal bindings: more
  rewriting). Recommendation at handoff: `readable-stream`.
- Size matters: built-in modules are compiled with the program (T55/T56: `buffer` makes a build ~4.5 s with the debug
  compiler); T57/T58 fixed wasmtime's superlinear compile cases, but check `wasmtime compile` time of a stream program
  (T57's `measure.sh` method: `WASMTIME_LOG=wasmtime_internal_cranelift=debug`, `translated to CLIF` lines).
- `fs` (T59): `createReadStream`/`createWriteStream` throw today; `graceful-fs/legacy-streams.js` requires `stream`
  at its top (so `fs-extra` fails at build time). `readSync` reads the rest of the file per call (F252): a read
  stream over it should read the file once (or chunk with `highWaterMark` and fix F252 first).
- zigbee2mqtt (docs/zigbee2mqtt-feasibility.md): `stream` in 34 modules, mostly through npm packages that bring
  their own `readable-stream`; those must build too (a package's `readable-stream` is compiled from `node_modules`
  like any CommonJS package).

## Design questions (decide first, record in docs/design.md D4)
1. Source: `readable-stream` copied as built-in(s) (one file or its directory structure as `node:internal/streams/…`
   CommonJS files?) vs node's sources.
2. `AbortController`/`AbortSignal`/`EventTarget`: implement a minimal global (node has them as globals) or drop
   signal support.
3. Missing language/runtime features it hits (`WeakMap`? `Symbol.asyncIterator` on classes, `queueMicrotask`,
   `process.nextTick` order): fix in the compiler (record as follow-ups if large).
4. `fs.createReadStream`: chunked reads need `read-via-stream` dropped early or offset reads (F252).

## Plan (slices; each ends with fixtures)
1. Try to build `readable-stream` 4.7.0 with js2wasi3 (scratchpad): list what breaks; decide 1–3.
2. `node:stream` + `stream/promises` as built-ins; fixtures `tests/fixtures/node/stream*.js` (Readable/Writable/
   Transform/pipe/pipeline/async iteration/object mode/errors), equal to node.
3. `fs.createReadStream`/`createWriteStream` (`x.fs/` fixtures, T59), `graceful-fs` + `fs-extra` probe (T59's
   `probe.cjs`: `outputFileSync`, `writeJsonSync`, `copySync`, `removeSync`), gating, docs, follow-ups.

## Read first
- `tasks/T56-buffer.md` (how an npm package became a built-in), `tasks/T59-fs.md` (Notes / Result, F252).
- `crates/frontend/src/node.rs` (MODULES, `exports`), `crates/frontend/node/events.js`, `node/fs.js`.
- `docs/design.md` D4 (T55, T56, T59 notes).

## Tests
`scripts/diff.sh` on `tests/fixtures/node` (`x.fs/` for fs streams); `cargo test`; `wasm-tools validate` + wasmtime;
gating (scratchpad `cmp.sh`, see T59 Progress: HEAD's compiler vs the new one on every fixture).

## Acceptance
- [x] `stream` and `stream/promises` behave like node for the covered API (fixtures, event order included).
- [x] `fs.createReadStream`/`createWriteStream` work; `fs-extra` builds and its sync API matches node (async too).
- [x] Programs without `stream` compile to the same bytes (except the deliberate changes listed in Notes).

## Notes / Result
**Decisions** (design.md D4): (1) node 24.21.0's own stream code, not readable-stream: readable-stream is node 18's
and already differed from the oracle (chunk-type messages, the order of `finished` vs `pipeline` callbacks, a broken
`ERR_MISSING_ARGS`). `scripts/gen-node-streams.mjs` copies `stream`, `stream/promises`, `_stream_*`,
`internal/streams/*`, `internal/fs/streams` from `process.binding("natives")` into `crates/frontend/node/stream/`,
rewriting `primordials` and `require`s, mapping node internals to support modules, applying checked patches.
(2) No `abort-controller`: `node/abort.js` (minimal `Event`/`EventTarget`/`AbortSignal`/`AbortController`), not
globals yet (F241). (3) Compiler gaps hit were fixed (below); `WeakMap` wasn't needed by the built-in (F98 → T61).
(4) `readSync` keeps the bytes read to the end per fd (dropped after any `fs` write), so a read stream reads once.

**What changed.** Built-ins: `stream`, `stream/promises`, `_stream_*`, `node:internal/{abort,fs/streams,fs_support,
errors,primordials,util,util/inspect,validators,streams/*}`; `fs.createReadStream`/`createWriteStream`, lazy
`ReadStream`/`WriteStream`/`FileReadStream`/`FileWriteStream`, `opendir`/`opendirSync`/`promises.opendir`/`Dir`.
Frontend: `packages::is_builtin` is node's exact list; `node::is_esm`; `node::exports` for `node:stream`, new `fs`
names; `frontend::fn_assign` (assigned function declarations). Lowering/backend: `ir::Module::cjs_main` (ticks
before microtasks after a CommonJS entry); `Function.prototype[Symbol.hasInstance]` and `Object.defineProperties`
in the prelude; gated built-ins left off the global object when off (fixes invalid Wasm for e.g. `require("util");
globalThis.x = 1`, also at HEAD); DOMException codes for `AbortError`/`TimeoutError`. Tests: three stale `link.rs`
unit tests (they still expected `fs` unsupported since T59) now use `zlib`, plus `process/`.

**Commands run.** `scripts/diff.sh` on `node/stream-{errors,async,flow}.js`, `node/fs-streams.js`,
`node/fs-opendir.js`, `commonjs/{stream,next-tick}` (new) and every fixture whose bytes changed: all equal node;
`cargo test` all pass; `wasm-tools validate` + wasmtime on each; gating (`cmp.sh`, HEAD vs this, 312 fixtures):
6 new fixtures, 12 byte changes, all expected (DOMException `code` getter: structured-clone, atob, buffer×2,
string-decoder; naming `hasInstance`: symbols, symbol-value; CommonJS entries with ticks: commonjs/builtins,
next-tick; `fs.js`: fs, fs-async). test262: the 972 baseline non-passing tests rerun locally: 7 unsupported → pass,
1 unsupported → fail (`typeof/get-value.js`: top-level `this` in the runner's module mode), 0 regressions; full subset
run on `a-03`: 27,834 of 28,799 run pass (96.6%), the same 8 changes, 0 regressions (HISTORY.md, baseline updated). Packages: `fs-extra` 11.4.1 (with `graceful-fs`, `jsonfile`)
equals node; npm `readable-stream` in a package needs `WeakMap` (T61). Sizes: a stream program 961 KB, release
build 4.4 s, `wasmtime compile` 1.9 s, run 38 ms (F255).

**Unsupported** (F254–F258, updates to F31, F45, F98, F241, F252, F253): web streams and `stream/consumers`,
`AsyncResource`, Blob bodies; `FileHandle` as a stream's `fd`; `AsyncFunction` as a value; assigned function
declarations in blocks/exported; gated built-ins absent from the global object unless named.

**Next smallest step:** T61: `WeakMap`/`WeakSet` (F98) and the `AbortController`/`EventTarget` globals (F241, from
`node/abort.js`), then a package bundling npm `readable-stream` (zigbee2mqtt's) should build.

## Checklist
- [x] probe + design questions decided and recorded
- [x] `stream`, `stream/promises`
- [x] fs streams, packages
- [x] docs (design.md, keywords.md, README), follow-ups 

## Progress
- 2026-10-01: handoff written after T59 (`main` at `f3880d8`).
- 2026-10-01: started. `readable-stream` 4.7.0 from npm in the scratchpad (`rs/`); `ours/index.js` (its main)
  does `require('stream')` itself, so a package's own `readable-stream` needs our `node:stream`; the built-in is built
  from `lib/stream.js`. First build: `require('process/')`/`string_decoder/` were taken for built-ins: fixed
  `packages::is_builtin` to node's exact `builtinModules` list (`process/` is the npm package in node).
- 2026-10-01: next blockers: `WeakMap` (`abort-controller`, `event-target-shim`), `Object.defineProperties`
  (`primordials.js`, `errors.js`), `console.assert`. Build ~10 s with the debug compiler before failing.
- 2026-10-01: decisions (to record in design.md D4): (1) `readable-stream` 4.7.0's `lib/` copied to
  `crates/frontend/node/stream/` as CommonJS built-ins `stream`, `stream/promises`, `_stream_*`,
  `node:internal/streams/*`, `node:internal/{errors,primordials,util,util/inspect,validators}` (one-off script
  `copy-stream.mjs` in the scratchpad: requires made absolute `node:` names; relative ones can't normalize under
  `node:`); `node::is_esm` (only `internal/wasi_fs` is ESM now). (2) `node:internal/abort` (`node/abort.js`):
  minimal `Event`/`EventTarget`/`AbortSignal`/`AbortController` replace `abort-controller` (so no `WeakMap`
  needed by the built-in; WeakMap stays F98, the globals F241). Patches: no `globalThis.X ||`, no `Blob`, the
  global `AggregateError`, `isAsyncFunction` by builtin tag (no `AsyncFunction` constructor value: reading it trapped),
  `addAbortListener` the fallback. Added `Object.defineProperties` (prelude, object.js); DOMException codes for
  AbortError/TimeoutError. t1 (Readable.from → Transform → Writable, pipeline) builds (856 KB, ~10 s debug build,
  ~1 s run) and fails with `Cannot read properties of undefined (reading 'call')`.
- 2026-10-01: `Function.prototype[Symbol.hasInstance]` was missing (primordials' `FunctionPrototypeSymbolHasInstance`,
  used by every `Writable`): added `$$Function_prototype_$hasInstance` (object.js; linked by naming `hasInstance`,
  read-only). t1 and a Readable/Writable event-order probe now equal node. Next: fixtures.
- 2026-10-01: a CommonJS entry ran microtasks before the `nextTick` queue after its code (node: ticks first):
  `ir::Module::cjs_main` (the linker's `$$cjsMain(` call) makes `host.rs` run `run_ticks` first. node 24 changes:
  default byte `highWaterMark` 64 KiB, `duplexPair` (`streams/duplexpair.js`, node 24's), `_isArrayBufferView`.
  node's own lib sources: `process.binding("natives")["internal/streams/…"]`. Fixture `node/stream.js` equals node.
- 2026-10-01: **design Q1 changed**: readable-stream (node 18) differed from node 24 (chunk-type messages, event order of
  `finished` vs `pipeline`, `ERR_MISSING_ARGS` broken in readable-stream itself). Now node 24.21.0's own
  `lib/internal/streams/*`, `stream`, `stream/promises`, `_stream_*` are generated by `scripts/gen-node-streams.mjs`
  from `process.binding("natives")` (primordials → `require("node:internal/primordials")`, node internals mapped to
  support modules, patches: no `AsyncResource`, `FastBuffer` → `Buffer.from`, no `err.stack` reads (trap, F31),
  inline assert). Support modules stay readable-stream's (MIT) `errors`/`primordials`/`validators`/`util`/`util/inspect`
  plus `node/abort.js`; added the codes and primordials node 24 uses, node 24's `ERR_MISSING_ARGS`. Fixtures
  `node/stream.js`, `node/stream-errors.js` equal node.
- 2026-10-01: fixtures `node/stream-{errors,async,flow}.js` (ES modules: `tests/fixtures` is `"type": "module"`),
  `commonjs/stream/main.js` (the CommonJS event-order one), `commonjs/next-tick/main.js`; `node::exports` lists
  `node:stream`'s names. All equal node (`diff.sh`). Sizes: a stream program 961 KB; release build 4.4 s (debug
  11.5 s; a Buffer+events program: 1.6 s / 375 KB, so about linear in size), `wasmtime compile` 1.9 s, run 38 ms.
- 2026-10-01: fs streams: `internal/fs/streams` (node 24's) generated too (no `internalBinding`: O_* constants, no
  Windows handles), `node/fs_support.js` (node's `copyObject`/`getOptions`/`getValidatedFd`/`validatePath`, no URL,
  `FileHandle` as `fd` unsupported); `fs.createReadStream`/`createWriteStream` + lazy `ReadStream`/`WriteStream`/
  `FileReadStream`/`FileWriteStream` accessors. F252's re-reading: `readSync` keeps the bytes read to the end per fd
  (dropped after any write/truncate through `fs`), so a read stream reads the file once. The generator checks
  primordials. Fixture `node/fs-streams.js` (+ `.fs/`, a 200 KB file) equals node.
- 2026-10-01: `fs-extra` probe: graceful-fs assigns to function declarations (`ReadStream = …`): new source rewrite
  `frontend::fn_assign` (after `using`'s round): an assigned declaration directly in a function body/top level is
  renamed `f$fn` and `var f = f$fn; Object.defineProperty(f$fn, "name", …)` goes after the scope's directives
  (exported ones stay unsupported). Then invalid Wasm: `builtin_constructor` of `BigInt64Array` reads an i32 global;
  reproducer `require("util"); globalThis.x = 1` (another invalid-Wasm error); checking HEAD.
- 2026-10-01: invalid Wasm was pre-existing (HEAD too): with a global object, `GLOBAL_NAMES` named only as
  property names/strings (`node:util`'s "BigInt64Array") became properties although their runtime support was gated
  off; now `BigInt`, the BigInt arrays, `SuppressedError` and the stacks are left off then (`builtins.rs`).
  `fs.opendir`/`opendirSync`/`promises.opendir`/`Dir` (entries read at open; fixture `node/fs-opendir.js`).
  `fs-extra` 11.4.1 probe (outputFile/JSON/copy/ensure/move/emptyDir/remove, sync + async) equals node (1.66 MB).
  npm `readable-stream` in a package still fails: `WeakMap` (F98), `console.assert` (`event-target-shim`).
- 2026-10-01: full test262 subset on `a-03` (866 s): 27,834 of 28,799 run pass (96.6%), 7 unsupported → pass, 1 → fail, 0 regressions; baseline and HISTORY.md updated.
