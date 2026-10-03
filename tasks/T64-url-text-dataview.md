# T64: `URL`/`URLSearchParams`, `TextEncoder`/`TextDecoder`, `DataView` (F266)

Status: done (2026-10-02) | Depends on: T63

## Goal
The web globals zigbee2mqtt needs next, each matching node 24.21.0 on its own fixture:
1. WHATWG `URL` and `URLSearchParams` (parsing, serializing, the getters/setters, `searchParams`, `toJSON`,
   `URL.canParse`) and node's `url` module (`parse`, `format`, `resolve`, `fileURLToPath`, `pathToFileURL`,
   `urlToHttpOptions`, `URL`/`URLSearchParams` re-exported).
2. `TextEncoder`/`TextDecoder` (UTF-8; `fatal`, `ignoreBOM`, `stream`; `encodeInto`).
3. `DataView` (every get/set method, both endiannesses, BigInt ones, bounds errors).
Programs that don't name them keep their bytes.

## What we know (at handoff)
- T62/T63 counts (docs/zigbee2mqtt-feasibility.md): `URL` 21 sites (herdsman's `tcp://` path check runs at
  start: with T63's rule the graph builds without stubs but stops at `Invalid TCP path`), `DataView` 5,
  `TextDecoder` 3, `TextEncoder` 2 (fflate at load, a device module). `tasks/T63-files/build-run.sh` with
  `T63_GLOBALS=0` is the probe without the `web.js` stubs.
- node globals are bound by the linker from built-in modules (`crates/frontend/src/node.rs` `GLOBALS`, T56/T61:
  `Buffer`, `WeakMap`, `AbortController`…): `URL`/`URLSearchParams` from `node:url`, `TextEncoder`/`TextDecoder`
  from `node:util` (node has them there too) or an internal module. Then `lower` `NODE_GLOBALS` stops treating
  them as lacked.
- `DataView` is an ECMAScript built-in: typed arrays are native (`crates/backend/src/typed.rs`,
  `crates/lower/builtins/typedarray.js`); a `DataView` could be a prelude class over the buffer's bytes
  (`__intrinsic_typed_*`) or a native kind next to the typed arrays.
- node's `url` is JS over ada (C++); the WHATWG parser must be written (or ported: e.g. `whatwg-url`, MIT, is
  large; a compact parser for special schemes + opaque paths may be enough). Check what the T62 sites need
  (`tcp://host:port`, `mqtt://`, `ws://`, `file:`).

## Plan
1. `DataView` (smallest, self-contained), fixture `builtins/dataview.js`; test262 `built-ins/DataView`.
2. `TextEncoder`/`TextDecoder`, fixture `builtins/text-coding.js`.
3. `URL`/`URLSearchParams` + `node:url`, fixtures `node/url.js`, `node/url-search-params.js`; the WPT-derived cases
   node itself tests are a good source of inputs.
4. Probe: `T63_GLOBALS=0 tasks/T63-files/build-run.sh` reaches `Opening TCP socket`.

## Read first
- `tasks/T63-small-gaps.md` (Notes / Result), `tasks/T61-weakmap-abort.md` (globals bound by the linker),
  `tasks/T56-buffer.md` (a node module over typed arrays), `docs/zigbee2mqtt-feasibility.md` (Survey 2026-10-01).

## Tests
Each fixture through `scripts/diff.sh`; `cargo test`; test262 `built-ins/DataView` through `scripts/test262.mjs`.

## Acceptance
- [x] Items 1–3 equal node on their fixtures; programs without them keep their bytes, except those using `util.inspect.custom` (the `Reindent` fix), `ArrayBuffer.isView` or `node:util` (see Notes / Result).
- [x] The zigbee2mqtt probe runs to `Opening TCP socket` without the `web.js` stubs.
- [x] Follow-ups updated.

