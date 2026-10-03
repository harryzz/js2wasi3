# T65: node built-ins that only need to load, and a `crypto` subset (F267)

Status: done (2026-10-02) | Depends on: T64

## Goal
The 37 link errors left in zigbee2mqtt's graph (T62) are node built-ins js2wasi3 lacks. Most are required at load
but used only on paths the milestone doesn't take; each should match node on its own fixture where it runs:
1. Modules that load, with node's shape (exports, classes, constants) and functions that fail like node does
   without the facility: `child_process` (`exec` reports `ENOENT` through its callback, as with no `git`), `dns`,
   `dgram`, `tls`, `http`, `https`, `http2`, `zlib`, `perf_hooks`, `module`.
2. A `crypto` subset zigbee-herdsman and zigbee2mqtt use: `randomBytes`, `randomUUID`, `getRandomValues` (and the
   `crypto` global's), `createHash` (`md5`, `sha1`, `sha256`), AES `createCipheriv`/`createDecipheriv` (Green Power
   frames, zspec).
Programs that don't name them keep their bytes.

## What we know (at handoff)
- T62/T64 counts (docs/zigbee2mqtt-feasibility.md "Survey 2026-10-01"): `crypto` 9, `http`/`https`/`http2` 8,
  `net` 7 (T66's), `tls` 4, `dgram` 3, `zlib` 3, `child_process` 2, `dns` 1, `perf_hooks` 1 (mqtt, ws, srvx,
  winston(-syslog), socks, multicast-dns, zigbee-herdsman, zigbee-on-host, @serialport/bindings-cpp, zigbee2mqtt).
  T62's `tasks/T62-files/` stubs show the shapes each package touches at load (`stubs/*.js`).
- Built-ins are CommonJS modules in `crates/frontend/node/` listed in `crates/frontend/src/node.rs` `MODULES`
  (T55); a module can import WASI interfaces directly (`import { f } from "wasi:…"`, T59's `fs` over
  `wasi:filesystem`, ES module `wasi_fs.mjs`), so `crypto.randomBytes` can use `wasi:random/random`
  `get-random-bytes`. Hashes and AES are plain JS over typed arrays (keep them small; node's `crypto` errors).
- node's own lib for `child_process`, `http`, … depends on C++ bindings throughout: write minimal modules with
  node's exports rather than porting (T64's `url` port worked because it is mostly JS).

## Plan
1. Load-only modules, one fixture per family (`node/child-process.js`, `node/http.js`, …): exports, `typeof`s, the
   failure path (`exec("git …", cb)` → `ENOENT`).
2. `crypto`: random first (WASI), then `createHash`, then AES; fixture `node/crypto.js` against node.
3. Probe: `T63_GLOBALS=0 tasks/T63-files/build-run.sh` with `patch.sh`'s built-in stubs removed for what this task
   provides; record which stubs remain.

## Read first
- `tasks/T64-url-text-dataview.md` (Notes / Result), `tasks/T59-fs.md` (WASI imports from a built-in module),
  `tasks/T55-node-builtins.md`, `docs/zigbee2mqtt-feasibility.md` (Survey 2026-10-01).

## Tests
Each fixture through `scripts/diff.sh`; `cargo test`.

## Acceptance
- [x] Items 1–2 equal node on their fixtures; programs without them keep their bytes.
- [x] zigbee2mqtt builds without the T62 built-in stubs this task replaces.
- [x] Follow-ups updated.

