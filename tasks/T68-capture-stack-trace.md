# T68: `Error.captureStackTrace` (F295), zigbee2mqtt against a live coordinator and MQTT broker

Status: done (2026-10-03) | Depends on: T66 | Before T67 (it blocks every zigbee-herdsman request)

## Goal
1. `Error.captureStackTrace(obj, fn?)` like node 24.21.0 (F295): an own `stack` accessor on `obj`, built the way
   T63's lazy `error.stack` is (`${String(obj)}\n    at <anonymous>`; `fn` accepted and ignored, there are no frames);
   `Error.captureStackTrace.length` 2; node's TypeError for a non-object; printing of `obj` unchanged (T63's
   `frameless_stack`). `Error.stackTraceLimit` (a number, writable) and `Error.prepareStackTrace` (absent:
   `undefined`; setting it has no effect, recorded) if they come cheap.
2. Run zigbee2mqtt 2.14.1 (T66's build, no stubs) against the user's test coordinator and MQTT broker, next to node
   on the same config, and record every difference past the Z-Stack handshake: NV reads, network start, coordinator
   backup, MQTT connect, publishes, Home Assistant discovery (on test-only topics), shutdown.
Programs that don't name `captureStackTrace` keep their bytes.

## What we know (at handoff)
- Live probe 2026-10-03 (after merging T62–T66): with the coordinator answering, node completes the start:
  `Coordinator firmware version: … "type":"ZStack3x0" … "revision":20230507`, `zigbee-herdsman started (reset)`, a
  coordinator backup written, `Currently 0 devices are joined`, then MQTT (none at 127.0.0.1 then: ECONNREFUSED) and a
  clean herdsman stop. js2wasi3 connects, writes the bootloader skip and `SYS ping`, then
  `Failed to connect to the adapter (TypeError: Error.captureStackTrace is not a function)`: zigbee-herdsman's
  `dist/utils/waitress.js:48` calls it in `start()` of every request timeout, so all three pings fail within a second.
- node: `Error.captureStackTrace(o)` gives `o` an own accessor `stack` (get/set, not enumerable, configurable).
  Users in zigbee2mqtt's graph: 18 files (zigbee-herdsman incl. the ember `oneWaitress.js`, ws, js-yaml, logform,
  fflate, bindings, stack-trace, the frontends); `stackTraceLimit` 5 files, `prepareStackTrace` 6 (bindings,
  stack-trace, frontends).
