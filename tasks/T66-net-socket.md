# T66: `net.Socket` (TCP client) over `wasi:sockets` in the event loop (F268)

Status: done (2026-10-02) | Depends on: T65

## Goal
zigbee2mqtt (T62–T65) now builds with only `net` stubbed and runs to `Opening TCP socket with 127.0.0.1:6638`,
where zigbee-herdsman's z-stack adapter does (`znp.js` `openSocketPort`):
`new net.Socket()`, `setNoDelay(true)`, `setKeepAlive(true, 15000)`, `writer.pipe(socket)`, `socket.pipe(parser)`,
listeners for `connect`, `ready`, `close`, `error`, then `socket.connect(port, host)`. mqtt's `tcp` transport
uses `net.createConnection`. Provide `node:net` so that:
1. With nothing listening, the run ends like node's: `connect ECONNREFUSED 127.0.0.1:6638` through `error`, then
   `close` (zigbee2mqtt's "Error while opening socket" path), not the stub.
2. With a TCP server listening, data flows both ways as a `stream.Duplex` (node 24's stream code, T60) on the
   P3 event loop, without blocking timers.

## What we know (at handoff)
- `wasi:sockets@0.3.1` (`probes/wit/deps/wasi-sockets-0.3.1/package.wit`): `tcp-socket.create(family)`,
  `connect: async func(ip-socket-address) -> result<_, error-code>`, `send: func(stream<u8>) ->
  future<result<…>>`, `receive: func() -> tuple<stream<u8>, future<result<…>>>`; `ip-name-lookup` for host names.
  wasmtime needs `-S inherit-network` (the T64/T65 probe passes it).
- T59's direct WASI imports from a built-in ES module (`wasi_fs.mjs`, T65's `wasi_random.mjs`) *block*: an
  `async func` is sync-lowered and streams are read/written whole around the call. A socket needs the
  non-blocking side: `connect` as a subtask the event loop waits on, `receive`'s `stream<u8>` read in chunks as
  data arrives (T20 slice 6b has a stream reader/pump in the prelude for user worlds: `builtins/stream.js`), and
  `send` fed from `_write`. Find out first how much of T20's async machinery a built-in module can use.
- T65's `http`, `tls`, `dns` throw "isn't available"; `tls.TLSSocket`/`http` would sit on top of this later
  (F281, F286). `dns.lookup` over `ip-name-lookup` is F283.

## Plan
1. Probe: a command that connects to a closed port and to a listening one (`nc -l` or a node server outside
   wasmtime) with the WIT calls directly; see the error codes (`connection-refused`) and stream behaviour.
2. `node:net`: `Socket` (Duplex: `connect`, `write`/`end`, `data`/`end`/`close`, `setNoDelay`/`setKeepAlive`/
   `setTimeout` as far as WASI allows, `destroy`, `remoteAddress`/`remotePort`), `createConnection`/`connect`,
   `isIP`/`isIPv4`/`isIPv6`; `Server`/`createServer` throwing unless cheap. node's errors (`ECONNREFUSED`).
3. Fixtures: `node/net.js` against node with a closed port (no server needed) and, if `diff.sh` can host one,
   an echo server.
4. Probe: `T63_GLOBALS=0 tasks/T65-files/build-run.sh` without the `net` stub (`mods=""`): the run should end
   with node's `Error while opening socket`/ECONNREFUSED output.

## Read first
- `tasks/T65-load-only-builtins-crypto.md` (Notes / Result), `tasks/T59-fs.md` (WASI imports from a built-in),
  `tasks/T20-wit-async-user-worlds.md` (slices 6a/6b: futures and streams), `tasks/T60-stream.md`.

## Tests
`node/net.js` through `scripts/diff.sh`; `cargo test`; the zigbee2mqtt probe.

## Acceptance
- [x] A refused connection matches node; a connection to a listening server exchanges data.
- [x] zigbee2mqtt builds with no built-in stubs and fails like node with no coordinator.
- [x] Follow-ups updated.

## Notes / Result
**What changed**
- `wasi:sockets/*` imports are non-blocking in a command (`main.rs`; the other WASI interfaces stay blocking,
  T59): `async func` → promise, returned `stream<u8>` → async iterable of `Uint8Array`s, `stream<u8>` argument ←
  async iterable, `future` → promise. `lower`: `Boundary.waits` lets a command import `async func`s.
- Backend (`command_waits`: a command with non-blocking imports): a library's pending-call globals after the host
  globals (`Cx::lib_globals`, `lib_base`), `run`'s waitable set shared (`abi::G_WS`), the callback applies
  import/stream events (`abi::Helper::Event`) and runs the jobs (`host::Helper::Jobs`), `event_loop` waits while
  `G_PENDING_N > 0`; blocking stream writes are counted there too. Each boundary type's sync
  `[stream-cancel-read-N]` is imported (after the payload intrinsics) for `StreamOp::CancelRead`
  (`__intrinsic_stream_cancel_read`): leave the set, cancel, handle the status as the read's event.
- Prelude: `$$StreamReader` returns the chunks generator (`$$StreamChunks`) with `cancel()`.
- `crates/frontend/node/`: `wasi_sockets.mjs` (`node:internal/wasi_sockets`), `net_support.js` (`node:internal/net`:
  `isIP*`, address formatting/parsing, node's errors for WASI error codes, `lookupAll`), `net.js` (`Socket` as a
  `stream.Duplex`: lookup, `connectionAttempt`/`connectionAttemptFailed`/`lookup`/`connect`/`ready` events, writes
  queued into the `send` stream, `receive` pushed with backpressure, `read(0)` after EOF like node, `close` after
  `setImmediate`, `setTimeout`, `setKeepAlive`, `destroy`; `Server`: bind, listen (no host: `::`, else
  `0.0.0.0`), accept, `close` waits for connections, EADDRINUSE as node; `connect`/`createConnection`/
  `createServer`; `BlockList`/`SocketAddress`/`BoundSocket` throw). `dns.lookup`/`dns.promises.lookup` over
  `lookupAll` (IP strings and `""` without a lookup, `family`, `all`, `order`). `node::exports` lists `net`.
- `scripts/diff.sh`: wasmtime gets `-S inherit-network -S allow-ip-name-lookup`.
- Fixtures: `tests/fixtures/node/net.js` (server and clients in one program, each step awaited: refused, echo with
  half-close, EADDRINUSE, timeout, destroy, `close` twice), `tests/fixtures/node/dns-lookup.js`.
- `tasks/T66-files/` (`patch.sh` with nothing stubbed, `build-run.sh` with `-S allow-ip-name-lookup`).
- Docs: README, design.md (D4 note), keywords.md, zigbee2mqtt-feasibility.md. `build.rs`'s missing-built-in test
  uses `worker_threads`.

**Commands run**: `scripts/diff.sh` on `node/net.js` (3 runs), `node/dns-lookup.js`, `node/tls-dns-dgram.js`,
`node/http.js`: pass. Bytes check against 9f0e67a (all command fixtures): only the programs that use `dns`
differ (`dns` now imports `wasi:sockets`). `cargo test`: all pass except the 4 `*_compose_with_wac` tests (no `wac`
in this container). zigbee2mqtt (`T63_GLOBALS=0 tasks/T66-files/build-run.sh`, release): build 93.8 s / 32.6 MB,
`wasmtime compile` 28.6 s, run ends like node: `connect ECONNREFUSED 127.0.0.1:6638`, `Error while opening socket`.

**Unsupported** (follow-ups): F289 (`ref`/`unref`, `setNoDelay`, keep-alive delay, backlog, `localAddress`, happy
eyeballs), F290 (IPC, `BlockList`, handles), F291 (DNS resolver), F292 (future cancel; `close` timing).
A program that imports `dns` or `net` needs `-S inherit-network` (and `-S allow-ip-name-lookup`) to use them.

**Next smallest step**: T67, `dgram` over `wasi:sockets` UDP (F282, F288's order: then the TLS client and the HTTP
client).

## Checklist
- [x] probe
- [x] `node:net`
- [x] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-02: handoff written after T65.
- 2026-10-02: probe (`$S/t66/p3.js`, raw `wasi:sockets/types` imports, node echo server outside wasmtime): a
  command's WASI imports were all blocking (T59), and an `async func` import in a command was rejected ("nothing
  would wait for it"). Design: **`wasi:sockets/*` imports are non-blocking** in commands (`main.rs`: the other WASI
  interfaces stay blocking): `async func` → promise, returned `stream<u8>` → async iterable (T20's
  `$$StreamReader`), `stream<u8>` argument ← async iterable (`$$StreamPump`), future → promise.
  Backend: a command with non-blocking imports (`command_waits`) gets `abi::LIB_GLOBALS` after its host globals
  (`Cx::lib_globals`, abi.rs uses it instead of `host_globals`), shares `run`'s waitable set, its callback applies
  import/stream events with `abi::Helper::Event` (its timer branch off: the host loop owns timers) and then runs
  the jobs (`host::Helper::Jobs`), and `Loop` waits while `G_PENDING_N > 0`. Blocking stream writes are counted
  too there (`stream_lower_sync`), since `stream_dispatch` uncounts every dropped writable end.
  Result: connect to a closed port rejects with `{tag: "connection-refused"}` while a timer fires; connect + send
  + receive against the echo server round-trips `echo:hello` and the program exits when the server closes.
