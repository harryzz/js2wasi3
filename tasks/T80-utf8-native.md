# T80: native UTF-8 encoding in `node:buffer` (F348)

Status: done (2026-10-05) | Depends on: T79 (profile) | Cloud batch: yes (`tasks/CLOUD-BATCH.md`)

## Goal
`Buffer.byteLength(s)`, `buf.write(s)` / `Buffer.from(s)` with `utf8` (the default) and the decoding side
(`buf.toString()`, `utf8Slice`, F339) without JS arrays of bytes: native helpers in the runtime, like T78's latin1
path. Same results as node, byte for byte, for every string (unpaired surrogates, 4-byte characters, partial writes).

## Why (T79, measured on a replay of a recorded production session; don't re-measure here)
On zigbee2mqtt's real workload, UTF-8 *encoding* in `node:buffer`'s JS is ~36% of js2wasi3's CPU (at a 1 GiB GC heap):
`Buffer.byteLength` 20.6% and `utf8Write` 15.1% inclusive, both building a JS array with one element per byte
(`utf8ToBytes`, 26.8%); callers: `Buffer.from(string)`, readable-stream's `decodeChunk`, socket writes (MQTT
publishes, their echoes, log lines). It's also much of the garbage the copying GC copies (F304). node does it natively.

## Read first
- `crates/frontend/node/buffer.js`: `byteLength` (~line 388), `utf8Write` (~829), `utf8Slice` (~927), `utf8ToBytes`
  (~1906), `byteSlice` (~1022: T78's native latin1 call `__intrinsic_typed_latin1(buf, start, end, mask)`).
- T78's latin1 op end to end (the pattern to copy): intrinsic name → `TypedOp::Latin1` in `crates/lower/src/lib.rs`
  (~8554), its type and name in `crates/ir/src/lib.rs` (~1550, ~1770), emitted in `crates/backend/src/typed.rs` (~777);
  `__intrinsic_typed_kind` (which typed arrays qualify).
- `docs/design.md` D1 (strings are `$string`: an `i16` array of UTF-16 code units).
- `tasks/README.md` F339, F348.

## Plan
1. Native ops (one `TypedOp` each, or a small family): **UTF-8 length** of a string range (unpaired surrogates count as
   U+FFFD: 3 bytes); **encode** a string range into a Uint8Array (a Buffer) at an offset, with a byte limit, returning
   bytes written (node's rule: never a partial character; a pair that doesn't fit isn't split); **decode** a byte
   range to a string (invalid sequences → U+FFFD exactly as node/V8's decoder: maximal subparts).
2. Use them in `byteLength` (utf8/utf-8 and the default), `utf8Write`, `utf8Slice`, `Buffer.from(string)` (its
   `fromString` path), `TextEncoder`/`TextDecoder` if they share the code (check `crates/frontend/node/encoding.js`),
   `StringDecoder` only if it calls these (keep its own state handling).
3. Fixtures vs node: `tests/fixtures/node/{buffer,buffer-edges,string-decoder}.js`,
   `tests/fixtures/builtins/text-coding.js`; add `tests/fixtures/node/buffer-utf8.js`: ASCII, 2/3/4-byte characters,
   unpaired high/low surrogates (also at the end of the string and at a write limit), `write` with offset/length that
   cut a character, `byteLength` of all of these, decoding of invalid/truncated/overlong sequences, empty strings,
   large strings (≥ 1 MiB).
4. A micro-benchmark (not a claim, a sanity check): `tasks/T80-files/utf8-bench.js` (byteLength / Buffer.from / write /
   toString on MQTT-sized strings, ASCII and mixed) under node and js2wasi3 + wasmtime, before/after; record the times.

## Acceptance
- [x] Native length/encode/decode used by `node:buffer`; `utf8ToBytes` no longer on these paths.
- [x] The fixtures above (and the new one) pass `scripts/diff.sh`; `wasm-tools validate`; a wasmtime smoke run;
      `cargo test` of the touched crates.
- [x] Micro-benchmark numbers before/after in Notes; follow-ups for anything left (F339 if decoding stays JS).
- [x] F348 ticked (and F339 if done) in `tasks/README.md`; the replay measurement is T82 (local, not here).

## Notes / Result
**What changed.** Three native ops, one `TypedOp` each (`crates/ir`, `crates/lower` `typed_op`, emitted in
`crates/backend/src/typed.rs` with two shared emitters `utf8_code_point` / `utf8_size`):
- `__intrinsic_typed_utf8_length(s)` → `TypedOp::Utf8Length`: UTF-8 byte count of a string (a pair is 4 bytes, an
  unpaired surrogate U+FFFD = 3).
- `__intrinsic_typed_utf8_write(t, s, offset, limit)` → `TypedOp::Utf8Write`: encodes into a Uint8Array's bytes,
  whole characters only while they fit in `limit`, returns the bytes written.
- `__intrinsic_typed_utf8_slice(t, start, end)` → `TypedOp::Utf8Slice`: the WHATWG decoder (U+FFFD per maximal
  invalid subpart) into a `$string` of `end - start` units, cut to the units written (one copy when not all ASCII/2-byte).

Used in `crates/frontend/node/buffer.js`: `byteLength` (`utf8`/`utf-8` and the unknown-encoding default),
`utf8Write` (so `buf.write` and `Buffer.from(string)` via `fromString`), `utf8Slice` (so `toString()`; F339) for
Uint8Array/Uint8ClampedArray receivers; the JS code stays as the fallback for other receivers (the fallback's
`utf8ToBytes` limit is now `min(length, remaining)`, so it no longer cuts a character at `length` either). Also
`TextEncoder.prototype.encode` (`crates/frontend/node/encoding.js`). `StringDecoder` uses `buf.toString('utf8', …)`,
so it gets the native decoder with its own state handling unchanged. Not changed: `TextEncoder.encodeInto` (needs the
units read too) and `TextDecoder.decode` (streaming, `fatal`, BOM): F353.

**Tests.** `scripts/diff.sh` passes on `tests/fixtures/node/{buffer,buffer-edges,string-decoder,buffer-utf8}.js` and
`tests/fixtures/builtins/text-coding.js`; the new `buffer-utf8.js` covers ASCII, 2/3/4-byte characters, unpaired
surrogates (also last and at a write limit), `write` with offset/length cutting characters (lengths 0–6), invalid /
truncated / overlong / surrogate-range byte sequences, `toString` ranges, Uint8Array views, `TextEncoder`, and 1 MiB+
strings. `wasm-tools validate` on the fixture and the benchmark; wasmtime smoke run (the fixture: 0.17 s).
`cargo test -p js2wasi3-ir -p js2wasi3-backend -p js2wasi3-lower -p js2wasi3-frontend`: all pass.

**Micro-benchmark** (`tasks/T80-files/utf8-bench.js`, 20,000 iterations each, MQTT-sized JSON: ascii 254 units /
254 bytes, mixed 147 units / 167 bytes; wasmtime 49.0.2 `-O gc-heap-initial-size=536870912`, cloud sandbox; ms):

| op | node | before | after |
|---|---|---|---|
| ascii byteLength | 3 | 2350 | 62 |
| ascii Buffer.from | 14 | 5716 | 130 |
| ascii write | 10 | 3601 | 57 |
| ascii toString | 7 | 1385 | 71 |
| mixed byteLength | 6 | 1191 | 49 |
| mixed Buffer.from | 22 | 3573 | 108 |
| mixed write | 11 | 2291 | 46 |
| mixed toString | 7 | 847 | 63 |

20× to 50× faster; still 5× to 20× node (the rest is the JS around the calls: argument checks, `Buffer.isBuffer`,
`isEncoding`, `createBuffer`). The replay measurement is T82 (local, a-03).

**Next smallest step.** T82 measures this on the replay; F353 if `TextDecoder` shows up in a profile.

## Progress
- 2026-10-05: handoff written (split from the earlier T80 plan "zigbee2mqtt speed on the T79 replay": T80 code, T81
  RegExp, T82 measurement on a-03). F347 (512 MiB in README and the run scripts) was done untested on 2026-10-04.
- 2026-10-05: cloud batch started (branch speed-batch from origin/main 76aea4a). Tools: wasmtime 49.0.2, wasm-tools 1.259.0, node 24.21.0 installed from release tarballs, rustc 1.98.1 via rustup; wkg, wit-bindgen, wac, wasm-opt missing (not needed here). Micro-benchmark before (tasks/T80-files/utf8-bench.js, 20000 iterations, debug compiler, wasmtime -O gc-heap-initial-size=512MiB): see Notes.
- 2026-10-05: before: ascii byteLength 2350 ms, Buffer.from 5716, write 3601, toString 1385 (table in Notes).
- 2026-10-05: TypedOp::Utf8Length/Utf8Write/Utf8Slice added (ir, lower, backend typed.rs); buffer.js byteLength,
  utf8Write, utf8Slice and TextEncoder.encode use them. New fixture tests/fixtures/node/buffer-utf8.js passes
  scripts/diff.sh first time; buffer, buffer-edges, string-decoder, text-coding pass; validate and smoke run ok.
- 2026-10-05: after: 20-50x faster (Notes). cargo test of ir, backend, lower, frontend: all pass. Done.
