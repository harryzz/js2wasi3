# T68: `Error.captureStackTrace` (F295), zigbee2mqtt against a live coordinator and MQTT broker

Status: todo (handoff written 2026-10-03) | Depends on: T66 | Before T67 (it blocks every zigbee-herdsman request)

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
- [ ] `Error.captureStackTrace` like node (fixture equals node).
- [ ] zigbee2mqtt under js2wasi3 against the live coordinator gets past the Z-Stack handshake; every difference from
      node's run recorded, grouped, with follow-ups.
- [ ] No messages on the production topics (`zigbee2mqtt/#`, `homeassistant/#`) from either run.

## Notes / Result
(to be written)

## Checklist
- [ ] captureStackTrace + fixture
- [ ] live run: node vs js2wasi3, differences recorded
- [ ] docs, follow-ups, next handoff

## Progress
- 2026-10-03: handoff written after merging T62–T66 (`main` at `683ac72`) and the live coordinator probe
  (`tasks/T68-files/`: test-only config, `build-run.sh` with `Z2M_SERIAL`/`Z2M_MQTT` from the environment).
