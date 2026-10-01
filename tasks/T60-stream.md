# T60: node's `stream` module (F238)

Status: todo (handoff written 2026-10-01, not started) | Depends on: T55 (built-in modules: `events`, `util`,
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
- [ ] `stream` and `stream/promises` behave like node for the covered API (fixtures, event order included).
- [ ] `fs.createReadStream`/`createWriteStream` work; `fs-extra` builds and its sync API matches node.
- [ ] Programs without `stream` compile to the same bytes.

## Notes / Result
(to be written)

## Checklist
- [ ] probe + design questions decided and recorded
- [ ] `stream`, `stream/promises`
- [ ] fs streams, packages
- [ ] docs (design.md, keywords.md, README), follow-ups

## Progress
- 2026-10-01: handoff written after T59 (`main` at `f3880d8`).
