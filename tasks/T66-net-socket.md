# T66: `net.Socket` (TCP client) over `wasi:sockets` in the event loop (F268)

Status: not started | Depends on: T65

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
- [ ] A refused connection matches node; a connection to a listening server exchanges data.
- [ ] zigbee2mqtt builds with no built-in stubs and fails like node with no coordinator.
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] probe
- [ ] `node:net`
- [ ] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-02: handoff written after T65.
