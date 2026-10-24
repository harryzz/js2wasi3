# T153: `crypto` Hash/Hmac/Cipheriv/Decipheriv are streams; small gaps (`getCipherInfo`, `getCurves`, `getFips`, `randomUUIDv7`, ...)

Status: done (2026-10-24) | Depends on: T152 | Local; ask before long runs and before every push

## Goal
Compat check after T152 (user: "next compatibility fixes for crypto", then "yes" to items 1+2): node's `Hash`, `Hmac`, `Cipheriv`, `Decipheriv` are `stream.Transform`s;
ours have `update`/`digest`/`final` only, so `readable.pipe(crypto.createHash('sha256'))` fails (`dest.on is not a function`), the standard checksum and file-encryption idiom.
1. Stream interface (`write`/`end`/`pipe`/`pipeline`, `on('data'|'readable'|'finish')`, `read()`, `setEncoding`, `digest()` after `end()`), without linking `stream` into programs that don't use it if it is large (lazy).
2. Small gaps: `getCipherInfo`, `getCurves`, `getFips`/`setFips`/`fips`, `secureHeapUsed`, `randomUUIDv7`, node-style `Hash` options; stale `docs/compat/partial.json` notes.

## Plan
- [x] Probe node 24.21.0 (stream behaviour of each class, errors, `getCipherInfo` shapes, `getCurves`, `randomUUIDv7`).
- [x] Streams for the four classes; measure the `.wasm` growth of a `crypto` user.
- [x] Small gaps; fixtures `crypto-stream.js`, `crypto-misc.js` against node.
- [x] Docs: compat page (`node scripts/compat.mjs`), `partial.json`, README, design.md, keywords.md, tasks/README.

## Progress
- 2026-10-24: started.
- 2026-10-24: streams done. node's source is readable with `process.binding('natives')['internal/crypto/hash']` etc. (v24.21: `LazyTransform` only sets `decodeStrings = false`; `Hash._transform` calls the handle's update, `_flush` pushes the cached digest;
  `Hash.digest()` after `end()` works because `_flush` doesn't mark the JS state finalized; errors from `_flush` come out as thrown/uncaught from a tick, so the fixture doesn't test them).
  Implementation: `node:internal/crypto_streams` (LazyTransform port) linked by a new `EXT_LIBS` trigger kind (`link.rs`: "a given module is in the graph", here `node:stream`) so crypto-only programs don't pay
  the stream module (~900 KB: 825 KB → 1.74 MB for a `crypto` + `stream` program); `crypto.js` sets the prototypes of Hash, Hmac and CipherBase to LazyTransform's when it's there.
  `tests/fixtures/node/crypto-stream.js` (pipe, events, setEncoding, write with encodings, pipeline callback/promise, async iterator, options, cbc/gcm/chacha through pipes) equals node; `fs.createReadStream(f).pipe(hash)` too.
- 2026-10-24: small gaps done: `getCipherInfo` (table of the ciphers we have, by name/alias/nid, `keyLength`/`ivLength` options as node's), `getCurves` = `[]` (none claimed until F493), `getFips` 0, `setFips(false)` no-op, `setFips(true)`
  throws, the `fips` accessor, `secureHeapUsed`, `randomUUIDv7`. `tests/fixtures/node/crypto-misc.js` equals node. `docs/compat/partial.json` notes refreshed.

## Notes / Result
Done (2026-10-24). Streams: `crypto_streams.js` + a "module present" trigger in `EXT_LIBS`; small gaps in `crypto.js`. Tests: `crypto-stream.js`, `crypto-misc.js`, and the older crypto fixtures equal node; `fs.createReadStream(f).pipe(createHash(..))` checked by hand.
Not done: node's error paths through the stream (a decipher's bad padding or a hash written after `digest()` surface as uncaught errors from a tick in node; not compared), Hmac misuse (`end()` then `digest()` returns uninitialized memory in node 24.21: not copied), `Hash.copy()` after `end()` (node: `ERR_OSSL_EVP_UPDATE_ERROR`; ours works).
