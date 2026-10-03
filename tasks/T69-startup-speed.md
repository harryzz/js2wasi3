# T69: zigbee2mqtt start-up speed under js2wasi3: profile, then fix the top costs (F299)

Status: todo (handoff written 2026-10-03) | Depends on: T68, T67

## Goal
Find out where js2wasi3's zigbee2mqtt spends the 3–5 s that node does in under a second, and make the largest
costs smaller in the compiler or runtime (not in zigbee2mqtt), so that the start ends like node's: one
`bridge/state` online (mqtt.ts's 2 s `republishRetainedTimer` is cleared by the echo of `bridge/info` before it
fires) and `Zigbee2MQTT started!` in the same second as `Connected to MQTT server`. Record the profile, what each fix
gained, and what's left. Programs that don't hit a changed path keep their bytes, unless a fix is a general speed-up
(then: every fixture still equals node and the bytes change is explained).

## What we know (at handoff)
- T68's live runs (`tasks/T68-files/`, see T68's Progress): from the MQTT connect to `started!` js2wasi3 takes 3–5 s,
  node < 1 s. In between: `onConnect` (QoS 1 publish of `bridge/state`, subscribe), then the controller starts the
  extensions (`Discovering entities to Home Assistant in 5s` comes 3 s after the online publish). The whole start
  (Z-Stack handshake, NV reads, backup) is otherwise the same lines as node's.
- It's compute, not I/O: T68's `tasks/T68-files/mqtt-probe.js` (connect, QoS 1 publish, subscribe, echo, 5 × QoS 1)
  takes 1,310 ms under js2wasi3 vs 312 ms under node, i.e. ~2× per round trip and a 665 ms connect that includes the
  program's start.
- `wasmtime run --profile=guest,<file>,<interval>` works on js2wasi3 components (checked 2026-10-03 on a small
  command: 34 samples at 1 ms, function names such as `main`, `run`, `consts$init0`, `hostcall`). The output is a
  Firefox profiler JSON; it sees only guest frames.
- `perf` 6.12 is installed (2026-10-03; `perf_event_paranoid` 2: own processes): `perf record -F 2000 -g wasmtime run
  --profile=perfmap …` then `perf report --no-children --sort sym --stdio` resolves guest functions by name (checked:
  `to_string`, `$$reEmit`, `find_own`, `string_eq`) **and** shows wasmtime's own costs, which the guest profiler can't:
  on the small check its GC (`collect_increment`, `trace_vmctx_roots`, `scan_field`) and loading
  (`ZSTD_decompress…`, `sha256`) were the top entries. Use `perf` first; `--profile=guest` when a timeline matters.
- If another tool is missing (e.g. a flame-graph script), ask the user to install it; don't write an emulation.
- Function names in the module: the backend names user functions after their JS names (`#funcN name`) and helpers by
  role (`inspect_format`, `call_value`, `promise_reaction_job`, …), so a profile points at JS functions and runtime
  helpers both.
- Candidates to check, not measured yet: property lookups on large objects (hash index past 8 entries), `call_value`
  and arguments arrays, string concatenation (log formatting: winston + logform run per log line, and the run logs
  at `debug`), `JSON.stringify`/`parse` of settings and the HA discovery payloads, RegExps (logform, safe-stable-
  stringify), the event loop's turn (timers, `run_callback`). Build-time costs (F269 lowering, F270 wasmtime compile)
  are separate.
- `log_level: debug` in T68's config: debug lines are formatted too (thousands of `zh:zstack:unpi` lines). Compare a
  run at `info` to see how much is logging.

## The test setup
T68's: `tasks/T68-files/build-run.sh` (`Z2M_SERIAL`, `Z2M_MQTT` from the environment; addresses in the local memory
note), the test-only `data/configuration.yaml` (base topic `zigbee2mqtt-js2wasi3`, HA discovery
`homeassistant-js2wasi3`; never the default topics: the broker is shared with production), `watch.sh` for MQTT,
and afterwards clear the retained test topics (`mosquitto_sub --retained-only -W 3` on the two test prefixes, then
`mosquitto_pub -r -n` for each). `T68_BUILD=0` reruns `p2.cwasm` without rebuilding. A profile without the coordinator
is possible too (`serial.port` unreachable: herdsman fails at once), but it doesn't contain the slow window.

## Plan
1. Profile: `perf record -g wasmtime run --profile=perfmap …` with T68's flags (the `.wasm`, or check that a `.cwasm`
   keeps the perf map), at `debug` and at `info`; split guest code from wasmtime's (GC, host calls). Summarize the
   samples between the MQTT connect and `started!` (log timestamps vs sample times) into the top 20 functions by
   self and inclusive time. Also profile `mqtt-probe.js` (small, quick to rebuild) to see the per-round-trip cost.
2. Pick the top one or two costs that are compiler/runtime work; fix with a micro-benchmark fixture each (node is the
   oracle for output; time measured with `scripts/diff.sh`'s timings or a loop).
3. Rerun the live start, record before/after (`connect → started!` time, the `bridge/state` publishes), and the
   remaining profile. Follow-ups for the rest.

## Read first
- `tasks/T68-capture-stack-trace.md` (Progress: the live runs, the probe), `tasks/T68-files/build-run.sh`,
  `docs/zigbee2mqtt-feasibility.md` ("Live coordinator"), `docs/design.md` (D3 values and objects, D5 built-ins).

## Tests
The new micro-benchmark fixtures with `scripts/diff.sh`; `cargo test`; `wasm-tools validate` + wasmtime; a bytes check
of the command fixtures against HEAD for each fix (explain every change); the live run is a probe, recorded in
Progress.

## Acceptance
- [ ] A profile of the slow window, summarized (top functions, where the time goes) in Notes / Result.
- [ ] At least the largest compiler/runtime cost reduced, with its before/after numbers.
- [ ] The live start measured again; follow-ups for what's left.

## Notes / Result
(to be written)

## Checklist
- [ ] profile
- [ ] fix(es) + fixtures
- [ ] live rerun, docs, follow-ups

## Progress
- 2026-10-03: handoff written after T67 (`main` at `0bf597c`); `--profile=guest` and `perf` + `--profile=perfmap` checked on a small component.
