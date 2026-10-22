# T150: `zlib` in Rust, as a second helper component (F284)

Status: in progress | Depends on: T149 | Local; ask before long runs and before every push

## Goal
`zlib` today loads with constants, `codes` and `crc32`; every compression function and stream is refused (66 rows in the compat page).
Implement node's `zlib` (deflate/inflate/gzip/gunzip/deflateRaw/inflateRaw/unzip, then Brotli, then zstd) on a Rust helper component composed
with the program, as T149 did for `crypto`: `--ext-lib zlib`, text-scan trigger, `scripts/link-zlib.sh` (or one script for all helpers).

## Design (to confirm while working)
- **Port node 24's `lib/zlib.js`** (as T64 ported `url`) so the classes (`ZlibBase`, `Zlib`, `Deflate`, ..., options, `flush`, `params`, `reset`, `close`,
  `bytesWritten`, `_processChunk`, sync + callback + stream forms, errors with node's `code`/`errno`) behave like node's. Only the native binding
  (`internalBinding('zlib')`: `init`, `write`, `writeSync`, `params`, `reset`, `close`, `_setAsyncId`) is replaced by a JS class over the helper.
- **Helper interface** (`js2wasi3:zlib/streams@0.1.0`): a `stream` resource (constructor: mode, level, windowBits, memLevel, strategy, dictionary) and
  `process(flush, input, out-capacity) -> result<(consumed, output, status), error>`; this has the shape of zlib's `avail_in`/`avail_out`, so the
  binding's `writeSync(flush, in, in_off, in_len, out, out_off, out_len)` and its `_writeState` map one to one. `reset`, `params`, one-shot helpers.
- **Crate candidates** (check each builds for `wasm32-unknown-unknown`): `flate2` with the `zlib-rs` backend (zlib's messages, e.g. "incorrect header
  check", wanted for node's error text) or `miniz_oxide`; `brotli` (pure Rust); zstd: `zstd` needs a C compiler for wasm (clang has the wasm32 target
  here) or `ruzstd` (pure Rust, decoder complete, encoder limited).
- **Compressed bytes differ from node's** (node uses Chromium's zlib; zlib-rs and miniz are not byte-identical). Tests therefore compare round trips,
  node-compressed input decompressed by us (identical), our output decompressed by node, and sizes; error codes/messages compared with node.
- Import declared only when used (text scan for the zlib function names, as T149), all other programs byte-identical.

