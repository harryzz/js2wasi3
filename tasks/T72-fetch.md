# T72: global `fetch` over `wasi:http@0.3.0` (the HTTP client, F286)

Status: todo (handoff written 2026-10-03) | Depends on: T71 (the Fetch classes), T66

## Goal
node's global `fetch(input, init)` in a command, over wasmtime 49's `wasi:http@0.3.0` `client` (`-S http`): the host
does the connection, HTTPS and HTTP/1.1–2, so no TLS in JS. `Request`/`Response`/`Headers` are T71's classes (a
`Response` body is a `ReadableStream`); methods, headers, a request body (string, `Uint8Array`, `ArrayBuffer`,
`URLSearchParams`, a stream), redirects (`redirect: "follow"` default, as undici), `AbortSignal`, node's errors
(`TypeError: fetch failed` with a `cause`). For zigbee2mqtt: OTA firmware updates. The user's order (2026-10-03):
T71, this, then the TLS client (T73). Programs that don't use `fetch` keep their bytes.

## What we know (at handoff)
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
- [ ] `fetch` against a local server equals node (fixture), errors included.
- [ ] One HTTPS `fetch` to a public host equals node in its stable parts.
- [ ] zigbee2mqtt's OTA check for the test device works like node, or what's missing is recorded.
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] probe
- [ ] `fetch`
- [ ] fixtures, zigbee2mqtt OTA probe, docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written with T71's (`main` at `7f4d4ef`); `wasi:http@0.3.0` read from wasmtime v49.0.1.
