# T149: `crypto` in Rust, as a component linked with the program (F489)

Status: done (2026-10-22) | Depends on: T148 | Local; ask before installing the Rust target and before long runs

## Goal
Node's `crypto` has 110+ functions js2wasi3 lacks (`crypto.pbkdf2`/`pbkdf2Sync`, `scrypt`, `hkdf`, AES-GCM, sign/verify, key pairs, ...). WASI has
only `wasi:random`; `wasi-crypto` is not part of WASI 0.3 and wasmtime removed its in-tree support (issues #6732/#6816), so a custom host is out.
Plan: write the heavy primitives in **Rust** (RustCrypto crates) as a small **Wasm component**, import it from `crypto.js`, and **compose** it
with the program's component at build time (`wac plug`) into one component that runs on a stock wasmtime. First consumer: Express's `auth`
example (`pbkdf2-password` calls `crypto.pbkdf2`), see `examples/express/README.md`.

## Context (found in T148, 2026-10-22)
- `crates/frontend/node/crypto.js` (993 lines) is a subset written in JavaScript: random values from `wasi:random`, hashes (`md5`, `sha1`..`sha512`),
  `createHash`/`createHmac`/`hash`, AES. Compat data (`site/compatibility.json`, chapter `crypto`): 25 supported, 110 missing, 4 partial.
- `pbkdf2` in JavaScript would be HMAC in a loop (we have HMAC) but slow at node's usual 10^5 iterations; `scrypt` and asymmetric crypto are
  impractical in compiled JavaScript. Rust crypto in Wasm runs near native speed.
- js2wasi3 already imports user interfaces with `list<u8>` ↔ `Uint8Array` (`docs/design.md`, "Slice 5"; `WitType::List` in `crates/ir/src/lib.rs`),
  host imports are listed in `crates/backend/src/host.rs` (`Import`, `import_list`, e.g. `LIBRARY_TIMER_IMPORTS`) and used from the prelude
  JavaScript through intrinsics (see how `net.js` reaches `wasi.*`).
- `wac` 0.11.0, `wkg` 0.16.1 and `wit-bindgen` 0.62.0 are pinned in `TOOLS.md` (the laptop has them; a-03 does not). `wac plug app.wasm --plug
  crypto.wasm -o linked.wasm` composes at build time. wasmtime's CLI cannot link two components at run time (only an embedding host can): build-time
  composition is the standard route.
- The Rust target `wasm32-wasip2` is probably not installed (`rustup target list --installed`): installing it needs the user's yes.

## Design decisions to take first (ask the user)
1. **Where the helper lives**: a separate crate (`crates/crypto-helper/`, outside the main workspace so `cargo build` of the compiler stays unchanged) with
   a prebuilt `crypto-helper.wasm` checked in (size?) or built by a script. Prefer: source in the repo, prebuilt artefact under `crates/js2wasi3/assets/`
   (or release asset) and `scripts/build-crypto-helper.sh` to rebuild.
2. **How composition is done**: document `wac plug` as an extra step + `scripts/link-crypto.sh` (no new Rust dependency in the compiler; the user prefers
   no new dependencies), or a `js2wasi3 build --link-crypto` flag (needs `wac-graph`/`wasm-compose`: a new dependency, ask first).
3. **Scope per slice**: slice 1 `pbkdf2` (hmac-sha1/224/256/384/512), `scrypt`, `hkdf`; slice 2 AES-CBC/CTR/GCM, `timingSafeEqual`, bigger hash list
   (`sha3`, `ripemd160`, `blake2`); slice 3 sign/verify, `generateKeyPair`, ECDH/RSA (heavy; maybe later).
4. **The import is declared only when used** (so programs not using these functions stay byte-identical; check with `scripts/gate.sh`).

## Plan
- [x] Decisions above with the user; install `rustup target add wasm32-wasip2` (ask).
- [x] WIT (`js2wasi3:crypto@0.1.0`, interface `primitives`): `pbkdf2(password, salt, iterations, keylen, digest) -> result<list<u8>, string>`,
  `scrypt(...)`, `hkdf(...)`; the Rust crate (`pbkdf2`, `scrypt`, `hkdf`, `hmac`, `sha1`, `sha2`) built as a component; unit tests with RFC 6070
  (pbkdf2-sha1), RFC 7914 (scrypt), RFC 5869 (hkdf) vectors.
- [x] Compiler side: the missing `crypto.*` functions in `crypto.js` call the import (argument checks and node's error messages/codes first, in
  JavaScript); `host.rs`/packaging: the import is added only when a program reaches those functions; the unresolved import is clear in the error
  message when the helper is not composed ("run scripts/link-crypto.sh").
- [x] Composition script and docs; `wac plug` result runs under wasmtime 50.0.0.
- [x] Fixtures against node (`tests/fixtures/node/crypto-pbkdf2.js`, random inputs and the RFC vectors; node is the oracle; fixtures needing the helper use
  the composed build in `scripts/diff.sh` or a documented way), Express `auth` example end to end (`examples/express`, `pbkdf2-password`).
- [x] Speed: time 10^5-iteration `pbkdf2` in the helper vs a JavaScript version (plain numbers; wasmtime and, for reference, node).
- [x] Docs (`docs/design.md`, `README.md` "What works", `docs/compat` regenerated with `node scripts/compat.mjs`, `TOOLS.md` for the Rust target and helper
  version, `CLAUDE.md`, `tasks/README.md` F489), gate on a-03 (programs not using crypto byte-identical), test262 (ask).

## Read first
- `tasks/T148-express-gaps.md` (Notes / Result), `crates/frontend/node/crypto.js`, `crates/backend/src/host.rs`, `crates/backend/src/abi.rs`,
  `docs/design.md` "Slice 5" (imports), `examples/express/README.md` ("What does not work").

## Standing rules (CLAUDE.md and the user's memory)
Ask before installing tools, before long runs (test262, the a-03 gate) and before every push; no attribution lines in commits; plain numbers; no
bare ids in reports (say what a task or follow-up is); use `scripts/diff.sh`, `scripts/gate.sh` on a-03; never use or mention zigbee2mqtt.

## Notes / Result
Done, slice 1. `pbkdf2`/`pbkdf2Sync`, `scrypt`/`scryptSync`, `hkdf`/`hkdfSync` (md5, sha1, sha224/256/384/512) come from a Rust
component (`crates/crypto-helper`, RustCrypto, 87 KB, no imports, built for `wasm32-unknown-unknown` + `wasm-tools component new`
because the `wasm32-wasip2` std build imports 14 WASI 0.2 interfaces). `scripts/link-crypto.sh` composes it (`wac plug`);
`scripts/diff.sh` and `scripts/compat.mjs` call it; `build` prints a note when the import is present. Argument checks and errors are
node's, in JavaScript (`crates/frontend/node/crypto_kdf.js`).
- Activation: `link::link` links twice; the second time only when a non-built-in module's text contains `pbkdf2|scrypt|hkdf`, so
  21 programs (18 benchmarks + others) build byte-identical to HEAD; a `crypto` user grows ~1.5 KB.
- Tests: `scripts/diff.sh tests/fixtures/node/crypto-kdf.js crypto.js` pass (RFC 6070/7914/5869 vectors, ~90 checks incl. errors,
  callbacks); `pbkdf2-password` 1.2.1 (Express `auth`) identical to node. Not run: test262 and the a-03 gate (no output changed for
  non-crypto programs; ask if wanted). compat page regenerated: supported 638 → 644.
- Speed (laptop, wasmtime 50.0.0): 10^5 × PBKDF2-SHA256 89–117 ms (node 69 ms; JavaScript over `createHmac` ~70 s); scrypt defaults
  195–234 ms (node 65 ms).
- Unsupported / next: F492 (AES, more digests), F493 (sign/verify, key pairs), F494 (text-scan activation limits, forcing option,
  `build` composing itself), F495 (scrypt speed, async blocking).

## Progress
- 2026-10-22: written as a handoff (user: "yes handoff task for this to start in new session"). State at the handoff: `main` at 3c82af1 (T148 pushed).
- 2026-10-22: user said yes to the recommended decisions: helper crate `crates/crypto-helper/` (excluded from the workspace) + prebuilt wasm under `crates/js2wasi3/assets/` + `scripts/build-crypto-helper.sh` and `scripts/link-crypto.sh` (`wac plug`), no new compiler dependency; slice 1 = pbkdf2 / scrypt / hkdf; import declared only when used. Also yes to `rustup target add wasm32-wasip2`.
- 2026-10-22: compiler plan: vendored WIT `js2wasi3:crypto/primitives@0.1.0` added to `component::WASI`/`COMMAND_WASI_IMPORTS`; ES-module wrapper `node:internal/crypto_helper` (as `wasi_random`); activation: `link()` links once with the wrapper resolved to a stub (`module.exports = null`), scans non-built-in modules' sources for `pbkdf2|scrypt|hkdf` (+`Sync`), re-links with the wrapper when found (so other programs stay byte-identical). Helper built for `wasm32-wasip2` (check it has no wasi imports; else `wasm32-unknown-unknown` + `wasm-tools component new`).
- 2026-10-22: helper crate builds (pbkdf2/scrypt/hkdf, RustCrypto 0.11–0.13 + wit-bindgen 0.62.0; wasm32-wasip2 imports 14 wasi 0.2 interfaces, so wasm32-unknown-unknown + `wasm-tools component new`: 79,812 bytes, no imports). `scripts/build-crypto-helper.sh` writes `crates/js2wasi3/assets/crypto-helper.wasm`; crate excluded from the workspace. Next: compiler side (WIT in component, wrapper module, link scan).
- 2026-10-22: compiler side done: vendored WIT in `crates/component/wit/js2wasi3-crypto-0.1.0.wit` (+ `COMMAND_WASI_IMPORTS`), `node:internal/crypto_helper` (ES wrapper) / `crypto_helper_off` (stub), `link()` links twice (second time with the wrapper) when a non-built-in module's text contains `pbkdf2|scrypt|hkdf`; `crypto.js` has `pbkdf2(Sync)`, `scrypt(Sync)`, `hkdf(Sync)` with node's checks/errors (md5, sha1, sha224…sha512 digests). `scripts/link-crypto.sh` (wac plug; no-op when there is no helper import); `scripts/diff.sh` composes after each build. `tests/fixtures/node/crypto-kdf.js` (RFC 6070/7914/5869 vectors, ~90 checks incl. error messages, callbacks): identical to node. Note: tests/fixtures is `"type": "module"` (fixtures use `import`). u64 params are JS numbers at the boundary, not BigInt.
- 2026-10-22: `build --ext-lib crypto` (repeatable; values listed in `main.rs`, `Options::ext_libs`) links the helper even when no text names the functions; fixture `crypto-kdf-forced.js` + `.flags`.
- 2026-10-22: standard benchmark: node v24.21.0 `benchmark/crypto/hkdf.js` (unchanged, `tests/bench/crypto/`, harness stand-in `tests/bench/common.js`), `scripts/bench-crypto.sh`: sync 1.39× node, async 2.84× node (laptop). Node has no pbkdf2/scrypt benchmark; test262 has no crypto. Not part of `scripts/gate.sh` (the helper does not change with the compiler).
- 2026-10-22: finished; docs (README, design.md, TOOLS.md, CLAUDE.md, Express README, compat) updated.
