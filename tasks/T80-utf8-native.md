# T80: native UTF-8 encoding in `node:buffer` (F348)

Status: todo (handoff written 2026-10-05) | Depends on: T79 (profile) | Cloud batch: yes (`tasks/CLOUD-BATCH.md`)

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
- [ ] Native length/encode/decode used by `node:buffer`; `utf8ToBytes` no longer on these paths.
- [ ] The fixtures above (and the new one) pass `scripts/diff.sh`; `wasm-tools validate`; a wasmtime smoke run;
      `cargo test` of the touched crates.
- [ ] Micro-benchmark numbers before/after in Notes; follow-ups for anything left (F339 if decoding stays JS).
- [ ] F348 ticked (and F339 if done) in `tasks/README.md`; the replay measurement is T82 (local, not here).

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written (split from the earlier T80 plan "zigbee2mqtt speed on the T79 replay": T80 code, T81
  RegExp, T82 measurement on a-03). F347 (512 MiB in README and the run scripts) was done untested on 2026-10-04.
