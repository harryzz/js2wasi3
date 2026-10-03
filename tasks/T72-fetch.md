# T72: global `fetch` over `wasi:http@0.3.0` (the HTTP client, F286)

Status: done (2026-10-03; handoff written 2026-10-03) | Depends on: T71 (the Fetch classes), T66

## Goal
node's global `fetch(input, init)` in a command, over wasmtime 49's `wasi:http@0.3.0` `client` (`-S http`): the host
does the connection, HTTPS and HTTP/1.1–2, so no TLS in JS. `Request`/`Response`/`Headers` are T71's classes (a
`Response` body is a `ReadableStream`); methods, headers, a request body (string, `Uint8Array`, `ArrayBuffer`,
`URLSearchParams`, a stream), redirects (`redirect: "follow"` default, as undici), `AbortSignal`, node's errors
(`TypeError: fetch failed` with a `cause`). For zigbee2mqtt: OTA firmware updates. The user's order (2026-10-03):
T71, this, then the TLS client (T73). Programs that don't use `fetch` keep their bytes.

## What we know (at handoff)
- T71 (done 2026-10-03) left the classes in `crates/frontend/node/fetch.js` (`node:internal/fetch`, node globals
  via `node::GLOBALS`): `Headers`, `Request`, `Response` (bodies as bytes or a `ReadableStream`; internals under a
  `kState` symbol), default `ReadableStream`, `Blob`. `fetch` belongs there; a response body from a WASI
  `stream<u8>` can be a `ReadableStream` with a `pull` reading the stream (T66's async iterable of `Uint8Array`s).
  Gaps to check against `ota.js`: F309. `globalThis.fetch` reads become the binding if `fetch` is added to
  `node::GLOBALS` (T71's linker rule).
- zigbee-herdsman's `dist/controller/helpers/ota.js` is zigbee2mqtt's `fetch` user (none in `dist/` or
  zigbee-herdsman-converters' `lib/`): `const response = await fetch(url)`, then `response.ok`, `response.body`
  (non-null check), `response.status`, `await response.json()` (the OTA index) and
  `Buffer.from(await response.arrayBuffer())` (the firmware image). URLs are HTTPS (GitHub-hosted indexes).
- `wasi:http@0.3.0` (wasmtime v49.0.1 `crates/wasi-http/src/p3/wit/deps/http.wit`, 509 lines): interface `types`
  (`fields` with `from-list`, `get`, `set`, `append`, `delete`, `copy-all`, `clone`; `request.new(headers,
  contents: option<stream<u8>>, trailers: future<…>, options)` and `set-method`/`set-scheme`/`set-authority`/
  `set-path-with-query`; `request-options` with connect / first-byte / between-bytes timeouts; `response` with
  `get-status-code`, `get-headers`, `consume-body(this, future) -> tuple<stream<u8>, future<result<option<trailers>,
  error-code>>>`; a large `error-code` variant (DNS, connection refused, TLS errors, …), interface `client` with
  `send: async func(request) -> result<response, error-code>`, and `handler` (exports, for `wasmtime serve`: not this
  task). Vendor it in `crates/component/wit/` next to `wasi-sockets-0.3.1.wit`, list `wasi:http/types@0.3.0` and
  `wasi:http/client@0.3.0` in `COMMAND_WASI_IMPORTS`, make them non-blocking like `wasi:sockets` (`main.rs`).
- WIT shapes that matter: resources (handles as numbers, T59), `async func` → promise, `stream<u8>` ↔ async iterable,
  `future` ↔ promise (T66), a large `variant` for `error-code` (T20's `{tag, val}`); `__intrinsic_import_cancel` (T67)
  for an aborted request.
- node's `fetch` is undici; its error for a failed request is `TypeError: fetch failed` with `cause` (an `Error` with
  `code`, e.g. `ENOTFOUND`, `ECONNREFUSED`): map `error-code` cases to those. node's `Response.body` is a WHATWG
  `ReadableStream`; `json()`, `text()`, `arrayBuffer()`, `blob()` (no `Blob` global yet: F262's list), `bytes()`.
- wasmtime needs `-S http` (and `-S inherit-network`): add it to `scripts/diff.sh`.
- Testing: a local server for deterministic fixtures: T71's `http.createServer` in the same program (fetch to
  `http://127.0.0.1:<port>`), and node's `http` as the oracle's server; a public HTTPS host is allowed for one fixture
  (user, 2026-10-03), printing only stable parts.

## Plan
1. Probe: a command importing `wasi:http/client` directly: GET a local server and a public HTTPS URL, read the body
   stream, an error case (connection refused, DNS failure): the `error-code` cases.
2. `fetch` in the prelude or a node module (global, bound by the linker like `URL`, T64): build the `wasi:http`
   request from a `Request`, send, wrap the response (status, headers, body stream) in a `Response`; redirects,
   `AbortSignal`, timeouts, node's errors.
3. Fixtures: `tests/fixtures/node/fetch.js` (a T71 server and `fetch` in one program, each step awaited: GET, POST
   JSON, headers, 404, a redirect, chunked body, abort, refused); one public HTTPS fixture; `diff.sh`, `cargo test`,
   bytes check.
4. zigbee2mqtt: an OTA index fetch (the `bridge/request/device/ota_update/check` request for the joined test device,
   on the test topic), compared with node.
5. Docs, follow-ups (`http.request`/`https.request` over `wasi:http` or `net`, `Blob`/`FormData`), next handoff (T73).

## Read first
- `tasks/T71-http-server.md` (its Notes / Result: the Fetch classes), `tasks/T66-net-socket.md` (Notes / Result),
  `tasks/T64-url-text-dataview.md` (globals bound by the linker), `crates/component/src/lib.rs`.

## Tests
The new fixtures with `scripts/diff.sh`; T71's fixtures; `cargo test`; `wasm-tools validate` + wasmtime; bytes check
against HEAD (only programs using `fetch` may change).

## Acceptance
- [x] `fetch` against a local server equals node (fixture), errors included.
- [x] One HTTPS `fetch` to a public host equals node in its stable parts.
- [x] zigbee2mqtt's OTA check for the test device works like node, or what's missing is recorded (the device has no
  OTA cluster: F316; herdsman's OTA downloads equal node's).
- [x] Follow-ups updated.

## Notes / Result
**What changed**
- `crates/component/wit/wasi-http-0.3.0.wit`: wasmtime v49.0.1's `wasi:http@0.3.0` (interfaces only; its
  `use wasi:clocks/types@0.3.0` points at the vendored 0.3.1). `COMMAND_WASI_IMPORTS` lists `wasi:http/types` and
  `wasi:http/client`; `main.rs` makes `wasi:http/*` non-blocking like `wasi:sockets/*`. Nothing else was needed in
  the compiler: sync `request.new`/`consume-body` with `stream`/`future` arguments and results worked as is.
- `crates/frontend/node/wasi_http.mjs` (`node:internal/wasi_http`, ES module) and `fetch_client.js`
  (`node:internal/fetch_client`): `fetch` as a node global (`node::GLOBALS`, also `globalThis.fetch`). undici's default
  request headers (not `accept-encoding`), `content-length` for byte bodies (and `0` for an empty POST/PUT), streamed
  bodies, the Fetch standard's redirects (`follow`/`manual`/`error`, 20 at most, 303 and 301/302 POST → GET with the
  body headers dropped, credentials dropped cross-origin), `AbortSignal` (`__intrinsic_import_cancel` of the pending
  `send`, then the body's read), the body as a `ReadableStream` whose first read calls `consume-body` (an unread body
  keeps nothing waiting: node exits too), undici's errors (`TypeError: fetch failed`, `cause` = node's error:
  `ECONNREFUSED`, `ENOTFOUND`, `EAI_AGAIN`, `EHOSTUNREACH`, undici's `SocketError`/`ConnectTimeoutError`/…; a body
  cut short: `TypeError: terminated`). `statusText` is the standard phrase (`builtin_data.statusCodes`).
- `fetch.js` (T71): `Response.redirected` is state, `Request` requires `duplex` for a stream body (node's message),
  `kState`/`kAppend` exported for `fetch_client`.
- `scripts/diff.sh` and the zigbee2mqtt run scripts (T68, T69, T71): wasmtime `-S http`.
- Fixtures: `tests/fixtures/node/fetch.js` (a local server, each step awaited), `tests/fixtures/node/fetch-https.js` (public
  hosts: raw.githubusercontent.com, github.com, example.com). `tasks/T72-files/`: `probe.js` (`wasi:http` directly),
  `ota-index.cjs` + `run-ota.sh` (zigbee-herdsman's OTA helpers under node and js2wasi3).

**Commands run**: `scripts/diff.sh` on `node/fetch.js` (4 runs), `node/fetch-https.js`, `node/fetch-classes.js`,
`node/http-server.js`, `node/http.js`, `node/net.js`, `commonjs/ws`, `globals/`: all pass; `cargo test`: all pass;
`wasm-tools validate` + wasmtime on the new fixtures; bytes check (release builds of every command fixture, HEAD vs
tree): changed are `commonjs/ws`, `globals/guarded.js` (names `fetch`) and `node/fetch-classes.js` (T71's classes),
plus the new fixtures. `tasks/T72-files/run-ota.sh`: herdsman's index download, a firmware download with its sha512
check and header parse, a bad hash, a 404 index and a DNS failure print node's output. zigbee2mqtt (T68's
`build-run.sh`, live test coordinator, MQTT test topics): builds (148 s, 767 MB; compile 63 s) and starts like node.
test262: not run (`fetch` isn't ECMAScript).

**Unsupported** (follow-ups): F314 (`listen(port)` binds `::` v6-only under wasmtime: IPv4 clients refused), F315
(`fetch` gaps: content decoding, reason phrases, hop-by-hop headers, `data:` URLs, proxies, options), F316 (the live
OTA check: the test device has no OTA cluster). Debug builds of `fetch` programs with an `http` server are as slow as
T71's (F312: `fetch.js` 31 s in `diff.sh`); `fetch` alone adds ~1 s / 150 KB to a release build over the Fetch classes.
Programs that use `fetch` need wasmtime's `-S http` (and `-S inherit-network` for the host's connections).

**Next smallest step**: T73, `tls.connect` over `wasi:tls@0.3.0-draft` (F281).

## Checklist
- [x] probe
- [x] `fetch`
- [x] fixtures, zigbee2mqtt OTA probe, docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written with T71's (`main` at `7f4d4ef`); `wasi:http@0.3.0` read from wasmtime v49.0.1.
- 2026-10-03: started. `wasi:http@0.3.0` vendored from wasmtime v49.0.1 (`crates/component/wit/wasi-http-0.3.0.wit`:
  its `use wasi:clocks/types@0.3.0` changed to 0.3.1, the one clocks version vendored; its worlds dropped), listed in
  `COMMAND_WASI_IMPORTS`, non-blocking like `wasi:sockets` (`main.rs`).
- 2026-10-03: probe (`tasks/T72-files/probe.js`, `wasmtime run -Sp3 -S http -S inherit-network`): built as is (sync
  `request.new`/`consume-body` with future arguments and results work in the non-blocking boundary). GET to a local
  node server: status, headers, body stream, trailers, `done` all right; refused → `{tag: "connection-refused"}`,
  unknown host → `{tag: "DNS-error", val: {rcode: "address not available"}}` (as `payload` of the thrown error).
- 2026-10-03: HTTPS GET of raw.githubusercontent.com's zigbee-OTA index.json (597,164 bytes) through the probe: 0.9 s.
- 2026-10-03: `fetch` written: `crates/frontend/node/fetch_client.js` (`node:internal/fetch_client`, `fetch` in
  `node::GLOBALS`) over `node:internal/wasi_http` (ES module), apart from `node:internal/fetch` so Headers-only
  programs don't import `wasi:http`. `fetch.js`: `redirected` state, the `duplex` check, `kState`/`kAppend` exported.
  `diff.sh`: `-S http`. Found: a server's `listen(port)` binds `::` v6-only under wasmtime (WASI 0.3 has no
  `ipv6-only` option), so `127.0.0.1` is refused (node: dual-stack): follow-up; fixtures listen on 127.0.0.1.
- 2026-10-03: `tests/fixtures/node/fetch.js` (T71 server + `fetch` in one program: GET, POST JSON, 404, 302→301
  chain, 303, 307, manual/error redirects, redirect loop, chunked reads, bytes, form, stream body + duplex, Request
  input, HEAD, 204, unread body, aborts before/while waiting/timeout/while reading, body cut off, refused, URL/scheme/
  method errors): equal to node after two fixes (response `url` without the fragment; hyper's `HTTP-protocol-error`
  for a body cut short is undici's `terminated` / `SocketError: other side closed`). 31 s in diff.sh: debug build
  of an `http` server program (F312); release build 8 s, `wasmtime compile` 2.5 s.
- 2026-10-03: `tests/fixtures/node/fetch-https.js` (raw.githubusercontent.com's OTA index, an http→https redirect
  from github.com with HEAD, example.com bytes, a DNS failure): equal to node. T71's fixtures (`fetch-classes`,
  `http-server`, `http`, `net`, `commonjs/ws`, `globals/`): pass. `cargo test`: all pass.
- 2026-10-03: zigbee2mqtt OTA: the test device (EFEKTA SNZB-02, `data-dev`) has no `genOta` output cluster (out
  clusters 6, 1026, 1029), so `ota_update/check` stops at herdsman's "No endpoint found with OTA cluster support"
  before any `fetch`, on node too. Instead `tasks/T72-files/ota-index.cjs` + `run-ota.sh` drive zigbee-herdsman
  10.9.2's `ota.js` (zigbee2mqtt's only `fetch` user): `getOtaIndex` (the 600 KB index), `getOtaFirmware` of the
  smallest image (37,634 bytes, sha512 checked, `parseOtaImage`), a bad hash, a 404 index, a DNS failure: output
  equal to node's (timestamps masked). Build 50 s (release).
- 2026-10-03: zigbee2mqtt's build names `fetch` (herdsman `ota.js`), so it now imports `wasi:http` and wasmtime
  refuses it without `-S http` (checked on a small program): `-S http` added to T68's `build-run.sh`/`restart-run.sh`,
  T69's `perf-run.sh`, T71's `build-run.sh`. Docs: design.md (D4 note), keywords.md (`fetch` row), README;
  follow-ups F314–F316, F286/F309 updated. Running: the bytes check (HEAD vs tree) and T68's live run.
- 2026-10-03: zigbee2mqtt (T68's `build-run.sh`, live test coordinator + MQTT test topics, with `-S http`): the build
  now imports `wasi:http/types` + `client` (herdsman's `ota.js` loads `fetch_client` at start); release build 148 s /
  767 MB, `wasmtime compile` 63 s; the run starts like node (backup, MQTT, HA discovery, `Zigbee2MQTT started!`).
- 2026-10-03: bytes check done (only `commonjs/ws`, `globals/guarded.js`, `node/fetch-classes.js` + the new fixtures changed). Notes / Result, checklist, README status, follow-ups F314–F316, HISTORY.md, feasibility doc, T73 handoff note written. Done.
