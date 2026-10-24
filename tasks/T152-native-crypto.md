# T152: `crypto` primitives as hand-written Wasm over the GC byte arrays (no copies across the helper boundary)

Status: done (2026-10-24) | Depends on: T151 | Local; ask before long runs (test262, a-03) and before every push

## Goal
T151's Rust helper copies every `list<u8>` byte by byte into linear memory and back (a loop each way; Wasm has no bulk copy between a linear memory and
a WasmGC array): AES-GCM of 64 KB ~3 ms (21 MB/s), sha3 the same; node ~1 GB/s. The user (2026-10-24, "option 1"): write the throughput primitives in the
compiler's own Wasm generation (`wasm-encoder`, like the regex interpreter and `typed.rs`), working directly on a typed array's `$bytes`: no copies, linked
by reachability like every runtime helper (so no keyword scan for them, and the JS hashes are replaced by fast ones). The Rust helper keeps what is not
throughput-bound or too big to port: pbkdf2/scrypt/hkdf (copies are small) and, until ported, the rare digests (ripemd160, blake2, sm3).

## Design
- One `TypedOp` per primitive (`crates/ir`), a mechanical `__intrinsic_<name>` (`lower`), the body in `crates/backend/src/crypto.rs`, reading/writing `$typed` →
  `$buffer` → `$bytes` as `Utf8Write` does. Padding, buffering, HMAC, node's state machines and errors stay JavaScript in `crypto.js`.
- Hashes: `<alg>_blocks(state: Int32Array/Uint32Array, data: Uint8Array, offset, blocks)`: the compression function over whole blocks, state words in an `Int32Array`
  (SHA-512 and Keccak lanes as i32 pairs). MD5, SHA-1, SHA-256/224, SHA-512/384/512-t, Keccak-f[1600].
- Ciphers: AES block encrypt/CTR-xor over expanded round keys, GHASH (4-bit table), ChaCha20 block xor, Poly1305 blocks.
- Order: (A) plumbing + SHA-256, measure; (B) the other hashes; (C) AES-CTR/GHASH → GCM; (D) ChaCha20-Poly1305; (E) retire the keyword scan where nothing needs the helper.

## Plan
- [x] A: `TypedOp::Sha256Blocks` end to end; `Hash` in crypto.js uses it; fixture `crypto-hash-more.js` + `crypto.js` equal node; MB/s vs the Rust helper (3 ms / 64 KB) and the JS hash.
- [x] B: SHA-512 family, SHA-1, MD5, Keccak (sha3, shake).
- [x] C: AES (CTR + GHASH) → GCM; CBC/ECB/CCM could use the same block function (F503).
- [x] D: ChaCha20 + Poly1305.
- [x] E: decide what stays in the Rust helper and what the keyword scan still triggers; docs.

## Progress
- 2026-10-24: started on the user's "option 1". Plumbing read: `TypedOp` (ir) → `typed_op(name)` (lower) → `typed::Helper::Op` body (backend/typed.rs, e.g. `Utf8Write`).
- 2026-10-24: step A done in code: `TypedOp::Sha256Blocks` (ir) → `sha256_blocks` (lower) → `crates/backend/src/crypto.rs`; `crypto.js` `sha256` (and sha224) calls it; output equals node for lengths 0…1000 incl. the padding edges.
  Speed (release wasmtime, in-wasm timers): first version (W/K in GC arrays, loop of 64 rounds) 38 MB/s; rounds unrolled by 8: 59 MB/s; fully unrolled with the schedule in 16 locals and constants as immediates: ~115 MB/s
  (vs the Rust helper through the copy loops 21 MB/s, the old JS hash 0.3 MB/s). Lesson: avoid GC-array access in the inner loops; keep state in locals, unroll, constants as immediates.
  Found: `new Uint8Array(view)` is a per-element loop (107 ms/MB; `new Uint8Array(n)` + `.set` is 2 ms/MB) and `Buffer#slice()` is a view, not a copy: `Hash.update` must copy with alloc + set.