## Notes / Result
**What changed**
- `node:crypto` (`crates/frontend/node/crypto.js`), JS over typed arrays:
  - random bytes from `wasi:random/random` `get-random-bytes` through the ES module `node:internal/wasi_random`
    (`wasi_random.mjs`, like T59's `wasi_fs`): `randomBytes` (callback form on `nextTick`), `randomFill(Sync)`,
    `randomInt` (48-bit rejection sampling), `randomUUID`, `getRandomValues` (node's `TypeMismatchError`/
    `QuotaExceededError`);
  - `createHash`/`hash`/`getHashes` (md5, sha1, sha224, sha256, sha384, sha512), `createHmac`, `copy()`;
  - AES-128/192/256 ECB/CBC (PKCS#7, `setAutoPadding`), CTR, CCM (RFC 3610; `authTagLength`, `setAAD`,
    `getAuthTag`/`setAuthTag`), `createCipheriv`/`createDecipheriv`, `getCiphers`; `timingSafeEqual`;
  - node's error codes and OpenSSL's messages (`bad decrypt`, `wrong final block length`, `Unknown cipher`, …).
- The `crypto` global: `node::GLOBALS` binds free `crypto` references to `node:crypto`'s `webcrypto`
  (`getRandomValues`, `randomUUID`; `subtle` exists, its methods reject with `NotSupportedError`).
- Load-only built-ins, hand-written with node 24's exports (`crates/frontend/node/`):
  - `http` (`Agent`, `globalAgent`, `METHODS`, `STATUS_CODES`, `maxHeaderSize`, `validateHeaderName/Value`,
    `IncomingMessage`/`OutgoingMessage`/`ServerResponse` constructible with header methods; `Server`,
    `ClientRequest`, `createServer`, `request`, `get` throw), `https` (`Agent`, `globalAgent`), `http2`
    (constants, `getDefaultSettings`, `sensitiveHeaders`);
  - `tls` (defaults, `getCiphers`, `convertALPNProtocols`; `rootCertificates` empty), `dgram`, `dns` (codes,
    flags, `get/setDefaultResultOrder`, `promises`), `zlib` (constants, codes, non-enumerable `Z_*`, `crc32`),
    `perf_hooks` (`performance` is the global, constants), `module` (`Module`, `builtinModules`, `isBuiltin`,
    `wrap`, `_nodeModulePaths`, `createRequire` → `MODULE_NOT_FOUND` with node's message);
  - `child_process`: every spawn fails as for a missing program (`spawn <file> ENOENT` on `error`, then `close`
    with -2; `exec`/`execFile` call back; `spawnSync` returns, `execSync`/`execFileSync` throw the error with
    node's fields; a command runs `/bin/sh -c` or `options.shell`).
  - What needs a facility throws `<module>.<name>() isn't available in js2wasi3 (<why>)` (`node:internal/unavailable`).
    Constants come from `node:internal/builtin_data`, generated by `tasks/T65-files/gen-data.mjs` (node 24.21.0).
  - `node::exports` lists the ES named exports of the modules that fill `module.exports` in loops or with accessors.
- zigbee2mqtt probe: `tasks/T65-files/{patch.sh,build-run.sh}` stub only `net`.

**Commands run**
- `scripts/diff.sh tests/fixtures/node/{crypto,crypto-global,http,tls-dns-dgram,zlib,child-process,module-perf}.js`: 7 pass.
- `wasm-tools validate --features all` on the crypto, http and child-process builds; wasmtime runs via `diff.sh`.
- `T63_GLOBALS=0 $S/t65/build-run.sh` (release build): builds in 89.0 s, 32.5 MB (T64: 79.7 s, 32.3 MB), runs to
  `Opening TCP socket with 127.0.0.1:6638`, then the `net.Socket` stub, as in T64.
- Bytes check (every command fixture, HEAD 6548d5d vs this tree): every fixture that builds with both is byte-identical (the 11 that fail are the 7 new ones and 4 that fail with both).
- `cargo test`: all pass but the 4 `*_compose_with_wac` (no `wac` in this container); three `link::tests` used `zlib` as the example of a missing built-in and now use `worker_threads`.

**Unsupported** (Follow-ups): crypto beyond the subset (F280); TLS (F281); UDP (F282); name lookup (F283);
compression (F284); the performance timeline (F285); HTTP clients/servers (F286); run-time `require` through
`createRequire` (F287).

**Next smallest step**: T66, `net.Socket` over `wasi:sockets` (F268): first a probe connecting to a closed and
an open port with the WIT calls, to see how a built-in module can read `receive`'s `stream<u8>` without blocking.

## Checklist
- [x] load-only modules
- [x] `crypto` subset
- [x] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-02: handoff written after T64.
- 2026-10-02: `crates/frontend/node/crypto.js` written (random over `wasi:random` via ES module `wasi_random.mjs`,
  md5/sha1/sha224/256/384/512, HMAC, AES-128/192/256 ECB/CBC/CTR/CCM, `timingSafeEqual`, `webcrypto`). Fixture
  `node/crypto.js` passes `scripts/diff.sh` after fixing the S-box inverse of 1 (`exp[255]` read past the table).
  `crypto` global = `webcrypto` (GLOBALS entry); `crypto.subtle` exists, its methods reject with NotSupportedError.
  Fixture `node/crypto-global.js` passes. Unknown cipher → `Error ERR_CRYPTO_UNKNOWN_CIPHER Unknown cipher` (node).
- 2026-10-02: load-only modules written by hand (`http`, `https`, `http2`, `tls`, `dgram`, `dns`, `zlib`,
  `perf_hooks`, `module`, `child_process`) over `internal/unavailable` (throwing functions/classes) and generated
  `internal/builtin_data` (`tasks/T65-files/gen-data.mjs`: METHODS, STATUS_CODES, http2/zlib/perf constants, TLS
  ciphers, dns codes, builtinModules). Working parts: `http.Agent`, header validation, `OutgoingMessage` headers,
  `IncomingMessage`/`ServerResponse` constructible (srvx extends them), `zlib.crc32`, legacy `Z_*`, dns result order,
  `tls.convertALPNProtocols`, `module` paths/`createRequire` (MODULE_NOT_FOUND), child_process ENOENT on every API.
  Named ESM exports for loop-built modules listed in `node::exports`. Fixtures `node/{http,tls-dns-dgram,zlib,
  child-process,module-perf}.js` pass `scripts/diff.sh` (with crypto's two: 7/7).
- 2026-10-02: zigbee2mqtt probe with only `net` stubbed (`tasks/T65-files/`): 89.0 s, 32.5 MB, runs to `Opening TCP
  socket`. Bytes check vs 6548d5d: all existing fixtures identical. `cargo test --no-fail-fast`: only the 4 wac
  tests fail (no `wac`); `link::tests` now use `worker_threads` as the missing built-in. Docs, F280–F287, T66 handoff.
