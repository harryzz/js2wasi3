# T67: `dgram` (UDP) over `wasi:sockets` in the event loop (F282)

Status: done (2026-10-03) | Depends on: T66

## Goal
`node:dgram` (T65: loads, `createSocket` throws) over `wasi:sockets` UDP, as T66 did `net` over TCP:
`createSocket('udp4' | 'udp6' | options)`, `bind`, `send` (with and without `connect`), `message` events with
`rinfo` (`address`, `family`, `port`, `size`), `close`, `address()`, `remoteAddress()`, node's errors. Users in
zigbee2mqtt's graph: multicast-dns (mDNS discovery), winston-syslog.

## What we know (at handoff)
- `wasi:sockets@0.3.1` `udp-socket` (`probes/wit/deps/wasi-sockets-0.3.1/package.wit`): `create(family)`,
  `bind`, `connect`/`disconnect`, `send: async func(list<u8>, option<ip-socket-address>)`, `receive: async func()
  -> result<tuple<list<u8>, ip-socket-address>>`, `get-local-address`, buffer sizes and hop limit (`u64`/`u8`).
  No multicast membership (`addMembership` etc. can't work: say so), no broadcast option.
- wasmtime 49: UDP is on by default (`-S udp`), needs `-S inherit-network` (`scripts/diff.sh` passes it).
- T66 made `wasi:sockets/*` imports non-blocking in commands (async funcs → promises; see T66 Notes/Result and
  `crates/frontend/node/wasi_sockets.mjs`, which needs the `udp-socket` imports added). `node:internal/net` has
  address formatting, node's errors for WASI codes and `lookupAll` (for `send` to a host name).
- A receive loop is one pending `receive` call at a time; `close()` can't cancel a pending `async func` call
  (only stream reads are cancellable, F292): find out whether dropping the `udp-socket` ends it, or whether the
  subtask needs `subtask.cancel` (the backend has `Import::SubtaskCancel` for timers).

## Plan
1. Probe: raw `udp-socket` imports in a command: bind, send to self, receive; drop the socket with a receive pending.
2. `node:dgram`: `Socket` (EventEmitter), `createSocket`, the methods above; `ref`/`unref` as in `net` (F289).
3. Fixture `node/dgram.js` (two sockets in one program, each step awaited) through `scripts/diff.sh`.

## Read first
- `tasks/T66-net-socket.md` (Notes / Result), `crates/frontend/node/net.js`, `net_support.js`, `dgram.js`.

## Tests
`node/dgram.js` and `node/net.js` through `scripts/diff.sh`; `cargo test`.

## Acceptance
- [x] Two sockets in one program exchange datagrams like node (`message`, `rinfo`, `close`, errors).
- [x] Follow-ups updated.

## Notes / Result
**What changed**
- `node:dgram` (`crates/frontend/node/dgram.js`, F282): node 24's lib/dgram.js logic (bind and connect states, sends
  and connects queued while binding, `send` argument forms incl. offset/length and arrays of chunks, implicit bind,
  `bindSync`/`connectSync`, `close` with its queue, `address()`/`remoteAddress()`, `setTTL`, node's errors:
  `ERR_SOCKET_BAD_TYPE`, `ERR_SOCKET_ALREADY_BOUND`, `ERR_SOCKET_DGRAM_NOT_RUNNING`/`IS_CONNECTED`/`NOT_CONNECTED`,
  `ERR_SOCKET_BAD_PORT`, `ERR_INVALID_ARG_TYPE`, `getsockname EBADF`, `bind EADDRINUSE`, `send EMSGSIZE`) over `UDP`, a
  handle shaped like node's `udp_wrap` on `wasi:sockets` `udp-socket`.
- `node:internal/wasi_udp` (`wasi_udp.mjs`): the `udp-socket` imports and `cancel(promise)`; separate from
  `wasi_sockets` so `net`/`dns` programs keep their bytes.
