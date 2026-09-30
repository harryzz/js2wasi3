# T56: `Buffer` and node's `buffer` module

Status: done | Depends on: T20 (typed arrays), T47 (subclassing typed arrays), T55 (built-in modules), F238

## Goal
Programs and packages that use `Buffer` behave like node 24.21.0:
1. `require("buffer")`/`import { Buffer } from "node:buffer"` and the `Buffer` global (the same object), a
   `Uint8Array` subclass: `Buffer.from` (string with encodings, array, array-like, `ArrayBuffer` + offset/length,
   another buffer), `alloc`/`allocUnsafe`/`allocUnsafeSlow`, `concat`, `byteLength`, `isBuffer`, `isEncoding`,
   `compare`; `toString(encoding, start, end)`, `write`, `slice`/`subarray` (views, not copies), `equals`, `compare`,
   `indexOf`/`lastIndexOf`/`includes`, `fill`, `copy`, `toJSON`, the `read*`/`write*` integer and float methods
   (LE/BE, 8–48-bit, `BigInt64`), `swap16/32/64`, `buffer.constants`, `kMaxLength`, `SlowBuffer`?
2. Encodings: `utf8`, `utf16le`/`ucs2`, `latin1`/`binary`, `ascii`, `base64`, `base64url`, `hex`.
3. Printing like node: `<Buffer 68 69>` (node's `inspect` for buffers, 50 bytes max then `... n more bytes`).
4. `atob`/`btoa` (F237) come nearly free with base64.
Programs that use no `Buffer` compile to the same bytes.

## What we know (at handoff)
- Typed arrays are native (`crates/backend/src/typed.rs`, `builtins/typedarray.js`); `class X extends Uint8Array`
  works (T47), but `subarray`/`slice`/`map` don't use species (they return a plain `Uint8Array`), a subclass prints
  as `Uint8Array(n) [ … ]` (node: `X(n) [Uint8Array] [ … ]`), and there's no `DataView` (F116?).
- node's `Buffer` is `FastBuffer extends Uint8Array` with the methods on `Buffer.prototype`, and
  `Buffer.prototype.__proto__ === Uint8Array.prototype`; `Buffer(…)` without `new` is deprecated but works.
- Candidate sources: node's lib/buffer.js (pure JS over internal bindings for encoding, `byteLengthUtf8`, `compare`,
  `indexOf`, `fill`: those would be prelude intrinsics or JS) or feross/buffer (the npm polyfill browserify uses,
  MIT, self-contained, ES5 style). zigbee2mqtt counts: `buffer` 36 modules, `Buffer.` ~1,500 uses.
- T55: built-in modules are CommonJS files in `crates/frontend/node/`; globals backed by the prelude are made on
  first use (`process`), but `Buffer` would live in a built-in module.

## Design questions (decide first, record in docs/design.md)
1. **The `Buffer` global**: a module's value, not a prelude object. Options: the linker turns an unresolved `Buffer`
   reference into a binding of `node:buffer`'s `Buffer` (an implicit import, so only programs naming it load the
   module); or `Buffer` in the prelude with `node/buffer.js` re-exporting it. Recommendation at handoff: the
   linker's implicit binding.
2. **Source**: port node's lib/buffer.js with our own encoding helpers, or feross/buffer. Check each against node
   with fixtures, not by inspection.
