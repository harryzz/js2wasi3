# T151: `crypto` slice 2: AES-GCM and more ciphers, more digests, in the Rust helper (F492)

Status: not started (handoff, 2026-10-24) | Depends on: T149, T150 | Local; ask before long runs (test262, a-03) and before every push

## Goal
T149 gave `pbkdf2`/`scrypt`/`hkdf` through a Rust helper component that `build` composes into programs naming them. The rest of node's `crypto`
is still JavaScript (`crates/frontend/node/crypto.js`: md5, sha1, sha224/256/384/512 hashes and HMAC, AES-128/192/256 in ECB, CBC, CTR and CCM,
`timingSafeEqual`, random) or missing (about 100 functions; compat page chapter `crypto`: 104 missing). This slice, from the follow-up list (F492):
1. **Ciphers**: AES-GCM (`aes-128-gcm`, `aes-192-gcm`, `aes-256-gcm` with `setAAD`, `getAuthTag`, `setAuthTag`, `authTagLength`, node's errors such as
   `Unsupported state or unable to authenticate data`), then `chacha20-poly1305`, `aes-*-ctr/cbc` through the helper only if it pays (speed), `getCiphers()`.
2. **Digests**: `sha3-224/256/384/512`, `shake128/256` (with `outputLength`), `ripemd160`, `blake2b512`, `blake2s256`, `sha512-224`, `sha512-256`,
   `md5-sha1`? (check what node 24 lists in `crypto.getHashes()`), for `createHash`, `createHmac`, `hash`, and as the `digest` of `pbkdf2`/`hkdf`.
   Today these give `Invalid digest` / `Digest method not supported`. `Hash.copy()`.
3. Optional if time: `crypto.subtle.digest` / `encrypt` / `decrypt` / `importKey` for AES-GCM and HMAC over the same primitives (compat: 28 `web_crypto_api` rows missing).

## Where things are (read these first)
- `tasks/T149-rust-crypto-component.md` (the whole mechanism and its Notes), `tasks/T150-zlib-rust-helper.md` (the generalisation: `EXT_LIBS`, `compose.rs`,
  three more helpers, the window feeding, what went wrong).
- `crates/crypto-helper/` (Rust crate, outside the workspace; `src/lib.rs`; WIT in `wit/crypto.wit`, copied to `crates/component/wit/js2wasi3-crypto-0.1.0.wit`),
  built by `scripts/build-crypto-helper.sh` into `crates/js2wasi3/assets/crypto-helper.wasm` (checked in; `wasm32-unknown-unknown` + `wasm-tools component new`).
- `crates/frontend/node/crypto.js` (the JS crypto, `HASHES`, `hashFunction`, `Hash`, `Hmac`, `Cipheriv`, `Decipheriv`), `crates/frontend/node/crypto_kdf.js`
  (the part over the helper; `require('node:internal/crypto_helper')`), `crypto_helper.mjs` (the ES wrapper that imports the WIT interface).
- `crates/frontend/src/link.rs` `EXT_LIBS` (name, module, trigger words) and `link()`; `crates/frontend/src/node.rs` (module table, `is_esm`);
  `crates/js2wasi3/src/compose.rs` (helpers table); `--ext-lib` in `crates/js2wasi3/src/main.rs`.
- Node's behaviour is the oracle: probe with `node` before writing (as T149/T150 did: error codes, messages, argument order of checks).

## Design points to settle first
- **Trigger words** (`EXT_LIBS["crypto"]`): today `pbkdf2`, `scrypt`, `hkdf`. Programs that only use the JS hashes/AES must stay byte-identical, so do
  not add words like `aes` or `sha256`. Add the *new* names: `gcm`, `chacha`, `sha3`, `shake`, `ripemd`, `blake2`, `sha512-2` (check false positives in the
  18 benchmark programs and the fixtures: they must not match). Text of non-built-in modules is scanned; `--ext-lib crypto` forces it.
  If `createHash(name)` takes a computed name the helper is not linked: keep today's error for that case but say why.
- **Module split**: `crypto.js` spreads `require('node:internal/crypto_kdf')` (empty `_off` stub when the helper is not linked). Put the new
  functions behind the same switch (rename the module if you like, e.g. `crypto_ext`, keep the `_off` stub rule) and make `hashFunction` /
  `Cipheriv` look for the helper's algorithms only when it is linked.
