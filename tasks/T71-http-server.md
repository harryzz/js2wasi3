# T71: `http.createServer` over `net`, WebSocket upgrades (`ws`), web `Request`/`Response`: zigbee2mqtt's frontend (F286)

Status: done (2026-10-03) | Depends on: T66 (net), T60 (stream), T65 (http shape)

## Goal
An HTTP/1.1 server in a command, as node's `http` has it, over T66's `net.Server`: `http.createServer`, `Server`,
`IncomingMessage`, `ServerResponse` (`writeHead`, `setHeader`, `write`, `end`, chunked and `Content-Length` bodies,
keep-alive), and the `upgrade` event that `ws` uses for WebSockets. Then zigbee2mqtt's **frontend** (the web UI,
`frontend: enabled: true`) and its **onboarding** page work under js2wasi3. The user's order (2026-10-03): this
first, then `fetch` (T72), then the TLS client (T73). Programs that don't use `http` keep their bytes.

## What we know (at handoff)
- zigbee2mqtt 2.14.1's servers:
  - `dist/extension/frontend.js`: `http.createServer(onRequest)` (or `https.createServer` with `ssl_key`/`ssl_cert`:
    needs a TLS *server*, which `wasi:tls` doesn't have: out of scope, say so), `server.on("upgrade", …)` handing the
    socket to `new ws.WebSocketServer({ noServer: true })` → `handleUpgrade`, `server.listen(port[, host])` (a
    `host` starting with `/` is a Unix socket: unsupported, T66's IPC gap F290). The request handler serves the
    `zigbee2mqtt-windfront` files through `dist/util/staticFileServer.js`.
  - `dist/util/staticFileServer.js` uses `srvx/node` and `srvx/static` (srvx 0.12.8), which need the WHATWG Fetch
    classes: `Response` (23 uses), `Request` (11), `Headers` (8), `ReadableStream` (6), as globals. `new Proxy` is in
    srvx's `body-limit.mjs` and `loader.mjs` only: check that the frontend's path doesn't reach them (F152, no
    `Proxy`). F273 recorded srvx as a patch ("loaded only with the frontend").
  - `dist/util/onboarding.js`: an `http` server on first start (no `configuration.yaml`; port 8080), skipped with
    `Z2M_ONBOARD_NO_SERVER=1` in T62–T70's runs.
- `ws` (zigbee2mqtt's dependency): the server side needs `http`'s `upgrade`, `net.Socket` (`setNoDelay`, `setTimeout`,
  writes, `end`, `destroy`), `crypto.createHash("sha1")` (T65 has it) and `Buffer`; `perMessageDeflate` is off by
  default on a `WebSocketServer` (on: `zlib`, F284, missing).
- node's own server: `lib/_http_server.js` (44 KB), `_http_common.js`, `_http_incoming.js`, `_http_outgoing.js`,
  `internal/http.js` in node 24.21.0 (`process.binding("natives")`, as T60's `scripts/gen-node-streams.mjs` got
  streams). They parse with `internalBinding("http_parser")` (llhttp, C): js2wasi3 needs a JS `HTTPParser` with that
  binding's interface (`initialize`, `execute`, `finish`, the `kOnHeadersComplete`/`kOnBody`/`kOnMessageComplete`
  callbacks, `consume`/`unconsume`), written from RFC 9112. Porting node's `_http_*` over it (like T60) keeps node's
  behaviour (headers, keep-alive, `Expect: 100-continue`, timeouts, errors); hand-writing a smaller server is the
  alternative: decide in step 1 by size.
- `node:http` today (T65, `crates/frontend/node/http.js`, 299 lines): node 24's shape, `STATUS_CODES`, `METHODS`,
  `Agent`, header validation, `OutgoingMessage` headers; `createServer`/`request`/`get`/`Server`/`ClientRequest` throw.
  `https` (T65) likewise.
- `net.Server`/`net.Socket` (T66): `listen` (no host: `::`, else `0.0.0.0`), accepted sockets are `stream.Duplex`;
  `ref`/`unref` are no-ops (F289). Test clients in a fixture can be raw `net.connect` writing `GET / HTTP/1.1`
  until the HTTP client exists (T72 is `fetch` over `wasi:http`, not node's `http.request`).
- zigbee2mqtt runs: `tasks/T68-files/` (test-only config; the frontend is `enabled: false` there: a frontend test
  config needs `frontend: { enabled: true, port: … }` on a free local port, still test-only MQTT topics); the build
  needs `Z2M_DEVICES=1` when the test network has a device (the SNZB-02 joined 2026-10-03, F272).

## Plan
1. Decide port-vs-write for the server (`_http_server` & co. over a JS `HTTPParser` vs a smaller server); probe
   `ws`'s `handleUpgrade` path and srvx's static path for what else they touch.
2. `HTTPParser` (requests and responses, chunked, keep-alive, errors with llhttp's codes such as `HPE_INVALID_METHOD`)
   and `http.createServer`/`Server`/`IncomingMessage`/`ServerResponse`, `upgrade`/`connect` events, `server.close`,
   `closeAllConnections`, timeouts as node.
3. The Fetch classes srvx needs (`Headers`, `Request`, `Response`, `ReadableStream` as far as srvx uses it; node 24's
   own `lib/internal/webstreams/*` and `internal/deps/undici` are the reference). T72 reuses them for `fetch`.
4. Fixtures: `tests/fixtures/node/http-server.js` (server + raw `net` clients in one program, each step awaited:
   GET/POST, `Content-Length` and chunked bodies, keep-alive, 404, a malformed request's 400, an upgrade);
   `ws` echo (server and a raw upgrade handshake); `diff.sh`, `cargo test`, bytes check.
5. zigbee2mqtt with the frontend on (test config, `Z2M_DEVICES=1` if the device is still joined): the page and its
   WebSocket from a local client (`curl`, node's `ws` client), compared with node on the same config.
6. Docs, follow-ups (https server, Unix sockets, `perMessageDeflate`), next handoff (T72).

## Read first
- `tasks/T66-net-socket.md` (Notes / Result), `tasks/T60-stream.md` (how node's lib was generated),
  `crates/frontend/node/http.js`, `crates/frontend/node/net.js`, `docs/zigbee2mqtt-feasibility.md` (latest sections).

## Tests
The new fixtures with `scripts/diff.sh`; `node/net.js`, `node/http.js`; `cargo test`; `wasm-tools validate` + wasmtime;
bytes check of command fixtures against HEAD (only programs using `http`/`https` may change). The zigbee2mqtt
frontend run is a probe, recorded in Progress.

## Acceptance
- [x] `http.createServer` serves requests like node (fixture equals node), including an `upgrade`.
- [x] A `ws` WebSocket server echoes messages like node.
- [x] zigbee2mqtt's frontend loads and its WebSocket connects under js2wasi3 (probe), or what's missing is recorded.
- [x] Follow-ups updated.

## Notes / Result
**Decision** (step 1): port node 24.21.0's `_http_*` (like T60's streams) over a JS llhttp rather than write a smaller
server: node's behaviour (keep-alive, pipelining, 100-continue, upgrades, errors, timeouts) for ~1,000 lines of new
code. node's real binding (`node --expose-internals`) is the parser's oracle.

**What changed**
- `crates/frontend/node/http_parser.js` (`node:internal/http_parser`): `HTTPParser` (requests, strict) and
  `ConnectionsList`, llhttp's callbacks, results and error codes/reasons; `tasks/T71-files/parser-diff.js` compares it
  with node's binding (86 cases × every two-piece split: 4,073 runs equal).
- `scripts/gen-node-http.mjs` → `crates/frontend/node/http/` (`_http_common`, `_http_incoming`, `_http_outgoing`,
  `_http_server`, `internal/http`), over `node/http_support.js` and `node/http_errors.js`; `node/http.js` takes the
  server, `IncomingMessage`, `OutgoingMessage`, `ServerResponse`, header validation, `STATUS_CODES` from them
  (`Agent`/`ClientRequest`/`request`/`get` as T65 left them).
- `crates/frontend/node/fetch.js` (`node:internal/fetch`): `Headers`, `Request`, `Response`, `ReadableStream`
  (+ reader, controller), `Blob` as node globals (`node::GLOBALS`), also defined on the global object when loaded.
- Linker (`link.rs` `global_refs`): a read of `globalThis.X` (X in `node::GLOBALS`) loads X's module and becomes
  its binding, unless written or the module declares `X` at its top level (a CommonJS wrapper's scope counts).
- Lowering/prelude scan: `__intrinsic_species_mode()` gates in node's built-in modules (`Prelude::species`,
  `Program::is_node_module`); `node/buffer.js` defines `Buffer[Symbol.species]` (a data property) under it (ws).
- `net.js` fixes: a receive stream that ended was cancelled again on destroy (trap); `destroy()` dropped queued
  writes (the server's 400); exceptions of `data` listeners were taken as read errors. `fs.js`: `realpath` through
  directories above a preopen; `FileHandle.createReadStream`/`createWriteStream`, `close()` waits for them.
- Fixtures: `node/http-server.js`, `node/fetch-classes.js`, `commonjs/ws/` (ws 8.22.0 vendored, MIT).
- `tasks/T71-files/`: `probe.js` (binding probe), `parser-diff.js`, `srvx-probe.js`, zigbee2mqtt frontend run
  (`data/`, `patch.sh`, `client.cjs`, `build-run.sh`).

**Commands run**: `scripts/diff.sh` on `tests/fixtures/node` (all 45 incl. the new ones), `commonjs/ws`,
`tests/fixtures/globals`, `tests/fixtures/commonjs` + `tests/fixtures/modules` (29): all pass; `parser-diff.js
--splits`: 4,073/4,073; `cargo test`: all pass; `wasm-tools validate` + wasmtime on each new fixture (5 extra runs
of `http-server.js` identical); bytes check (release builds of every command fixture, HEAD vs tree): changed are
the `http` programs, `net.js`/`child-process-shell.js` (net fixes), the `fs` programs (realpath, FileHandle) and
`globals/guarded.js` (`Blob` now exists), as expected; test262 `built-ins/global`: no changes. zigbee2mqtt frontend
run (live test coordinator, test topics): client log equal to node's (see the feasibility doc).

**Unsupported** (follow-ups): F306 (https/TLS servers), F307 (HTTP client), F308 (parser gaps), F309 (Fetch class
gaps), F310 (other globals on the global object), F311 (onboarding page), F312 (debug build time of `http`
programs), F313 (async_hooks/diagnostics_channel/timers modules); updated F248, F252, F273, F286.

**Next smallest step**: T72, `fetch` over `wasi:http@0.3.0` with these `Request`/`Response` classes.

## Checklist
- [x] port-vs-write decision, probes
- [x] HTTPParser + server, upgrade
- [x] Fetch classes for srvx
- [x] fixtures, zigbee2mqtt frontend probe, docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written after T70 and the live-device test (`main` at `7f4d4ef`); zigbee2mqtt's frontend,
  onboarding and srvx needs read from the 2.14.1 package.
- 2026-10-03: started (`main` at `95d8d3b`); tools unchanged. zigbee2mqtt 2.14.1 set up in the session scratchpad
  (T62's `setup.sh`). Step 1, port-vs-write: **port** node 24.21.0's `_http_common`, `_http_incoming`,
  `_http_outgoing`, `_http_server`, `internal/http` (~120 KB) with a generator like T60's
  (`scripts/gen-node-http.mjs`); they need from the socket only what `net.Socket` has or can get cheaply
  (`_unrefTimer`, `_paused`, `destroySoon`, `cork`), and from node's internals `internalBinding('http_parser')`
  (`HTTPParser`, `ConnectionsList`: written in JS from RFC 9112 with llhttp's interface and error codes),
  async_hooks/trace/perf/diagnostics_channel (no-ops). The client half (`_http_client`, `_http_agent`) stays
  T65's (out of scope). srvx's static path: `staticMiddleware` → `fs/promises` `open`/`stat`/`realpath`,
  `FileHandle.stat`/`createReadStream` (F252: a `FileHandle` as a stream's fd is rejected: needed), `FastResponse`
  = srvx's `NodeResponse`, `NodeRequest`; `body-limit.mjs` (imported by `srvx/node`) has a `new Proxy` in a function.
- 2026-10-03: `node --expose-internals` + `require("internal/test/binding").internalBinding("http_parser")` gives
  node's real llhttp binding: the JS `HTTPParser` is checked against it in node (callbacks, return values, error
  codes, `bytesParsed`, every split point) before it runs in js2wasi3. Constants: REQUEST 1, RESPONSE 2,
  kOn* 0–6, kLenient* (All 1023, HeaderValueRelaxed 1024); `methods` (35) / `allMethods` (47, the index is the
  `method` number passed to kOnHeadersComplete).
- 2026-10-03: llhttp behaviour probed through the binding (`tasks/T71-files/probe.js`): `kOnHeadersComplete`
  gets `(major, minor, headers|undefined, methodIndex, url|undefined, undefined, undefined, upgrade, keepAlive)`;
  its result 1 = no body, 2 = upgrade now (the server returns 2 when it takes an upgrade); an upgrade otherwise
  ends after the body; `execute` returns the bytes parsed (upgrade: up to the end of the message) or an `Error`
  with `code`/`reason`/`bytesParsed`; trailers come through `kOnHeaders(trailers, url)` before
  `kOnMessageComplete`; >31 header pairs are flushed through `kOnHeaders`; `HPE_HEADER_OVERFLOW` counts URL +
  names + values against `maxHeaderSize` (default 16384); RTSP/ICE versions, HTTP/0.9 request lines, PRI.
- 2026-10-03: `crates/frontend/node/http_parser.js` written (requests; strict; `ConnectionsList`).
  `tasks/T71-files/parser-diff.js` (86 cases, every two-piece split: 4,073 runs) equals node's binding on callbacks,
  arguments, results and error codes/reasons; found and copied two binding quirks (after one `kOnHeaders` flush,
  later messages on that parser also get their headers through `kOnHeaders`; Transfer-Encoding then
  Content-Length is `HPE_INVALID_CONTENT_LENGTH`). With `--pos`, `bytesParsed` of 25 error cases is off by 1–8
  bytes (llhttp reports some errors at the span end/start): left (follow-up).
- 2026-10-03: `scripts/gen-node-http.mjs` writes `crates/frontend/node/http/` (patches: `Server` is a class
  extending T66's class `net.Server`, `super(…)` before `storeHTTPOptions`; no `_handle.readStart/readStop`; no
  `promisify`; primordials the shared shim lacks are local consts, so stream programs keep their bytes);
  `http_support.js` (`node:internal/http_support`: no-op async_hooks/perf/diagnostics/trace, `FreeList`, timers,
  `assignFunctionName`), `http_errors.js` (T60's errors + the HTTP codes, `ConnResetException`). `http.js` takes
  `Server`, `ServerResponse`, `IncomingMessage`, `OutgoingMessage`, `createServer`, `_connectionListener`, header
  validation, `STATUS_CODES` from them (client half still throws). First smoke test (GET over a raw `net`
  client, `Connection: close`) prints the same bytes as node.
- 2026-10-03: `tests/fixtures/node/http-server.js` (GET/POST, Content-Length and chunked bodies both ways,
  pipelined keep-alive, 404, HEAD, trailers, 204, missing Host → 400, malformed → 400, half-close, 100-continue,
  `clientError`, `upgrade` with head bytes, CONNECT without listener, response API errors) equals node
  (`diff.sh`; 5 more wasmtime runs the same). Fixes on the way: `net.Socket` cancelled a receive stream that
  had already ended (trap "handle is not a stream" when a socket is destroyed after its EOF: the reader is
  dropped at the end now); `destroy()` dropped queued writes (node's writes reach the kernel at once: the
  server's `400` before `destroy`): the handle now goes after the send stream took them; `HideStackFramesError`,
  `%s` of `undefined` in the HTTP error messages. Debug build of the fixture: 33 s (`http.js` fixture 21.6 →
  35 s, 1.08 → 1.64 MB: `http` loads the server modules eagerly; `net.js` alone is 24 s).
- 2026-10-03: `tests/fixtures/commonjs/ws/` (ws 8.22.0 vendored, MIT: `noServer` + `handleUpgrade` like
  zigbee2mqtt's frontend, a raw `net` client doing the handshake and sending masked text/close frames; 300-byte
  frame, `close(4401, …)`, a wrong path) equals node. Needed: `Buffer[Symbol.species]` (ws's
  `FastBuffer = Buffer[Symbol.species]`; js2wasi3's typed arrays have none, F248): `node/buffer.js` defines the
  getter when the program names `species` (`__intrinsic_named`); and `net.Socket`'s receive loop no longer
  catches what a `data` listener throws (it destroyed the socket as a read error, silently: ws's
  `TypeError` was invisible).
- 2026-10-03: bytes check (release builds of all command fixtures, HEAD vs tree, `$S/bytes.sh`): 30 changed;
  every `Buffer` program changed by the species block (an `if (__intrinsic_named(…))` isn't folded, and its getter
  function renumbers all functions). Now `if (__intrinsic_species_mode()) { … }` is a gate like the prelude's
  `__intrinsic_cjs_mode()`, allowed in node's built-in modules (`Program::is_node_module`;
  `Prelude::species`: the program names `species` outside node's modules), and the property is a data property
  (`value: Buffer`): `buffer`/`string-decoder` fixtures are byte-identical to HEAD again.
- 2026-10-03: `crates/frontend/node/fetch.js` (`node:internal/fetch`; `node::GLOBALS` `Headers`, `Request`,
  `Response`, `ReadableStream`, `ReadableStreamDefaultReader`, `ReadableStreamDefaultController`, `Blob`, and set on
  the global object when loaded, as srvx reads `globalThis.Headers`): Headers (sorted iteration, set-cookie, undici
  messages), Response/Request (body from string/bytes/Blob/URLSearchParams/stream/async iterable; text/json/
  arrayBuffer/bytes/blob/clone; statics error/json/redirect; prototypes list node's members in node's order),
  default `ReadableStream` (start/pull/cancel, reader, async iteration, `from`), `Blob`.
  `tests/fixtures/node/fetch-classes.js` equals node (body reads awaited one by one: tick counts differ from undici).
- 2026-10-03: srvx probe (`tasks/T71-files/srvx-probe.js`, run from a `t71probe/` dir inside zigbee2mqtt's
  package: zigbee2mqtt's `createStaticFileServer` over `http`, 12 raw requests: `/`, `.br` variant, HEAD, 404,
  directory index, Range 206, POST 404, bad `%` 400, `..`, `If-None-Match` 304) equals node. Needed:
  (1) the linker: a read of `globalThis.X` for X in `node::GLOBALS` (srvx's `globalThis.URL`, `.Headers`,
  `.Request`, `.Response` at load) loads X's module like a bare `X` and becomes its binding (not assignment
  targets; not when the module declares `X`); (2) `fs.realpath` walked `lstat` through the directories above a
  preopen (`/tmp` for `--dir /tmp/x`), which WASI can't see: they're taken as directories now; (3)
  `FileHandle.createReadStream`/`createWriteStream` (F252): a stream on the handle's fd that closes it, and
  `FileHandle.close()` waits for such a stream's close.
- 2026-10-03: zigbee2mqtt with the frontend on (`tasks/T71-files/`: `data/configuration.yaml` = T68's test-only
  config + `frontend: {enabled: true, host: 127.0.0.1, port: 18080}`; `patch.sh` = T66's without the srvx stub and
  with `import("zigbee2mqtt-windfront")` for the computed `import(settings…package)`; `client.cjs`: GET `/` plain
  and `br`, `/index.html`, `/missing`, `/device_icons/x.png`, a `.br` asset, WebSocket `/api` for 5 s with a
  health check; `build-run.sh`). First build failed: readable-stream's CommonJS files declare
  `const Blob = globalThis.Blob || …` (a wrapper-scope binding the `globalThis.X` rule must respect: now a
  declaration at the top level, or in a CommonJS wrapper's scope, keeps `globalThis.X` as it is; IIFE-local ones,
  as in srvx, don't). Then: release build 91.6 s / peak 774 MB, `wasmtime compile` 40.9 s; started like node
  (`Started frontend on port 127.0.0.1:18080`, `Zigbee2MQTT started!`) and **the client log equals node's**
  (pages, br assets, 404s, WebSocket open, topics bridge/converters … bridge/response/health_check, close).
- 2026-10-03: docs (design.md D4 note, keywords.md, README, zigbee2mqtt-feasibility.md, test262 HISTORY.md),
  follow-ups F306–F313, T72 handoff updated; task done.
