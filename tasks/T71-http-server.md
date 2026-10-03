# T71: `http.createServer` over `net`, WebSocket upgrades (`ws`), web `Request`/`Response`: zigbee2mqtt's frontend (F286)

Status: todo (handoff written 2026-10-03) | Depends on: T66 (net), T60 (stream), T65 (http shape)

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
- [ ] `http.createServer` serves requests like node (fixture equals node), including an `upgrade`.
- [ ] A `ws` WebSocket server echoes messages like node.
- [ ] zigbee2mqtt's frontend loads and its WebSocket connects under js2wasi3 (probe), or what's missing is recorded.
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] port-vs-write decision, probes
- [ ] HTTPParser + server, upgrade
- [ ] Fetch classes for srvx
- [ ] fixtures, zigbee2mqtt frontend probe, docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written after T70 and the live-device test (`main` at `7f4d4ef`); zigbee2mqtt's frontend,
  onboarding and srvx needs read from the 2.14.1 package.
