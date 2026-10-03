# T67: `dgram` (UDP) over `wasi:sockets` in the event loop (F282)

Status: todo | Depends on: T66

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
- [ ] Two sockets in one program exchange datagrams like node (`message`, `rinfo`, `close`, errors).
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] probe
- [ ] `node:dgram`
- [ ] fixture, docs, follow-ups

## Progress
- 2026-10-02: handoff written after T66.
