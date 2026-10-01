# T61: `WeakMap`/`WeakSet` and the `AbortController`/`EventTarget` globals (F98, F241)

Status: done (2026-10-01) | Depends on: T60

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
- [x] `WeakMap`/`WeakSet` like node (fixtures, test262 directories: 183 of 185 run).
- [x] The globals; npm `readable-stream` in a package builds and matches node.
- [x] Programs without them don't load them (bytes change for all programs through the `proto()` fix; every fixture equals node).

## Notes / Result
**Decisions** (design.md D4): no native `WeakMap`: T56's `Buffer` binding became a table, `node::GLOBALS` (global →
built-in module export), so `WeakMap`/`WeakSet` (`node/weak.js`, classes over a `Map` with private-field brand checks)
and `AbortController`/`AbortSignal`/`EventTarget`/`Event` (`node/abort.js`, rewritten after node's event_target.js)
are bound by the linker where code names them; programs that don't, don't load them. Weakness isn't observable.

**What changed.** `link.rs` (`global_refs`/`global_modules` over `node::GLOBALS`); `node/weak.js` (new),
`node/abort.js` (private state, accessors, `isTrusted`, node's `eventPhase` quirk, `onabort` handler attribute,
`toStringTag`, printing, unref'd `AbortSignal.timeout`); `events.addAbortListener`; `console.assert` (`$$console_assert`,
direct calls); `process` on the global object; naming `constructor` makes `Function`'s constructor value; backend fix:
`$<B>.prototype()` re-checks its global after making its parent (it was re-entered through `Object.prototype`'s
constructor and made `Function.prototype` twice, the second keeping a `constructor` sentinel, a trap); `console.x`
other than `assert` reports once. test262: `built-ins/WeakMap`, `built-ins/WeakSet` in the sparse checkout and the
subset.

**Commands run.** Fixtures `builtins/{weakmap,console-assert,function-constructor}.js`, `node/abort.js`: equal node;
`scripts/diff.sh` on every fixture (see Progress); `cargo test` (the `unsupported` test caught the double report);
`wasm-tools validate` + wasmtime; `cmp.sh`: 257 of 316 differ (the `proto()` helper is in nearly every program), 4 new
fixtures, so gating was checked by the full `diff.sh` instead. Probe: a program using npm `readable-stream` 4.7.0
(with `abort-controller`, `event-target-shim`, the `process` package) equals node.

**test262** (full subset on `a-03`): 28,052 of 28,984 run pass (96.8%), 262 changes, 0 regressions, 1
unsupported → fail (F260); baseline and HISTORY.md updated.

**Unsupported** (F259, F260; F241, F253, F256 updated): the bound globals aren't global-object properties; `WeakMap()` without
`new`'s message; `console.assert` as a value; async/generator functions' own prototypes.

**Next smallest step:** zigbee2mqtt's next blockers (docs/zigbee2mqtt-feasibility.md): build its dependency tree and
list what fails (likely `crypto`, `url`, `net`, `child_process`; F238).

## Checklist
- [x] WeakMap/WeakSet
- [x] globals, `console.assert`
- [x] package probe, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T60.
- 2026-10-01: design: no native WeakMap; T56's `Buffer` linker binding generalized to `node::GLOBALS` (global →
  built-in module export: `Buffer`, `WeakMap`, `WeakSet`, `AbortController`, `AbortSignal`, `EventTarget`, `Event`;
  `link.rs` `global_refs`/`global_modules`). `node/weak.js` (`node:internal/weak`): classes over a `Map` with
  private-field brand checks, node's errors, `toStringTag`, printing via `util.inspect.custom`. A probe equals node
  except `WeakMap()` without `new` (the class's message).
- 2026-10-01: test262 `built-ins/WeakMap`, `WeakSet` (sparse checkout added, `fetch-test262.sh`): 183 of 185 run pass
  (2: top-level `this` in module mode). `node/abort.js` rewritten after node's event_target.js: private state,
  accessors on prototypes, `isTrusted`, `eventPhase` (node's flag cleared after the first listener), `onabort` as a
  handler attribute, `toStringTag`, printing like node, unref'd `AbortSignal.timeout`, `addAbortListener`. A probe
  (controller/signal/target/event/subclass/timeout/any) equals node.
- 2026-10-01: `console.assert` (`$$console_assert`, a direct call; not on the `console` object yet). npm
  readable-stream built, then trapped on `Object.getPrototypeOf(async function () {}).constructor`: (1) naming
  `constructor` now makes `Function`'s constructor value (`builtins.rs`); (2) a backend bug: `$<B>.prototype()` made its
  parent first and could be re-entered through it (`Object.prototype`'s constructor is a function → `Function.prototype`),
  so `Function.prototype` was made twice and the second kept the `constructor` sentinel; it now checks its global again
  after the parent (`object.rs` `proto`).
- 2026-10-01: `process` on the global object (npm's `process` package is `global.process`); npm readable-stream probe
  equals node. `console.x` reported twice once `$$console_assert` existed: fixed (`unsupported` test). Fixtures
  `builtins/{weakmap,console-assert,function-constructor}.js`, `node/abort.js`. `cmp.sh`: 257 of 316 differ (the
  `proto()` fix is in nearly every program), so `diff.sh` on all fixtures: 207 pass, 19 skip, `async/timers-more.js`
  failed under load and passes alone (F250). `cargo test` passes. test262 locally (baseline non-passing + WeakMap/
  WeakSet): 0 regressions; unsupported/fail → pass across Map/Set (WeakMap receivers), subclassing, instanceof,
  property accessors, DisposableStack `move`, RegExp match indices; WeakMap/WeakSet 183 of 185 run.
- 2026-10-01: full test262 subset on `a-03` (863 s): 28,052 of 28,984 run pass (96.8%), 262 changes, 0 regressions; 1 unsupported → fail (`await using` with a `null` `asyncDispose`: F260); baseline and HISTORY.md updated.
