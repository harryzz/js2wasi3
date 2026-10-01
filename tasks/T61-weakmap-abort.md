# T61: `WeakMap`/`WeakSet` and the `AbortController`/`EventTarget` globals (F98, F241)

Status: todo (handoff written 2026-10-01, not started) | Depends on: T60

## Goal
Packages that bundle npm's `readable-stream` (most of zigbee2mqtt's stream users) build and behave like node 24.21.0:
1. `WeakMap`, `WeakSet` (F98): constructors, `get`/`set`/`has`/`delete`/`add`, node's errors (`Invalid value used as
   weak map key`, `… in weak set`), symbols (non-registered) as keys, printing `WeakMap { <items unknown> }`,
   `Object.prototype.toString`, subclassing as for `Map`. Weakness isn't observable (no `WeakRef`/`FinalizationRegistry`).
2. Globals `AbortController`, `AbortSignal`, `EventTarget`, `Event` (F241), from T60's `node/abort.js` (bound by the
   linker like T56's `Buffer`, or the prelude), also as `globalThis.X`; `events.addAbortListener`.
3. `console.assert` (F253: `event-target-shim`).
Programs that don't use them keep their bytes.

## What we know (at handoff)
- T60's probe: `require("readable-stream")` from npm (4.7.0) fails on `WeakMap` (`abort-controller`,
  `event-target-shim`) and `console.assert`; its `ours/index.js` requires node's `stream`, which exists now.
- `Map`/`Set` are native (`crates/backend/src/mapset.rs`, `builtins/map.js`); a WeakMap can be the same table with
  a kind that rejects non-object keys and has no iteration/size. Constructors as values and prototypes: see how T45
  wired `DisposableStack` (`ir::Builtin`, `runtime.rs` globals, `object.rs`), or a simpler path if one exists.
- `node/abort.js` is minimal (no phases, `passive`, `isTrusted`, printing like node).

## Plan
1. `WeakMap`/`WeakSet` + fixtures (`builtins/weakmap.js`), test262 `built-ins/WeakMap`, `built-ins/WeakSet` into the subset.
2. The abort/event globals + fixtures; `console.assert`.
3. Probe: a package with npm `readable-stream` (scratchpad), docs, follow-ups.

## Read first
- `tasks/T60-stream.md` (Notes / Result), `crates/frontend/node/abort.js`, `crates/lower/builtins/map.js`,
  `docs/design.md` D4 (T56 `Buffer` global, T60).

## Tests
`scripts/diff.sh` on the new fixtures; `cargo test`; `wasm-tools validate` + wasmtime; gating (`cmp.sh`).

## Acceptance
- [ ] `WeakMap`/`WeakSet` like node (fixtures, test262 directories).
- [ ] The globals; npm `readable-stream` in a package builds and matches node.
- [ ] Programs without them compile to the same bytes.

## Notes / Result
(to be written)

## Checklist
- [ ] WeakMap/WeakSet
- [ ] globals, `console.assert`
- [ ] package probe, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T60.