- **WIT shape**: streaming state needs resources like the zlib helpers use (`resource hasher { constructor(name), update(bytes), digest(len?), clone() }`,
  `resource cipher { create(name, key, iv, encrypt, aad-len, tag-len), update, set-aad, final, get-tag, set-tag }`). `list<u8>` crosses the boundary by copy
  (a byte loop each way); u64 are JS numbers; `result<_, string>` errors arrive as a thrown value with `.payload`. Resource handles are freed by
  an explicit drop only (no GC finalizers: F501) — drop on `digest()` / `final()` / error, and note leaked unfinished hashes.
- **Speed**: JS md5/sha1/sha2 are not fast; decide (measure with node's benchmark files) whether `createHash` for the existing algorithms should use the
  helper when it is linked. Do not link the helper just for them (byte-identical rule).
- **Size**: the helper is 87 KB now; RustCrypto `aes-gcm`, `chacha20poly1305`, `sha3`, `ripemd`, `blake2` add roughly 100–200 KB. Keep `opt-level = "s"`.
- Crate versions: the latest RustCrypto (the crypto helper uses hmac 0.13, sha2 0.11, pbkdf2 0.13, scrypt 0.12, hkdf 0.13; `aes-gcm`/`chacha20poly1305`
  versions that match digest/crypto-common 0.2). Record them in `TOOLS.md`.

## Plan
- [ ] Probe node 24.21.0: `crypto.getHashes()`, `getCiphers()`, GCM states and errors, `Hash.copy`, xof (`shake`) `outputLength`, error texts for each misuse.
- [ ] WIT + Rust: hashes (sha3, shake, ripemd160, blake2b512/s256, sha512-224/256), HMAC over them, AES-GCM, ChaCha20-Poly1305; unit checks with NIST/RFC vectors
  (RFC 8439, NIST GCM test vectors, SHA-3 vectors).
- [ ] JS: `Hash`/`Hmac`/`hash`/`pbkdf2`/`hkdf` digests, `Cipheriv`/`Decipheriv` for the new ciphers with node's state machine and errors; `getHashes()`/`getCiphers()`.
- [ ] Trigger words in `EXT_LIBS`, `--ext-lib crypto`; check the 18 benchmarks + fixtures build byte-identical.
- [ ] Fixtures against node (`tests/fixtures/node/crypto-gcm.js`, `crypto-hash-more.js`: node is the oracle; random keys/IVs are fixed in the file).
- [ ] Node's own benchmarks in `tests/bench/crypto` (add `create-hash.js`, `create-hmac.js`, `oneshot-hash.js`, `aes-gcm-throughput.js`, `hmac-throughput.js`,
  `randomBytes.js`... unchanged; `scripts/bench-node.sh`): record node vs js2wasi3.
- [ ] Docs: README "What works", `docs/design.md` T149 paragraph, `docs/keywords.md` crypto row, `TOOLS.md`, `CLAUDE.md`, compat page (`node scripts/compat.mjs`,
  needs `cargo build --release`), `tasks/README.md` (F492 ticked, new follow-ups), `tests/test262/HISTORY.md` (run only with the user's yes; a-03 ~25 min).

## Standing rules (CLAUDE.md and the user's memory)
Ask before installing tools, before long runs and before every push; no attribution lines in commits or docs; plain numbers; describe tasks and follow-ups
in words, not bare ids; never mention zigbee2mqtt; speed comparisons only against the standard tests/benchmarks; write progress to the task file after
every step; temp files only in the session's scratchpad, delete them at the end. `scripts/diff.sh -j 1` for the heavy fixtures (the 10 s run timeout fails
under parallel load with the debug compiler). Do not `pkill -f` a pattern that also appears in your own command line (it kills the shell).
`clang` (wasm32 target) is on the PATH for C code in helpers (`CC_wasm32_unknown_unknown=clang`).

## Notes / Result

## Progress
- 2026-10-24: written as a handoff (user: "handoff crypto slice 2 (AES, more digests) for new session"). State: `main` at 92a4dbb plus the docs commit that adds this file;
  T149 and T150 pushed and done.