- 2026-10-24: step B done: `sha1_blocks`, `md5_blocks`, `sha512_blocks` (i64 locals; SHA-384 and SHA-512/224,256 by initial state, the 512/t IV computed with FIPS 180-4's method),
  `keccak_blocks` (sha3-*, shake*; round constants from the LFSR; `rate` 0 = squeeze another block) as `TypedOp`s; `crypto.js`'s JS hashes deleted; `md5-sha1` = md5 ‖ sha1.
  The names/aliases/sizes moved to `node:internal/crypto_hashes` (used by `crypto.js` and the helper-backed `crypto_kdf.js`). The helper now has only ripemd160, blake2*, sm3 (and AES-GCM/ChaCha until C/D);
  trigger words reduced to `pbkdf2|scrypt|hkdf|gcm|chacha|ripemd|rmd160|blake2|sm3` (+ upper case).
  `crypto-hash-more.js` and `crypto.js` fixtures equal node. Speed (4 MB, release wasmtime; node 24 in brackets): md5 169 (501), sha1 162 (707), sha256 94 (348), sha512 125 (503),
  sha3-256 59 (291), sha3-512 35 (159), shake128 77 (354) MB/s; before: JS ~0.3 MB/s, Rust helper 21 MB/s.
  Next: C (AES-CTR + GHASH natively, tables as `i32s` arrays built by JS once).
- 2026-10-24: step C/D done. AES-GCM: `aes_ctr` (AES by SIMD: SubBytes = sixteen `i8x16.swizzle`s over the S-box rows (an index ≥ 16 reads 0), ShiftRows/MixColumns swizzles + xors, round keys in `v128`
  locals; GC-array T-tables were 2.3× slower: a GC array access is ~11 instructions on wasmtime, with spills), `ghash_init`/`ghash_blocks` (BearSSL's constant-time 64-bit carry-less multiply: Karatsuba over `bmul64`,
  the high halves from the reversed operands; the first 4-bit-table version was right but half the speed). ChaCha20-Poly1305: `chacha20_xor` (carry into word 13 when the counter wraps, as OpenSSL: found by the fixture),
  `poly1305_blocks` (26-bit limbs on `i64`). The Rust helper lost its `cipher` resource (170 KB now) and the trigger words `gcm`/`chacha`; `crypto.js` has `Gcm`, `ChaChaPoly`, `ChaCha20Stream`, `BlockMac`.
  `crypto-gcm.js` (now with odd-chunk streams, counter wrap, buffer mutation) and the other crypto fixtures equal node. Speed (laptop, 1 MB messages; node 24 in brackets): aes-128-gcm 42 (761), aes-256-gcm 35 (553),
  chacha20-poly1305 100 (590) MB/s; 1 KB messages 8 / 9 / 13 MB/s (node 21–25): per-object setup ~80 µs (node 33). Before (Rust helper + copies): 21 MB/s for all.
  Not done: AES in CBC/ECB/CTR/CCM still run the JS AES (F503); AES-GCM could be ~3× faster with a nibble-table S-box; `pbkdf2`/`scrypt`/`hkdf`, `ripemd160`, `blake2`, `sm3` stay in the Rust helper.

## Notes / Result
Done (2026-10-24). The hashes (md5, sha1, sha224/256/384/512, sha512-224/256, sha3-*, shake128/256, md5-sha1) and the ciphers (AES-128/192/256-GCM, ChaCha20-Poly1305, ChaCha20) are Wasm generated by the
backend (`crates/backend/src/crypto.rs`, `TypedOp`s `sha256_blocks`, `sha1_blocks`, `md5_blocks`, `sha512_blocks`, `keccak_blocks`, `aes_ctr`, `ghash_init`, `ghash_blocks`, `chacha20_xor`, `poly1305_blocks`)
working on the typed arrays' `$bytes`: no copies, linked by reachability, the Rust helper not involved. Padding, buffering, HMAC, node's state machines and errors stay JavaScript (`crypto.js`, names in `crypto_hashes.js`).
- Speed (laptop, release wasmtime, 4 MB / 1 MB messages; node 24 in brackets): md5 169 (501), sha1 162 (707), sha256 94 (348), sha512 125 (503), sha3-256 59 (291), sha3-512 35 (159), shake128 77 (354),
  aes-128-gcm 42 (761), aes-256-gcm 35 (553), chacha20-poly1305 100 (590) MB/s; 1 KB messages: GCM 8–9, ChaCha 13 (node 21–25). Before: JS hashes ~0.3 MB/s, Rust helper ciphers 21 MB/s.
- Lessons: a GC array access is ~11 machine instructions with spills on wasmtime (T-table AES with `i32s` tables 2.3× slower than SIMD S-box swizzles): keep inner loops on locals, constants as immediates, unroll;
  `new Uint8Array(view)` / `Uint8Array.from` copy element by element (use alloc + `set`); `Buffer#slice` is a view.
- Tests: `crypto-hash-more.js`, `crypto-gcm.js` (new cases: odd chunk sizes, counter wrap, buffer mutation), `crypto.js`, `crypto-kdf*.js`, `crypto-global.js` equal node; `wasm-tools validate`. Not run: test262, the a-03 gate
  (no benchmark imports `crypto`; the new ops exist only where `crypto.js` is linked), the whole fixture suite.
- Trigger words now `pbkdf2|scrypt|hkdf|ripemd|rmd160|blake2|sm3` (+ upper case): the helper is linked only for those.
- Next: F507 (AES CBC/ECB/CTR/CCM native; faster S-box), F508 (small-message setup), F509 (small hashes), F505 (`subtle`).