3. **Species** for `subarray`/`slice` (node's `buf.subarray()` is a `Buffer`): general `TypedArraySpeciesCreate` in
   `typedarray.js` (gated like T47's subclassing), or `Buffer` overriding them.
4. **Speed**: UTF-8/base64/hex loops in JS vs native helpers (the string ↔ UTF-8 code exists for the WIT boundary).
5. **Printing**: node's `<Buffer …>` comes from `Buffer.prototype[util.inspect.custom]`; the formatter doesn't call
   custom inspectors (F240): a native case for buffers, or `inspect.custom` support.

## Plan (slices; each ends with fixtures)
1. Measurement + design; the global; `from`/`alloc`/`toString` with utf8/hex/base64/latin1; printing.
2. The rest of the static and prototype methods; read/write numbers; `atob`/`btoa`.
3. Species/subclass fixes in typed arrays if chosen; real-package probe (`safe-buffer`, `base64-js`, `ws` parts?),
   gating, targeted test262 (`built-ins/TypedArray*` if species changes), docs, follow-ups.

## Read first
- `tasks/T55-node-builtins.md`: Notes / Result; `crates/frontend/src/node.rs`; `link.rs` (`node::`, CommonJS table).
- `crates/lower/builtins/typedarray.js`, `crates/backend/src/typed.rs` (element access, printing).
- `docs/design.md` D4 (T55), D5 (T20/T47/T48 typed arrays).

## Tests
Fixtures `tests/fixtures/node/buffer*.js` (and a CommonJS case) via `scripts/diff.sh`; `cargo test`;
`wasm-tools validate` + wasmtime; gating (scratchpad `cmp.sh`: HEAD's compiler vs the new one on every fixture).

## Acceptance
- [x] `Buffer` (global and module) behaves like node for the covered API and encodings (fixtures).
- [x] Buffers print like node.
- [x] Unsupported members are clear errors with follow-ups.
- [x] Programs without `Buffer` compile to the same bytes.

## Notes / Result
**Decisions** (design.md D4, D5): (1) the `Buffer` global is the linker's: a CommonJS module with an unresolved
`Buffer` gets `const Buffer = require("node:buffer").Buffer;` in its wrapper, an ES module's references become
`node:buffer`'s export (a request); programs not naming it don't load it. (2) `node/buffer.js` is the `buffer` npm
package 6.0.3 (with `base64-js`, `ieee754` inlined, each in its own scope) brought to node 24's behaviour (errors,
argument handling, WHATWG UTF-8 decoding, `base64url`, exports). (3) No typed-array species: the port re-prototypes
plain `Uint8Array`s and overrides `subarray` (species: F248). (4) The encodings stay JS. (5) Printing through general
`util.inspect.custom` support (a prelude hook the formatter calls first), which also covers user classes.

**What changed**: `crates/frontend/node/{buffer,string_decoder}.js` (new); `link.rs` (`buffer_refs`/`buffer`,
`global_refs`, the wrapper line, reference rewriting; unit test `buffer_global`); prelude `builtins/base64.js`
(`atob`/`btoa`), `node.js` (`$$inspectCustom`), `clone.js` (`$$domException`); `Module::inspect_custom` and the
formatter's call (`inspect.rs`, only in programs with the string `"nodejs.util.inspect.custom"`); `source_text` skips
generated code (F246).

**Tests**: fixtures `node/{buffer,buffer-edges,string-decoder,atob,inspect-custom}.js`, `commonjs/builtins` (the
`Buffer` global in CommonJS): 14/14 node and CommonJS-builtins fixtures equal node; `cargo test` all pass; gating
(`cmp.sh`): identical except programs using `util` (its `inspect.custom` string), `structuredClone` (shared
`DOMException`), `modules/namespace` (names `toString`: the linker's getters keep no source now), all equal node
(`diff.sh` on `modules`, `commonjs`, `structured-clone.js`: 27/27). Real packages equal node: `safe-buffer`,
`buffer-crc32`, `iconv-lite` (needed `string_decoder`). Full test262 subset on `a-03`: 27,826 of 28,799 run pass (96.6%), no changes (1 timeout under load passes alone). Size: a Buffer program is ~360–420 KB, ~2.5 s in wasmtime.

**Unsupported / follow-ups**: F247 (Buffer leftovers), F248 (typed-array species, subclass printing, `DataView`);
F246 done; F237's `atob`/`btoa` and F240's `util.inspect.custom` done (their other items stay open); F238 goes on
(`buffer`, `string_decoder` done).

**Next smallest step**: `fs` on `wasi:filesystem` (F238), or `stream` (zigbee2mqtt: 34 modules; `readable-stream` is
pure JS over `events`, `buffer`, `string_decoder`, `process.nextTick`).

## Checklist
- [x] design questions decided and recorded
- [x] global + core (`from`/`alloc`/`toString`, encodings, printing)
- [x] the rest of the API, `atob`/`btoa`
- [x] fixtures, gating check, real-package probe
- [x] docs (design.md, keywords.md, README, HISTORY.md if run), follow-ups

## Progress
- 2026-09-30: handoff written after T55 (`main` at `7680c63`; test262 subset 27,826 of 28,799 run pass, 96.6%).
  Probe: `class FastBuffer extends Uint8Array` works; `subarray` returns a plain `Uint8Array`; a subclass prints as
  `Uint8Array(4) [ … ]`; `DataView` is missing.
- 2026-09-30: started (user chose Buffer next). Measurement: feross's `buffer` 6.0.3 (MIT) makes buffers by
  re-prototyping plain `Uint8Array`s (`Object.setPrototypeOf(arr, Buffer.prototype)`), so no species or subclass
  construction is needed; as a user package it hit `SharedArrayBuffer` and `arguments` in its error classes.
  Decision (Q2): port it to `crates/frontend/node/buffer.js` with `base64-js`/`ieee754` inlined (each in its own
  function scope: base64-js's `byteLength` would replace `Buffer.byteLength`), node's error objects from plain
  functions, no `TYPED_ARRAY_SUPPORT`/`SharedArrayBuffer` branches. A first probe (utf8/hex/base64/latin1,
  read/write ints and doubles, concat, compare, indexOf, toJSON, fill) equals node through `require("buffer")`.
  Decision (Q1): the global is the linker's: a CommonJS module with an unresolved `Buffer` gets
  `const Buffer = require("node:buffer").Buffer;` in its wrapper (loaded lazily, in node's order); an ES module's
  unresolved `Buffer` references become `node:buffer`'s `Buffer` export (a request, like an import).
- 2026-09-30: the global (linker: `Module::buffer_refs`/`buffer`, `global_refs`; CommonJS wrapper line, ES module
  references → `node:buffer`'s export): ESM, CommonJS and `typeof Buffer` probes equal node. Printing: general
  `util.inspect.custom` support (Q5): prelude `$$inspectCustom(v, depth)` (`builtins/node.js`, linked when a string
  in the code is `"nodejs.util.inspect.custom"`; `Module::inspect_custom`), called first by the formatter for every
  object (node's formatValue: a string result as is, another value formatted in place, prototypes skipped); user
  classes with `[util.inspect.custom]` equal node too. `atob`/`btoa` as prelude globals (`builtins/base64.js`, node's
  `InvalidCharacterError` DOMException through a shared `$$domException`, T53's DataCloneError now uses it).
  `buffer.js` brought to node's behaviour: node's error codes/messages (`ERR_INVALID_ARG_TYPE`, `ERR_OUT_OF_RANGE`,
  `ERR_BUFFER_OUT_OF_BOUNDS`, `ERR_UNKNOWN_ENCODING`), `from`'s order, `fromArrayBuffer`, `base64url`, node's base64
  `byteLength`, `indexOf("")`, `equals`/`compare`/`concat` with `Uint8Array`, `subarray` giving a Buffer, read/write
  offset checks (no `>>> 0` coercion), `kMaxLength` 2^53−1, `poolSize` 64 KiB, the module's exports in node's order
  (`isUtf8`, `isAscii`, `constants`, `kStringMaxLength`, `atob`/`btoa`; `transcode`, `Blob`, `File`,
  `resolveObjectURL` throw). A 45-line probe of every encoding and method family equals node.
- 2026-09-30: a second probe (read/write errors, defaults, `Buffer()` without `new`, invalid UTF-8, utf16le/ascii
  edge cases, `write`/`fill` argument forms, `from` of views/ArrayBuffers/JSON, `byteLength`, `concat`/`compare`
  errors, `copy`, `lastIndexOf`, printing) equals node after: a WHATWG UTF-8 decoder (one U+FFFD per maximal
  subpart), node's `write`/`fill` argument handling (`validateOffset`). Left: `buf.map`/`filter` give a
  `Uint8Array` (typed arrays ignore species: follow-up). F246 hit hard: that probe calls `.toString()`, so every
  function kept its source text, including `node:buffer`'s 66 KB module function: 807 KB, 32 s in wasmtime.
  `source_text` now skips generated code (node's built-in modules and every CommonJS module's wrapper function,
  whose text nothing can observe): 419 KB, 2.6 s; functions inside CommonJS modules keep theirs.
- 2026-09-30: `node/string_decoder.js` (node's former pure-JS decoder, `base64url`, state reset after `end()`, as
  node 24): `iconv-lite` then equals node, with `safe-buffer` and `buffer-crc32`. Fixtures
  `node/{buffer,buffer-edges,string-decoder,atob,inspect-custom}.js` and the `Buffer` global in
  `commonjs/builtins`: 14/14 pass; unit test `buffer_global`; `cargo test` passes; gating as in Notes. Docs and
  follow-ups (F247, F248; F246 ticked). Done.
- 2026-09-30: user approved a full test262 run on `a-03`, then commit + push.
- 2026-09-30: full test262 subset on `a-03`: 27,826 of 28,799 run pass (96.6%), 0 regressions (the load timeout
  `S13.2.1_A1_T1.js` passes alone). HISTORY.md updated.