- 2026-10-02: `server.close()`/`destroy()` hung: a read in progress on the `listen`/`receive` stream never ends
  (dropping the `tcp-socket` doesn't end it). Added `StreamOp::CancelRead` (`__intrinsic_stream_cancel_read`):
  a command that waits for imports imports each boundary type's sync `[stream-cancel-read-N]` (after the other
  payload intrinsics; `Cx::payload_cancel`), leaves the set, cancels and handles the status as the read's event.
  (The async cancel needs wasmtime's `component-model-more-async-builtins`: not used.) The prelude's
  `$$StreamReader` now returns the chunks generator (`$$StreamChunks`) with a `cancel()`.
- 2026-10-02: `crates/frontend/node/net.js` (Socket as Duplex, Server, connect/createConnection/createServer,
  isIP*), `net_support.js` (`node:internal/net`: isIP, address formatting/parsing, node errors from WASI error
  codes, `lookupAll`), `dns.lookup`/`dns.promises.lookup` over it; `diff.sh` runs wasmtime with
  `-S inherit-network -S allow-ip-name-lookup`. A hang that looked like a lost event was node semantics: a paused
  socket ends only after `read(0)` following `push(null)` (node's `onStreamRead`). Fixtures `node/net.js`
  (server + clients in one program: refused, echo, EADDRINUSE, timeout, destroy, close) and
  `node/dns-lookup.js` pass against node.
- 2026-10-02: zigbee2mqtt with no built-in stubs (`T63_GLOBALS=0 tasks/T66-files/build-run.sh`, release build):
  build 93.8 s / 32.6 MB, `wasmtime compile` 28.6 s, run ends like node with no coordinator: `Socket error Error:
  connect ECONNREFUSED 127.0.0.1:6638`, `Error while opening socket`, exit 1.