- T63's machinery: `crates/backend/src/object.rs` `error_stack`, `frameless_stack`; `props.rs` `lazy_stack_set`;
  gated on code naming `stack` (`lower/src/builtins.rs` GATED). Missing statics are run-time errors where the
  call may not run (T63's rule, design.md "node's globals js2wasi3 lacks").
- Log timestamps differ (js2wasi3 prints UTC, node local time): F182, not this task.

## The test setup (the user's; see the local memory note for addresses)
- Coordinator: a Sonoff ZBBridge Pro (CC2652P, Z-Stack 3.x.0 20230507) behind ESPHome's `stream_server` on TCP port
  6638, test only, no devices. Its ESPHome config had the BSL pin (GPIO22) active, so every reset entered the ROM
  bootloader (silence to ZNP, `00 cc` to `55 55`); fixed 2026-10-03 (`inverted: true`). If it is silent again, send
  `55 55`: an ACK means the bootloader again.
- MQTT broker: no auth, **shared with a production zigbee2mqtt and Home Assistant**. Use only
  `tasks/T68-files/data/configuration.yaml`: base topic `zigbee2mqtt-js2wasi3`, client id `zigbee2mqtt-js2wasi3`,
  HA discovery on `homeassistant-js2wasi3` (not `homeassistant`), frontend off; a test network with its own PAN,
  extended PAN, key, channel 25 (not zigbee2mqtt's defaults). Never run with the default `zigbee2mqtt` or
  `homeassistant` topics. Clean up retained test topics afterwards if any are left (`mosquitto_pub -r -n` or mqtt.js).
- Each run with a fresh data dir resets the coordinator's network (fine: test stick).

## Plan
1. `Error.captureStackTrace` (+ `stackTraceLimit`/`prepareStackTrace` if cheap), fixture
   `tests/fixtures/exceptions/capture-stack-trace.js` (own accessor, `length`, `fn` argument, a class calling it in
   its constructor like zigbee-herdsman, printing, `JSON.stringify`, a non-object), `diff.sh`, `cargo test`, bytes check.
2. Rebuild zigbee2mqtt and run `tasks/T68-files/build-run.sh` (node and js2wasi3 on the same config, same coordinator
   and broker); record the first differences in order, fix what is tiny, record the rest (follow-ups).
3. Docs (design.md T13/T63 paragraph, keywords.md, README, zigbee2mqtt-feasibility.md "live coordinator" section),
   follow-ups, the next handoff (T67 or what the live run found first).

## Read first
- `tasks/T63-small-gaps.md` (Notes / Result: `error.stack`), `tasks/T66-net-socket.md` (Notes / Result),
  `docs/zigbee2mqtt-feasibility.md` (latest section), `tasks/T62-files/setup.sh`, `tasks/T68-files/build-run.sh`.

## Tests
The new fixture with `scripts/diff.sh`; `cargo test`; `wasm-tools validate` + wasmtime; bytes check of every command
fixture against HEAD (only programs naming `captureStackTrace` may change). The live run is a probe, not a test
(`diff.sh` can't reach the coordinator): record its commands and results in Progress.

## Acceptance
- [x] `Error.captureStackTrace` like node (fixture equals node).
- [x] zigbee2mqtt under js2wasi3 against the live coordinator gets past the Z-Stack handshake; every difference from
      node's run recorded, grouped, with follow-ups.
- [x] No messages on the production topics (`zigbee2mqtt/#`, `homeassistant/#`) from either run.

## Notes / Result
**What changed**
- `Error.captureStackTrace(o, fn)` (F295): `$$Error_captureStackTrace` in `crates/lower/builtins/object.js`, JS over
  `Object.defineProperty`: an own `stack` accessor (get/set functions, not enumerable, configurable: node's
  descriptor, unlike T63's lazy slot), made on its first read from `Error.prototype.toString`'s steps on `o` then (V8
  uses those, not `o`'s own `toString`) plus T63's one frame; a write replaces it. node's errors: `TypeError:
  invalid_argument` for a non-object, `Cannot define property stack, object is not extensible`. `length` 2; usable as
  a value. No backend change; only programs reaching it change.
- `Error.isPrototypeOf(…)` & co. (`Object.prototype` methods read from `Error`) count as inherited
  (`builtins.rs` `inherited_prop`, `lib.rs` the missing-static rule): `Error` having a prelude static had made them
  missing statics (a regression the bytes check caught in `node:assert`'s `throws`).
- Fixture `tests/fixtures/exceptions/capture-stack-trace.js`. Scripts `tasks/T68-files/restart-run.sh`, `watch.sh`,
  `mqtt-probe.js` (addresses from the environment).

**Commands run**: `scripts/diff.sh` on the fixture and `tests/fixtures/exceptions` (13 pass, 2 skipped) and
`node/assert.js`; `cargo test --release --no-fail-fast` (all pass); bytes check of all 338 `tests/fixtures/**/*.js` by
HEAD and this tree: only the new fixture differs. test262: `built-ins/Error` isn't in this machine's sparse checkout,
not run (the change is the one static and `Error`'s inherited methods). Live: `build-run.sh` (node, build, js2wasi3),
a second node run, `restart-run.sh node|wasm`, MQTT watched throughout (see Progress).

**zigbee2mqtt live result**: under js2wasi3 it starts like node against the real coordinator and broker (handshake,
NV reads, network start, backup, MQTT, retained bridge topics, HA discovery with equal payloads), stops and restarts
on an MQTT request like node. Differences, all recorded: herdsman version stubbed by the patch (F273), slow start
(3–5 s from MQTT connect to started, so an extra `bridge/state` online: F299), the old socket's `close` late on a
restart (F300), no signals (F298), UTC timestamps (F182). Production topics: nothing; retained test topics cleared.

**Unsupported / deferred**: `Error.stackTraceLimit` default 10 and node's `Error.prepareStackTrace` (F296); statics
as own properties (`Error.hasOwnProperty("captureStackTrace")`, F297); F298–F300 above.

**Next smallest step**: T67 (`dgram` over `wasi:sockets` UDP, F282), as planned; then F299 (profile the start) or
F300 (`net` close timing) if the milestone needs them first.

## Checklist
- [x] captureStackTrace + fixture
- [x] live run: node vs js2wasi3, differences recorded
- [x] docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written after merging T62–T66 (`main` at `683ac72`) and the live coordinator probe
  (`tasks/T68-files/`: test-only config, `build-run.sh` with `Z2M_SERIAL`/`Z2M_MQTT` from the environment).
- 2026-10-03: started; tools unchanged. node probes: header is Error.prototype.toString's steps at first read (not the object's own toString), non-object → TypeError `invalid_argument`, non-extensible → `Cannot define property stack, object is not extensible`, `stackTraceLimit` 10 (writable, enumerable data), `prepareStackTrace` is node's own function. Wrote `$$Error_captureStackTrace` in `builtins/object.js` as a real accessor (node's descriptor; no backend change).
- 2026-10-03: fixture `exceptions/capture-stack-trace.js` equals node (first build); `diff.sh tests/fixtures/exceptions`: 13 pass, 2 skipped; `cargo test --release --no-fail-fast`: all pass. Printing an error after the capture is unchanged (`[TE: x]`, as without it; node prints frames). `stackTraceLimit`/`prepareStackTrace`: zigbee2mqtt's graph (stack-trace, bindings, the frontends) only saves and restores them, which works (reads `undefined`, writes stored); node's defaults (10, node's function) left as a follow-up.
- 2026-10-03: bytes check (every `tests/fixtures/**/*.js` built by HEAD and by this tree, `cmp`): 335 same, 3 differ:
  the new fixture and `node/assert.js`, `commonjs/builtins/main.js`. Cause: a regression. `Error` now has a prelude static,
  so `Error.isPrototypeOf(…)` (node:assert's `throws`) became a missing static (a build error where it always runs).
  Fixed: `Error`'s `Object.prototype` methods count as inherited (`builtins.rs` `inherited_prop`, `lib.rs` the
  missing-static rule), as for `Promise`/constructor values. After that: only the new fixture differs. Found too: statics
  aren't own properties (`Error.hasOwnProperty("captureStackTrace")` false, node true; `Promise.hasOwnProperty` is a build
  error at HEAD already): follow-up.
- 2026-10-03: live run 1 (`build-run.sh`, `Z2M_SERIAL=tcp://<coordinator>:6638 Z2M_MQTT=mqtt://<broker>:1883 T63_GLOBALS=0
  T68_TIMEOUT=90`; MQTT watcher `mosquitto_sub` on the test topics, and on `zigbee2mqtt/#`+`homeassistant/#` filtered for
  `js2wasi3|19004|0x4a3c`). node: full start (`started (reset)`, backup, MQTT, 8 HA discovery configs), clean SIGTERM stop.
  build 105 s / 725 MB, `wasmtime compile` 52 s / 2.1 GB. **js2wasi3 gets through the whole start**: handshake, NV reads,
  `zigbee-herdsman started (resumed)`, coordinator backup written, `Currently 0 devices are joined`, MQTT connect,
  `bridge/state` online, `Zigbee2MQTT started!`, HA discovery. Then idle until the timeout.
- 2026-10-03: node run 2 on the same coordinator/broker state (the network formed by run 1, retained discovery) for a fair
  comparison: `(resumed)` and 7 "already discovered" in node too (order effects, not differences). Normalized diff of the
  start (timestamps, data paths, unpi frames stripped), 2,062 lines: equal except (a) `Starting zigbee-herdsman (T62 stub)`
  (patch.sh's stub for `import(\`${depend}/package.json\`, {with json})`), (b) one NIB byte (47 vs 18) the coordinator
  changes between runs, (c) a second `bridge/state` online and `started!` 5 s after the MQTT connect (node: same second):
  mqtt.ts's `republishRetainedTimer` (2 s) fires because startup is slow. The one HA discovery publish is byte-equal.
  mqtt.js probe (connect, QoS 1 publish, subscribe, echo, 5 × QoS 1): node 312 ms, js2wasi3 1,310 ms (~2× per round
  trip, connect 665 ms incl. start): I/O is not the 3–5 s, startup compute is. SIGTERM: wasmtime can't deliver it, the
  timeout kills the guest (no offline publish, no final backup). Production topics: 0 messages naming the test instance.
- 2026-10-03: restart run (`$S/restart-run.sh node|wasm`: start, `mosquitto_pub zigbee2mqtt-js2wasi3/bridge/request/restart`
  once started, stop after the second start): the stop path works under js2wasi3 (`Stopping Zigbee2MQTT (restart=true)`,
  offline publish, MQTT disconnect, herdsman stop, final backup, `closing`, then a second full start). Normalized diff,
  4,094 lines: the known (a)–(c) plus the old socket's `Port closed` logged after the new socket's `Opening TCP socket`
  (node: before `Logging to console`): `net.Socket`'s `close` after `destroy()` comes later than node's. Afterwards:
  production capture empty (0 lines); the 15 retained test-topic messages cleared (`mosquitto_pub -r -n`), 0 left.
- 2026-10-03: docs (design.md T13/T63 paragraph, keywords.md `throw`, README, zigbee2mqtt-feasibility.md "Live coordinator"), follow-ups F296–F300 (F295 ticked), handoff to T67. Done.
