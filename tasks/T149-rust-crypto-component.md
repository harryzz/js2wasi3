# T149: `crypto` in Rust, as a component linked with the program (F489)

Status: not started (handoff, 2026-10-22) | Depends on: T148 | Local; ask before installing the Rust target and before long runs

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
- [ ] Decisions above with the user; install `rustup target add wasm32-wasip2` (ask).
- [ ] WIT (`js2wasi3:crypto@0.1.0`, interface `primitives`): `pbkdf2(password, salt, iterations, keylen, digest) -> result<list<u8>, string>`,
  `scrypt(...)`, `hkdf(...)`; the Rust crate (`pbkdf2`, `scrypt`, `hkdf`, `hmac`, `sha1`, `sha2`) built as a component; unit tests with RFC 6070
  (pbkdf2-sha1), RFC 7914 (scrypt), RFC 5869 (hkdf) vectors.
- [ ] Compiler side: the missing `crypto.*` functions in `crypto.js` call the import (argument checks and node's error messages/codes first, in
  JavaScript); `host.rs`/packaging: the import is added only when a program reaches those functions; the unresolved import is clear in the error
  message when the helper is not composed ("run scripts/link-crypto.sh").
- [ ] Composition script and docs; `wac plug` result runs under wasmtime 50.0.0.
- [ ] Fixtures against node (`tests/fixtures/node/crypto-pbkdf2.js`, random inputs and the RFC vectors; node is the oracle; fixtures needing the helper use
  the composed build in `scripts/diff.sh` or a documented way), Express `auth` example end to end (`examples/express`, `pbkdf2-password`).
- [ ] Speed: time 10^5-iteration `pbkdf2` in the helper vs a JavaScript version (plain numbers; wasmtime and, for reference, node).
- [ ] Docs (`docs/design.md`, `README.md` "What works", `docs/compat` regenerated with `node scripts/compat.mjs`, `TOOLS.md` for the Rust target and helper
  version, `CLAUDE.md`, `tasks/README.md` F489), gate on a-03 (programs not using crypto byte-identical), test262 (ask).

## Read first
- `tasks/T148-express-gaps.md` (Notes / Result), `crates/frontend/node/crypto.js`, `crates/backend/src/host.rs`, `crates/backend/src/abi.rs`,
  `docs/design.md` "Slice 5" (imports), `examples/express/README.md` ("What does not work").

## Standing rules (CLAUDE.md and the user's memory)
Ask before installing tools, before long runs (test262, the a-03 gate) and before every push; no attribution lines in commits; plain numbers; no
bare ids in reports (say what a task or follow-up is); use `scripts/diff.sh`, `scripts/gate.sh` on a-03; never use or mention zigbee2mqtt.

## Notes / Result

## Progress
- 2026-10-22: written as a handoff (user: "yes handoff task for this to start in new session"). State at the handoff: `main` at 3c82af1 (T148 pushed).
