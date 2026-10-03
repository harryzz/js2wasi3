# T73: `tls.connect` over `wasi:tls` (TLS client, F281)

Status: todo (handoff written 2026-10-03; renumbered from T71 after the user put the HTTP server and `fetch` first) | Depends on: T66 (net), T65 (tls shape); after T71, T72

## Goal
`node:tls` client connections (F281, next in F288's order) over the host's TLS: wasmtime 49's experimental
`wasi:tls@0.3.0-draft` client (`-S tls`), on top of T66's TCP sockets. `tls.connect(port, host[, options][, cb])` /
`tls.connect(options)`, `TLSSocket` as a `stream.Duplex` (data, `end`, `close`, `secureConnect`, `error`),
`servername`, `authorized`/`authorizationError` where the host tells us, node's errors for a failed handshake.
Then `mqtts`/`wss` clients can follow (`fetch` doesn't need it: T72 uses `wasi:http`, where the host does HTTPS). Programs that don't use `tls`
keep their bytes.

## What we know (at handoff)
- `wasi:tls@0.3.0-draft` (wasmtime v49.0.1 `crates/wasi-tls/src/p3/wit/deps/tls/`): interface `types` (`resource
  error { to-debug-string: func() -> string }`) and interface `client`:
  ```wit
  resource connector {
    constructor();
    send: func(cleartext: stream<u8>) -> tuple<stream<u8>, future<result<_, error>>>;
    receive: func(ciphertext: stream<u8>) -> tuple<stream<u8>, future<result<_, error>>>;
    connect: static async func(this: connector, server-name: string) -> result<_, error>;
  }
  ```
  A connector is a TLS engine between streams: TCP's `receive` stream → `receive(ciphertext)` → cleartext; JS's
  cleartext → `send(cleartext)` → ciphertext → TCP's `send`; `connect(server-name)` runs the handshake. No server
  side, no client certificates, no ALPN, no session resumption, no peer certificate access.
- Trust: wasmtime's rustls provider verifies against `webpki_roots::TLS_SERVER_ROOTS` (Mozilla's roots compiled in;
  `src/providers/rustls.rs`), no way to add a CA or skip verification: `rejectUnauthorized: false`, `ca`, `cert`/`key`,
  `checkServerIdentity` can't be honoured (say so with node-like errors or documented deviations).
- js2wasi3 side: WASI interfaces a command may import are vendored WIT in `crates/component/wit/` and listed in
  `crates/component/src/lib.rs` `COMMAND_WASI_IMPORTS` / `WASI`; `wasi:sockets/*` are non-blocking
  (`crates/js2wasi3/src/main.rs`, the `starts_with("wasi:sockets/")` partition): `wasi:tls/*` must be too. T66's
  `stream<u8>` ↔ async iterable and `future` ↔ promise mappings, T59's resources as handle numbers, T67's
  `__intrinsic_import_cancel` (a pending call borrowing a resource) all apply. Passing the TCP `receive` stream straight
  into `connector.receive` (host to host, no JS copy) would need a stream handle to be passed through without
  lifting: check whether the boundary can do that, else pump through JS.
- `node:tls` today (T65): loads with node 24's shape, `connect` throws (`crates/frontend/node/tls.js`);
  `tls.rootCertificates` is empty. `net.Socket` (T66, `crates/frontend/node/net.js`) is the model for a Duplex over
  WASI streams.
- wasmtime needs `-S tls` (with `-S inherit-network` for the TCP part); `scripts/diff.sh` passes the network flags
  since T66: add `-S tls` there.
- Testing (user, 2026-10-03: a fixture may reach a public host): a local TLS server (node's own `tls.createServer` with a self-signed certificate) can only test the
  *failure* path: wasmtime won't trust it, and node's client rejects it too (`self-signed certificate`,
  `DEPTH_ZERO_SELF_SIGNED_CERT`) unless told otherwise: compare those errors. A successful handshake needs a server
  with a publicly trusted certificate: a fixture may reach a public host (user's decision); pick one whose reply is
  stable (e.g. an HTTPS server's status line), print only what doesn't change.
- First check (not done): that wasmtime 49's CLI links the p3 `wasi:tls` imports under `-S tls` for a component
  (the p2 version exists too).

## Plan
1. Probe: a command importing `wasi:tls` directly (vendor the WIT, list it): TCP connect, connector, pump both ways,
   `connect("<host>")`, send a request, read the reply; and against a local self-signed server: the error's
   `to-debug-string`.
2. `node:tls` client: `TLSSocket` (Duplex) over `net.Socket`'s TCP plus a connector; `tls.connect` forms, events,
   `getProtocol()`/`authorized` as far as the host tells us; node's errors (map the host's handshake errors to node's
   codes where it's clear, else `ERR_SSL_*`-style with the host's text); unsupported options documented.
3. Fixtures: the local failure path and the public-host success path in `tests/fixtures/node/` (`diff.sh` with
   `-S tls`). `cargo test`, bytes check (programs without `tls` unchanged).
4. Docs (README, design.md D4/T66 paragraph, keywords.md), follow-ups, next handoff (F288's order is then done).

## Read first
- `tasks/T66-net-socket.md` (Notes / Result), `tasks/T67-dgram.md` (Notes / Result: `import_cancel`),
  `crates/frontend/node/net.js`, `crates/frontend/node/tls.js`, `crates/frontend/node/wasi_sockets.mjs`,
  `crates/component/src/lib.rs` (`COMMAND_WASI_IMPORTS`, `WASI`).

## Tests
The new fixtures with `scripts/diff.sh`; `node/net.js`, `node/tls-dns-dgram.js`; `cargo test`; `wasm-tools validate`
+ wasmtime; bytes check against HEAD for programs without `tls`.

## Acceptance
- [ ] `tls.connect` to a host with a publicly trusted certificate exchanges data like node (a fixture).
- [ ] A handshake failure (self-signed local server) gives node's error shape.
- [ ] Unsupported options (no CA/verification override, no client certs, no ALPN, no server) documented; follow-ups.

## Notes / Result
(to be written)

## Checklist
- [ ] probe
- [ ] `node:tls` client
- [ ] fixtures, docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written after T70 and the live-device test (`main` at `56236e3`); WIT and trust roots read
  from wasmtime v49.0.1's `crates/wasi-tls`.