- `Intrinsic::ImportCancel` (`__intrinsic_import_cancel(promise)`; IR, lowering, backend `abi.rs` `import_cancel`):
  a pending `async func` import call borrows its resource, so a socket can't be dropped under a pending `receive`
  (`cannot remove owned resource while borrowed`); the helper finds the call in the pending table, leaves the
  waitable set, runs a sync `subtask.cancel`: returned → handled as its event; cancelled → entry, count and subtask go,
  the promise never settles. A no-op outside commands waiting for imports.
- Fixture `tests/fixtures/node/dgram.js`. Docs: README, design.md (T66 paragraph), keywords.md.

**Commands run**: probe (raw `udp-socket` imports); `scripts/diff.sh` on `node/dgram.js` (4 runs), `node/net.js`,
`node/tls-dns-dgram.js`, `node/dns-lookup.js`: pass; an edge probe vs node (close before `listening`, queued send then
close, bind to `localhost`): equal; `cargo test --release --no-fail-fast`: all pass; `node/net.js` built by HEAD and
this tree: byte-identical; zigbee2mqtt (T66's `patch.sh`, `T63_GLOBALS=0`) builds (92 s) and validates.

**Unsupported** (F301): multicast/broadcast (ENOSYS; multicast-dns's mDNS can't work), buffer sizes, `ref`/`unref`,
socket options (`reuseAddr`, `ipv6Only`, block lists), `bind({ fd })`, cluster.

**Next smallest step**: F288's order goes on with the TLS client (F281) and the HTTP client (F286); no task file
yet. For the zigbee2mqtt milestone, F299 (start-up speed) and F300 (`net` close timing) may come first.

## Checklist
- [x] probe
- [x] `node:dgram`
- [x] fixture, docs, follow-ups

## Progress
- 2026-10-02: handoff written after T66.
- 2026-10-03: started; tools unchanged. Probe (raw `udp-socket` imports in a command): create/bind/get-local-address, send to self and receive (data, sender address), `datagram-too-large` for 70,000 bytes, send after `connect` with no address: all work. Dropping the socket with a `receive` pending traps (`cannot remove owned resource while borrowed`): close must cancel the call first. Added `Intrinsic::ImportCancel` (`__intrinsic_import_cancel(promise)`, backend `abi.rs` `import_cancel`: find the call in the pending table, leave the waitable set, sync `subtask.cancel`; returned → handled as its event, cancelled → entry and count removed, subtask dropped, promise stays pending) and the UDP imports + `cancel` to `node:internal/wasi_sockets`.
- 2026-10-03: `node:dgram` written (`crates/frontend/node/dgram.js`): node 24's lib/dgram.js logic (bind/connect states,
  the queue of sends and connects made while binding, `bindSync`/`connectSync`, its errors) over `UDP`, a handle shaped
  like node's `udp_wrap` on `udp-socket` (receive loop = one pending `receive`; `close` cancels it, then drops the socket
  once no send is in flight). Fixture `node/dgram.js` (two/three sockets, each step awaited; ports masked in messages)
  equals node on the first build, 3 more runs too; edge probe (close before `listening`, a send queued behind a bind
  then closed, a bind to `localhost`) equals node.
- 2026-10-03: the UDP bindings first went into `wasi_sockets.mjs`, which made every `net`/`dns` program import them
  (`node/net.js` bytes changed): moved to their own `node:internal/wasi_udp` (`wasi_udp.mjs`, with `cancel`); `net.js`
  is byte-identical to HEAD again. `diff.sh` on `node/dgram.js`, `net.js`, `tls-dns-dgram.js`, `dns-lookup.js`: pass;
  `cargo test --release --no-fail-fast`: all pass.
- 2026-10-03: zigbee2mqtt rebuilt with the new `dgram` (T66's `patch.sh`, `T63_GLOBALS=0`): 92 s / 751 MB, valid. Docs, follow-ups (F282 ticked, F292 noted, F301). Done.