## Plan
- [x] Step 0: which crates build for wasm32-unknown-unknown and their sizes; messages of zlib-rs vs node for bad input.
- [x] Slice 1: deflate/inflate/gzip/gunzip/raw/unzip: one-shot (`*Sync`, callback) and streams; `crc32` stays.
- [ ] Slice 2: Brotli (`brotliCompress`/`Decompress`, streams, params).
- [ ] Slice 3: zstd (node 24's `zstdCompress`/`zstdDecompress`).
- [x] (slice 1) Fixtures against node (`tests/fixtures/node/zlib-*.js`), node's own `benchmark/zlib` files if present in v24.21.0, Express `compression` middleware.
- [x] (slice 1) Docs (README, design.md, TOOLS.md, CLAUDE.md, compat page regenerated, F284 ticked), gate (other programs byte-identical).

## Read first
- `tasks/T149-rust-crypto-component.md` (the whole mechanism), `crates/frontend/node/zlib.js`, `crates/frontend/node/crypto_kdf.js`,
  `tasks/T64-files/port-url.py` (how node's lib was ported), `crates/frontend/src/link.rs` (`link`, `ext_libs`).

## Notes / Result
Slice 1 done (2026-10-22); Brotli and zstd (slices 2 and 3) open, so the task stays `in progress`.
- Done: `deflate`/`inflate`/`gzip`/`gunzip`/`deflateRaw`/`inflateRaw`/`unzip` in every form (sync, callbacks, streams, `flush`, `params`, `reset`, `close`,
  dictionaries, `info`, `maxOutputLength`, `rejectGarbageAfterEnd`), node's own lib/zlib.js ported (`tasks/T150-files/port-zlib.py`), Rust `ZlibContext` on
  libz-rs-sys, helper 110 KB. `build` composes helpers (`compose.rs`, wac-graph 0.12); `link()` has an `EXT_LIBS` table; `--ext-lib crypto|zlib`.
- Tests: `tests/fixtures/node/zlib-sync.js` (~110 checks: node-made streams decoded here, round trips for every level/strategy/windowBits/memLevel,
  dictionaries, members, trailing zeros/garbage, truncation, corrupt data, option errors, callbacks) and `zlib-stream.js` (pipes, small chunks, flush,
  params, reset, events, errors, async iteration, promisify, backpressure) pass `scripts/diff.sh`; `crypto-kdf*.js` still pass after composing moved into
  `build`; 22 of 23 benchmark/fixture programs build byte-identical to before (the 23rd, `crypto.js`, differs only by T149's own change); Express example
  (+4%, the helper comes with `body-parser`) and Express + `compression` (gzip, deflate, identity bodies) answer exactly like node.
- Measured (laptop): gzip of 256 KB at level 6: 9 ms vs node 6; 1/2/4 MB inputs are 2.5–7× (gzip) and 15–30× (gunzip) node: F498.
- Not run: test262 (no non-zlib output changed), the a-03 gate (same), compat page regenerated (zlib rows).
- Next: slice 2 (Brotli: `brotli` crate, `BrotliEncoder/Decoder` binding with `params`, `BROTLI_OPERATION_*`), slice 3 (zstd: `zstd` crate with `CC_wasm32_unknown_unknown=clang`, `build-zlib-helper.sh` must set it; or `ruzstd`), then F498.

## Progress
- 2026-10-22: task written; user: "let start with zlib".
- 2026-10-22: step 0: a probe crate (scratchpad) builds for `wasm32-unknown-unknown` with no imports: `flate2` (`zlib-rs` backend, no default features), `brotli` 9.0.0, `ruzstd` 0.9.1 (decoder) and `zstd` 0.13 (C zstd 1.5.7 through `CC_wasm32_unknown_unknown=clang`, `zstd-sys` 2.1.2: compress + decompress round trip linked: 1.77 MB with everything, gzip+brotli+ruzstd 1.2 MB before size tuning). So all three formats are feasible with the real C zstd; clang is in `~/.local/share/swiftly/bin`. Open: zlib-rs's error texts vs node's; component sizes per format.
- 2026-10-22: design: port node's `lib/zlib.js` nearly unchanged (node's `internal/*` modules for streams already exist: primordials, errors, validators, util); replace `internalBinding('zlib')` by a JS binding (`Zlib`, `BrotliEncoder/Decoder`, `ZstdCompress/Decompress` classes with `init/write/writeSync/params/reset/close`, `_writeState = [availOutAfter, availInAfter]`, `onerror(message, errno, code)`) over one WIT resource `codec` (`create(mode, params, dictionary, reject-garbage)`, `process(flush, input, out-capacity) -> {consumed, output}`, `params`, `reset`). The Rust side ports node_zlib.cc's `ZlibContext` (mode switch, UNZIP header peeking, multi-member gunzip, dictionary retry, error texts/codes) on `libz-rs-sys` (zlib's C API in Rust: `deflateInit2_`, `inflate`, `strm.msg` = zlib's messages), then Brotli and zstd contexts. Separate crate `crates/zlib-helper`, WIT `js2wasi3:zlib/codecs@0.1.0`, `--ext-lib zlib`.
- 2026-10-22: user decision: **`js2wasi3 build` composes the helpers itself** (new compiler dependency `wac-graph`, helper `.wasm` files embedded with `include_bytes!`), because zlib is reached by programs that never compress (Express's body-parser) and a manual step would break them. The T149 crypto helper moves to the same mechanism (`scripts/link-crypto.sh` stays as a thin wrapper / is dropped once build composes). `--ext-lib` keeps its meaning (force). Helper crate `crates/zlib-helper` builds (110 KB component, node's `ZlibContext` ported on `libz-rs-sys`).
- 2026-10-22: `crates/js2wasi3/src/compose.rs` (`wac-graph` 0.12 + `wac-types` 0.12; they use wasm crates 0.258 next to our 0.259): `build` plugs in every helper (crypto, zlib) whose interface the program's component imports; checked: `crypto-kdf.js` builds to one 872 KB component that runs and validates. `scripts/link-crypto.sh` removed; `diff.sh`, `compat.mjs`, `bench-crypto.sh` no longer link. Still to do: re-run crypto fixtures, docs mention (T149 file notes say script), TOOLS.md (wac-graph).
- 2026-10-22: slice 1 core works: `crates/zlib-helper` (node's `ZlibContext` on `libz-rs-sys` 0.6.8, 110 KB component), `internal/zlib_helper` (ES wrapper), `internal/zlib_binding` (handles: init/write/writeSync/params/reset/close; Brotli/zstd classes throw "not implemented yet"), `internal/zlib_impl` = node 24.21.0 `lib/zlib.js` ported by `tasks/T150-files/port-zlib.py`; `link()` generalised to an `EXT_LIBS` table (crypto, zlib; zlib trigger: a non-built-in module containing `zlib`); `--ext-lib zlib`. First run (deflate/inflate/gzip/gunzip/unzip/raw sync + callbacks, bad-header error `Z_DATA_ERROR -3 incorrect header check`) equals node; compressed sizes differ (zlib-rs vs Chromium's zlib: deflate 39 vs 35 bytes, gzip 51 vs 47). Next: fixtures against node, docs, byte-identical check.
