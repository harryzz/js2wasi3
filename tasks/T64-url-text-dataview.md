# T64: `URL`/`URLSearchParams`, `TextEncoder`/`TextDecoder`, `DataView` (F266)

Status: not started | Depends on: T63

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
- [ ] Items 1–3 equal node on their fixtures; programs without them keep their bytes.
- [ ] The zigbee2mqtt probe runs to `Opening TCP socket` without the `web.js` stubs.
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] `DataView`
- [ ] `TextEncoder`/`TextDecoder`
- [ ] `URL`/`URLSearchParams`, `node:url`
- [ ] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T63.