## Notes / Result
**What changed**
- `DataView` (`crates/frontend/node/dataview.js`, bound as a global by the linker like T61's `WeakMap`): a class
  over a `Uint8Array` of its bytes and an 8-byte scratch buffer; every get/set method (BigInt and Float16 too),
  V8's messages, views of resizable buffers track their length and a detached or out-of-bounds view throws V8's
  TypeError (prelude `$$bufferState`/`$$bufferResizable`: node modules can't use `__intrinsic_rab_mode()`).
  `ArrayBuffer.isView` and `util.types.isDataView`/`isArrayBufferView` know it (`$$setDataViewCheck`).
- `TextEncoder`/`TextDecoder` (`crates/frontend/node/encoding.js`): UTF-8 with the WHATWG decoder (maximal
  subparts, `fatal`, `ignoreBOM`, `stream`), UTF-16LE/BE, windows-1252 (`latin1`, `ascii`), `encodeInto`, node's
  errors and printing.
- `URL`/`URLSearchParams` and node's `url`/`querystring`: node 24.21.0's `internal/url.js`, `url.js`,
  `querystring.js`, `internal/querystring.js` ported by `tasks/T64-files/port-url.py` (from the files at
  `raw.githubusercontent.com/nodejs/node/v24.21.0/lib/…`; primordials as local functions, node's errors local,
  `URLPattern`/object URLs left out). The C++ binding they call (ada) is `crates/frontend/node/url_binding.js`, a
  WHATWG URL parser written from the URL Standard: parse to a record, serialize to `href` + ada's component
  offsets, setters as the basic parser with a state override, IPv4/IPv6/opaque hosts, Punycode.
- Backend `inspect.rs` `Reindent`: a `[util.inspect.custom]` method's string result is indented when it's nested
  (node does the same); `DataView { …, [buffer]: ArrayBuffer {…} }` needed it.
- design.md (D4's T61 paragraph), README, keywords.md, zigbee2mqtt-feasibility.md.

**Commands run**: `scripts/diff.sh` on the new fixtures (`builtins/dataview.js`, `builtins/dataview-resizable.js`,
`builtins/text-coding.js`, `node/url.js`, `node/url-search-params.js`, `node/url-legacy.js`, `node/querystring.js`):
all equal node; the URL modules were first checked under node itself (a harness loading them as CommonJS) before
compiling. `cargo test --no-fail-fast`: all pass except the 4 `*_compose_with_wac` (wac isn't installed here).
Bytes (every command fixture built by HEAD and by this tree): the 22 that differ all use `util.inspect.custom`
(every `Buffer`/`stream`/`util`/`WeakMap` user: the `Reindent` call), `ArrayBuffer.isView` (the `DataView` hook)
or `node:util` (its `isDataView`), or name `URL`/`TextEncoder` (`globals/guarded.js`); all of them still equal node.
test262: `built-ins/DataView` 426 of 521 run pass (78 of the failures use the runner's missing `$262.detachArrayBuffer`, 11 `transferToImmutable`, which node 24 lacks too), `ArrayBuffer/isView` 17 of 17; with T63's directories 9,299 of 9,488 run, 0 regressions (baseline updated).

**zigbee2mqtt probe** (`tasks/T64-files/`: T63's patch without the `url` stub; `T63_GLOBALS=0`, so no `web.js`
bindings either; release build): builds in 80 s, 32.3 MB, and runs to `Opening TCP socket with 127.0.0.1:6638`,
then the T62 `net.Socket` stub, as with the stubs.

**Unsupported / deferred**
- `DataView`: called without `new` gives the class's message; every `DataView` program is in BigInt mode;
  no `SharedArrayBuffer`. F278.
- URL: UTS #46's mapping table and validity checks for international domain names (lower-casing, full-width forms,
  NFC, Punycode only), `URLPattern`, `URL.createObjectURL`; `util.TextEncoder`/`TextDecoder` still throw; other
  `TextDecoder` encodings; `{ windows: true }` file-URL options (F243). F279.
- The URL fixtures take ~6 s to build in a debug build (the url modules are ~3k lines): F255.

**Next smallest step**: T65 ([T65-load-only-builtins-crypto.md](T65-load-only-builtins-crypto.md)): node built-ins
zigbee2mqtt only needs to load (`child_process`, `dns`, `dgram`, `tls`, `http(s)`, `http2`, `zlib`, `perf_hooks`,
`module`) and a `crypto` subset (F267).

## Checklist
- [x] `DataView`
- [x] `TextEncoder`/`TextDecoder`
- [x] `URL`/`URLSearchParams`, `node:url`
- [x] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T63.
- 2026-10-02: started. `DataView` is a built-in module `crates/frontend/node/dataview.js` (a class over a
  `Uint8Array` view and an 8-byte scratch buffer; all getters/setters incl. BigInt and Float16), bound as a
  global through `GLOBALS`. `ArrayBuffer.isView` asks it through `$$setDataViewCheck`; `util.types.isDataView`
  and `isArrayBufferView` know it. Fixture `builtins/dataview.js` passes `scripts/diff.sh`.
- 2026-10-02: backend `Helper::Reindent`: a `[util.inspect.custom]` string result is re-indented when nested
  (node does the same); needed for `DataView { …, [buffer]: ArrayBuffer {…} }`.
- 2026-10-02: `TextEncoder`/`TextDecoder` in `crates/frontend/node/encoding.js` (utf-8 with WHATWG
  maximal-subpart replacement and streaming, utf-16le/be, windows-1252; `fatal`, `ignoreBOM`, node's error
  codes). Fixture `builtins/text-coding.js` passes. `util.TextEncoder`/`util.TextDecoder` stay unimplemented.
- 2026-10-02: test262 for T63's directories still running in the background (release binary).
- 2026-10-02: test262 for T63's directories (delete, typeof, optional-chaining, class ×2, Promise/any, for,
  variable): 8852 pass / 96 fail / 76 unsupported, 12 changes since the baseline, 0 regressions (recorded in
  tests/test262/HISTORY.md).
- 2026-10-02: `URL`/`URLSearchParams`: the WHATWG parser written from the URL Standard
  (`crates/frontend/node/url_binding.js`: what node's ada binding gives, `href` + ada's component offsets;
  setters with state overrides; IPv4/IPv6/opaque hosts; IDNA reduced to lower-casing, full-width forms, NFC and
  Punycode). node's `internal/url.js`, `url.js`, `querystring.js`, `internal/querystring.js` ported by
  `tasks/T64-files/port-url.py` (primordials as local functions, node's errors local). Checked first under node
  with a harness that loads the modules, then compiled: fixtures `node/url.js`, `node/url-search-params.js`,
  `node/url-legacy.js`, `node/querystring.js` pass `scripts/diff.sh`.
- 2026-10-02: zigbee2mqtt probe with `T63_GLOBALS=0` (no `web.js` stubs): build 79.7 s, 32.3 MB, runs to
  `zh:zstack:znp: Opening TCP socket with 127.0.0.1:6638` (then the T62 `net.Socket` stub).
- 2026-10-02: `DataView` over resizable/detached buffers (length tracking, V8's TypeErrors) through prelude helpers;
  fixture `builtins/dataview-resizable.js`. Probe `tasks/T64-files/` drops the `url` stub too: same result.
  Bytes check and `cargo test` (see Notes / Result). Follow-ups F278, F279; T65 handoff.
