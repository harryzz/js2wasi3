# T69: zigbee2mqtt start-up speed under js2wasi3: profile, then fix the top costs (F299)

Status: done (2026-10-03) | Depends on: T68, T67

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
- [x] A profile of the slow window, summarized (top functions, where the time goes) in Notes / Result.
- [x] At least the largest compiler/runtime cost reduced, with its before/after numbers.
- [x] The live start measured again; follow-ups for what's left.

## Notes / Result
**Profile** (`perf record -e cpu-clock -k CLOCK_REALTIME -g` + `wasmtime --profile=perfmap`, live start, debug log level):
the 4 s from `Connected to MQTT server` to `Zigbee2MQTT started!` were ~70% wasmtime's copying GC
(`CopyingHeap::forward` 21.8%, memmove 13.1%, `scan_field` 11.3%, `collect_increment` 11.0%) and ~20% js2wasi3's
code; over the whole run GC 51%, guest 41%. With the GC heap fixed, guest code is 80%: property access (`find_own`,
`get`, `string_eq`, `own_get`, `string_hash`, `to_property_key` ≈ 27%), RegExp (`$$reRun` 4%), and (before the fix
below) wasmtime's `is_subtype` libcall, 14.4%.

**What changed**
- Host: wasmtime 49's copying collector only grows once live data fills the active semi-space (`should_collect_first`:
  `usage < capacity / 2`, capacity = both spaces; unchanged on `main`). zigbee2mqtt's ~30 MiB live set then sits in a
  64 MiB heap: 54 collections copying ~32 MiB and freeing ≤ 1.5 KiB each. `tasks/T68-files/build-run.sh` and
  `restart-run.sh` pass `-O gc-heap-initial-size=${Z2M_GC_HEAP:-268435456}` to `compile` and `run` (64 MiB isn't
  enough); README shows the flag. Not in `diff.sh`/test262 (user's choice: GC-timing bugs like F157 stay visible).
  Standalone reproducer `tasks/T69-files/gc-thrash.wat` (1,837 ms / 249 collections → 284 ms / 1) and a wasmtime issue
  draft `tasks/T69-files/wasmtime-issue.md`, **not posted** (F304).
- Compiler: `runtime::JsTest` (`test_string(v)`, `test_object(v)`): value tests of a local check the abstract
  `array`/`struct` kind and exact final types before the concrete `$string`/`$object` test, which wasmtime compiles to
  an `is_subtype` libcall for every non-exact type. 77 sites in 10 backend files; same results.
  `props.mjs` micro-benchmark ~10% faster (419–478 → 375–417 ms), `is_subtype` 10.6% → 0; zigbee2mqtt 14.4% → 1.7%
  of samples. Size: +2.4% `hello.js`, +0.4% `node/net.js`, +0.02% zigbee2mqtt.
- Fixture `tests/fixtures/objects/value-kinds.js` (every kind of value through truthiness, `===`/`==`, `+`, `typeof`,
  conversions, property reads/writes, `in`, `instanceof`, keys).

**Live result** (256 MiB heap + `JsTest`, zigbee2mqtt rebuilt from this tree): connect 08:21:08 → started 08:21:09,
**one** `bridge/state` online (node's behaviour); production capture empty. One-second log stamps are too coarse to
time the window further; the deterministic no-coordinator start is ~0.2 s with the big heap.

**Commands run**: `diff.sh` on objects, classes, arrays, builtins, exceptions, `values.js`, the new fixture (all pass);
`cargo test --release --no-fail-fast` (pass); live runs `$S/perf-run.sh` (dbg, big, m64, t69), GC traces
(`WASMTIME_LOG=wasmtime::runtime::vm::gc=trace`), micro-benchmarks `gc.mjs`, `props.mjs`, `gc-thrash.wat`.
Bytes: every program changes (the runtime's type tests); explained above.

**Left** (follow-ups): F302 `Symbol.prototype.constructor` traps; F303 property access (constant-key hashes at build
time, hash in the index, identity fast path), the 19 stack-value tests, 61k GC-root globals; F304 the upstream issue.

**Next smallest step**: T70 (`fs.promises` after a turn, F300); a speed task for F303 when start-up matters again.

## Checklist
- [x] profile
- [x] fix(es) + fixtures
- [x] live rerun, docs, follow-ups

## Progress
- 2026-10-03: handoff written after T67 (`main` at `0bf597c`); `--profile=guest` and `perf` + `--profile=perfmap` checked on a small component.
- 2026-10-03: started; tools unchanged. perf needs `-e cpu-clock` (WSL: no PMU) for `-k CLOCK_REALTIME`; with
  `--profile=perfmap` a `.cwasm` keeps its names (`wasm[0]::function[N]::name`). Live run under perf
  (`$S/perf-run.sh`: T68's flags, MQTT watched, production capture empty): connect 07:37:45 → started 07:37:49, two
  `bridge/state` online. **The slow window is ~70% wasmtime's copying GC** (`CopyingHeap::forward` 21.8%, memmove
  13.1%, `scan_field` 11.3%, `collect_increment` 11.0%, `is_subtype_cached` 5.6%, roots ~6%); guest code ~20%
  (`find_own` 1.9, `string_hash` 1.5, `get` 1.5, `to_number` 1.2, `box_number` 1.2, `js_less` 1.1). Whole run: GC
  51%, guest 41%.
- 2026-10-03: same `.cwasm` compiled and run with `-O gc-heap-initial-size=268435456`: connect 07:40:12 → started
  07:40:13, **one** `bridge/state` online (node's behaviour); GC 19% of the run, guest 68%. Heap size alone meets the
  goal; the component can't set it.
- 2026-10-03: why: `WASMTIME_LOG=wasmtime::runtime::vm::gc=trace` on a 200k-iteration allocation loop: the GC heap
  stays at 64 KiB (`capacity = 0x00010000`) for all 6,241 collections; utilization after each is ~26%, too low for
  wasmtime 49's copying collector to grow it. Micro-benchmark (`gc.mjs`: optional live set N, then 2M short-lived
  objects; node 25 ms): N=0 default 12.9–15.0 s, `drc` collector 13.9 s, initial heap 1 MiB 2.3 s, 16 MiB 1.8 s,
  256 MiB 2.8 s; N=200k (heap grows with the live set) default 4.9 s ≈ 256 MiB 5.3 s. Per collection with a small live
  set the fixed cost is root tracing (`trace_vmctx_roots` + `RegisteredType::root`/drop + `from_wasm_type` ≈ 57% of
  N=0's time; 628 ref globals there, **61,442 in zigbee2mqtt**, 60,276 of them string constants). A live "ballast"
  array to force growth is worse (1 MiB: 30 s, 4 MiB: 136 s: copied every collection). 20k extra string globals made
  N=0 faster (10.7 s): the bigger live set grew the heap.
- 2026-10-03: user decision: pass the GC heap size in the zigbee2mqtt scripts and document it (not in `diff.sh` or the
  test262 runner: GC-timing bugs such as F157 stay visible), then compiler-side GC cost; draft a wasmtime issue for
  review (nothing posted). 64 MiB initial heap does **not** help zigbee2mqtt (window 5 s, two online publishes, GC
  49%); 256 MiB does.
- 2026-10-03: why (wasmtime 49.0.1 source, `store/gc.rs` `should_collect_first`; unchanged on `main` 3be9345 of
  2026-10-02): on a failed allocation wasmtime collects instead of growing while `last post-GC bytes + needed <
  capacity / 2`. For the copying collector `capacity` is both semi-spaces, so the threshold is a **full** active
  semi-space: it grows only when live data alone fills it. zigbee2mqtt (trace, run without coordinator): live data
  after each collection doubles 32 KiB → 32 MiB, then stays at 0x1fff9f0–0x2000000 in a 64 MiB heap: 54 collections,
  each ~110 ms (copying ~32 MiB) and freeing a few hundred bytes to ~1.5 KiB. Upstream context: wasmtime#13470 (the
  heuristic from #12942 regresses allocation-heavy workloads; maintainers keep it for RSS ∝ live set and would
  accept a tunable ratio; not landed). Our case is the copying collector's own bound, not #13470's small-live-set one.
- 2026-10-03: standalone reproducer `tasks/T69-files/gc-thrash.wat` (no js2wasi3: a live list of 29,000 × 1 KiB
  just under a 32 MiB semi-space, then 8M small garbage structs): default 1,837 ms / 249 collections (live stuck at
  0x1e17230 in a 64 MiB heap), `-O gc-heap-initial-size=536870912` 284 ms / 1 collection.
- 2026-10-03: scripts: `tasks/T68-files/build-run.sh` and `restart-run.sh` pass `-O gc-heap-initial-size=${Z2M_GC_HEAP:-268435456}`
  to `wasmtime compile` and `run` (they must agree; `Z2M_GC_HEAP=0` is wasmtime's default). Issue draft for review:
  `tasks/T69-files/wasmtime-issue.md` (not posted).
- 2026-10-03: with a big heap the guest is 68% of the run; top: property access (`find_own` 6.7%, `get` 4.6%,
  `string_eq` 3.7%, `string_hash` 3.0%, `own_get` 2.7%, `to_property_key` 2.3%, `property_value` 1.9%) and RegExp
  (`$$reRun` 3.8%). Micro-benchmark `props.mjs` (1,000 instances with 10 fields, a method through the prototype,
  option reads, a missing property; 300 rounds) has the same profile plus wasmtime's `is_subtype_cached` +
  `is_subtype` libcall 10.6%. Cause (cranelift `func_environ/gc.rs`): `ref.test`/`ref.cast` against a concrete
  non-final type (`$string` = 2, `$object` = 4) is inline only for the exact type; anything else (oddballs,
  number boxes, arrays, functions, symbols) calls `is_subtype`, even for "no". `to_boolean`, `strict_eq`, `js_add`,
  `get`, `set`, `to_string`, `to_number`, `to_property_key` all test values against `$string`/`$object`.
- 2026-10-03: fix: `runtime::JsTest` (`test_string(v)`, `test_object(v)` on `InstructionSink`): the abstract
  `array`/`struct` kind first (header, inline), exact final types (number, oddball; function, array), then the
  concrete test only where it is exact or rare; same results. 77 `local_get(x).ref_test_non_null(object()/string())`
  sites rewritten (19 tests of values on the stack left as they are). `props.mjs` 419–478 ms → 375–417 ms (~10%),
  `is_subtype` 10.6% → 0. `diff.sh` on objects, classes, arrays, builtins, exceptions, `values.js`: 106 pass.
- 2026-10-03: live run with the new build (256 MiB): connect → started 1 s, one online publish. Fixture `objects/value-kinds.js` equals node (found F302 on the way: pre-existing, same trap at HEAD). Docs (README flag, design.md D3 note, feasibility), follow-ups F299 ticked, F302–F304. Done.
