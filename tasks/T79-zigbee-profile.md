# T79: profile zigbee2mqtt's real workload (recorded once, replayed offline)

Status: done 2026-10-04 (started 2026-10-04) | Depends on: T77 stage A (the time cluster no longer crashes, F330)

## Goal
Find where js2wasi3's zigbee2mqtt spends its CPU on the **real** workload, before any more speed work (user,
2026-10-04; memory "validate speed targets"). The 2026-10-04 production runs cost ~18.5–19.4 ms CPU per logged
event vs node's 1.6 ms (~12×), unchanged by T74–T76, whose proxy benchmark wasn't representative (F329). Deliver:
a repeatable offline replay of a recorded production session, CPU per handled event for node and js2wasi3 on the
same replay, and a profile (guest functions vs wasmtime's GC/runtime vs host calls) that names the top costs.
No optimizations in this task: they become follow-ups / the next task, ranked by their share of the replay.

## Read first
- `tasks/README.md` F318, F329, F334; `docs/zigbee2mqtt-feasibility.md`; `tasks/T69-*.md` (perf setup:
  `perf record -e cpu-clock -g` + `wasmtime run --profile=perfmap`; WSL has no PMU).
- `~/js2wasi3-prod-run/README.md` (outside the repo): the build tree `zp/patched`, production snapshots
  (`prod-1004b/`), `run-prod3.sh`, `sample.sh`.
- Memory: zigbee test rig (production broker 192.168.1.18 is shared: test topics only), a-03 machine.

## Plan
1. **Recorder** (`tasks/T79-files/`): a TCP proxy between zigbee2mqtt and the coordinator that logs both directions
   with timestamps (raw bytes; Z-Stack UNPI frames decoded for the report), plus an MQTT log of everything the
   broker delivers to zigbee2mqtt (retained discovery/state and live commands) and what it publishes.
2. **Record** ~10–15 min of production under node (production's zigbee2mqtt stopped by the user; snapshot of its data
   dir first, as run 3) through the proxy. Needs the user (stops production).
3. **Replayer**: a fake coordinator serving the recording (answers each request with the recorded response, rewriting
   transaction/sequence numbers where herdsman picks them; device indications at their recorded times) and a local
   MQTT broker preloaded with the recorded retained messages and inbound commands. Check that node on the replay
   logs the same events as in the recording.
4. **Measure** node and js2wasi3 (`.cwasm`, T68 flags) on the replay: CPU per handled event (startup and steady
   state separately), repeatable to a few %.
5. **Profile** the js2wasi3 replay (`perf` + `--profile=perfmap`): split guest code / GC / host; map the top guest
   functions back to zigbee2mqtt/herdsman/converters code; compare with node's `--cpu-prof` of the same replay.
6. Results, ranked follow-ups, docs (`docs/zigbee2mqtt-feasibility.md`).

## Where it runs
- Recording: the laptop (LAN access to production's coordinator and broker).
- Building p.wasm/.cwasm and the offline replays/profiles: **a-03** (user's suggestion, 2026-10-04): 72 threads,
  125 GB (wasmtime's compile of the 65 MB build needs ~9 GB), wasmtime 49.0.2 and node 24.21.0 there; the replay
  needs no LAN. perf 5.15 and mosquitto 2.0.11 are installed there (not on PATH, see Progress); the laptop has perf 6.12, no broker.
  Code gets there by rsync (memory "a-03"), never by pushing.

## Acceptance
- [x] A recording of a real production session and a replayer; node on the replay matches the recording's events.
- [x] CPU per handled event, node vs js2wasi3, on the replay (startup and steady state), repeatable.
- [x] A profile with the top costs named and their shares; ranked follow-ups.

## Notes / Result
- **Recording**: `~/js2wasi3-prod-run/rec-20261004-1941` (outside the repo, also on a-03): production's zigbee2mqtt
  (52 devices) for 900 s under node via `tasks/T79-files/record-prod.sh` (coordinator and MQTT through
  `tcp-record.cjs`). **Replay**: `run-replay.sh <rec> node|wasm <out> [s]` with `replay.cjs` (fake coordinator,
  rewriting AF transid / ZCL sequence numbers; device frames and inbound MQTT anchored at `bridge/state` online, up
  to the recording's offline) and a private mosquitto; `WASM_FLAGS`/`CWASM` for GC variants, ports for parallel
  runs, `PERF=` / `NODE_PROF=1` for profiles. node on the replay publishes exactly the recording's 13,567 messages;
  js2wasi3 runs repeat within 1%.
- **Result** (903 s from start, 548 device frames): node 8.66 s CPU; js2wasi3 with T69's 256 MiB GC heap 143–145 s
  (16.5×; 70% of it wasmtime's copying collector), with 512 MiB 39.1 s (4.5×, RSS 765 MB vs 431 MB), 1 GiB 35.9 s
  (4.1×, = the null collector's 37.0 s). DRC falls behind (100% of a core). Table and profile in
  `docs/zigbee2mqtt-feasibility.md` (first section).
- **Ranked costs at a big heap** (perf, inclusive): UTF-8 in `node:buffer` JS ~36% (F348), property access helpers
  ~27% self (F321, F334), RegExp built-ins ~9% (F349), wasmtime libcalls ~7% (F350), `box_number` 3%, GC ~3%.
- **Machines**: a-03 builds the zigbee2mqtt `.wasm` in 242 s and compiles the `.cwasm` in 48 s (14.5 GB; DRC 155 s,
  56 GB); paths mirror the laptop (`~/js2wasi3-prod-run/{zp,t62,t79,rec-*}`; t62 must exist before building);
  perf 5.15 at `/usr/lib/linux-tools/5.15.0-198-generic/perf` (Ubuntu's `/usr/bin/perf` wrapper refuses the WSL
  kernel), mosquitto at `/usr/sbin/mosquitto`.
- **Next smallest step**: F347 (heap 512–768 MiB in the run scripts and README), then F348 (native UTF-8) measured
  with `run-replay.sh` on this recording; production can be re-recorded with `record-prod.sh` when it changes.
- Follow-ups: F347–F351; resolved F329, F318.

## Progress
- 2026-10-04: started (user: pause T77 after stage A; profile zigbee2mqtt; use a-03 for building/compiling). Checked
  a-03: reachable, 72 threads, 125 GB, 716 GB free, wasmtime 49.0.2, node 24.21.0; no `perf`, no MQTT broker
  (laptop: perf 6.12, no broker). Last build: `~/js2wasi3-prod-run/p79.wasm` (65 MB) / `p79.cwasm` (343 MB).
- 2026-10-04: user: perf and mosquitto on a-03 (no passwordless sudo there), but both are already installed, just
  not on PATH: `/usr/lib/linux-tools/5.15.0-198-generic/perf` (5.15.216; `-e cpu-clock -g` works) and
  `/usr/sbin/mosquitto` 2.0.11 (system service active on 1883: replays run their own instance as harry on another
  port). Recording: build and test the recorder on the test rig first, then ask the user to stop production.
- 2026-10-04: perf on a-03 checked for real (user asked): `perf record -e cpu-clock -F 2000 -g wasmtime run
  --profile=perfmap` of a CPU-bound js2wasi3 program: 1,053 samples, wasmtime's symbols resolved (cranelift,
  regalloc2, rayon) and guest functions named (`wasm[0]::function[19]::fib`). perf 5.15 on the 6.18 WSL kernel:
  fallback is the laptop's perf 6.12 if a newer feature is missing. Recorder `tasks/T79-files/tcp-record.cjs`
  tested on the test rig (node zigbee2mqtt via both proxies, 30–60 s: starts, 1 device, MQTT ok; one transient
  11 s broker connect in the first try, 1 ms in the second). `record-prod.sh` written for the production session.
- 2026-10-04: replay tooling in `tasks/T79-files/`: `znp.cjs` (decodes recordings with zigbee-herdsman's own MT
  definitions), `replay.cjs` (fake coordinator: recorded answers matched per request, AF transid / ZCL sequence
  numbers rewritten, unsolicited frames and inbound MQTT on the recorded timeline anchored at `bridge/state` online;
  a private mosquitto preloaded with the recorded retained messages), `run-replay.sh` (one run: node | wasm, CPU at
  the anchor and at the end, `result.txt`), `record-prod.sh`, `sample.sh` (copied from the prod-run dir).
  On a-03 (paths mirrored: `~/js2wasi3-prod-run/{zp,t62,t79}`): build `p.wasm` 242 s (1.7 GB), `wasmtime compile`
  48 s (14.5 GB); the build `require`s `t62/ajv/*.js` by absolute path, so t62 must be there before building (a
  first build without it threw "Cannot find module" at run time). Test-rig recordings (30 s and 4 min) replay under
  node and js2wasi3: 19/19 requests matched, started in 2.1 s / 1.9 s; no device traffic on the test rig (the test
  device is back in production), so the device-message path is checked on the production recording.
- 2026-10-04: **production recording** `~/js2wasi3-prod-run/rec-20261004-1941` (laptop; copied to a-03): production's
  zigbee2mqtt stopped by the user (first try refused: database.db written 23 s before, by its shutdown), 900 s of
  zigbee2mqtt 2.14.1 under node on a fresh snapshot, 52 devices: 2,973 MT frames (536 AF.incomingMsg, 131
  AF.dataRequest, 107 ZDO.srcRtgInd, the startup and the shutdown backup's ~930 NV reads), 55,253 MQTT packets (986
  retained topics, 6 live inbound), 13,567 publishes; node CPU 11.3 s total. Production restarted by the user.
- 2026-10-04: first node replay on a-03: 577 device frames, 150 requests matched, CPU 9.37 s steady (936 s);
  14,348 publishes vs 13,567. Differences found and fixed: (1) 5 time-read answers (`genTime.readRsp`, live time
  in the payload) found no recorded match: new middle match by destination, cluster and ZCL command; (2) +25 meter
  updates: the recorded zigbee2mqtt stopped at 900 s and spent ~15 s on its shutdown backup while the meter kept
  reporting (per-minute counts equal until then): the replay now ends at the recording's `bridge/state` offline
  (898 s after online). `run-replay.sh` gained profiling (PERF=…: perf attached at the anchor for the steady
  window, wasmtime `--profile=perfmap`; NODE_PROF=1: node `--cpu-prof`).
- 2026-10-04: **first measurement** (a-03, replay of rec-20261004-1941, 903 s from zigbee2mqtt's start; `t79/out-node-2`,
  `out-wasm-2`): node 8.66 s CPU, js2wasi3 143.2 s (**16.5×**); per replayed device frame (548) 15.8 vs 261 ms.
  node's replay publishes 13,567 = the recording's exactly. CPU over time: first minute (startup + ~600 HA
  discovery publishes) node 4.5 s vs 37.9 s (8.4×); the ~13 min after it node 6.1 s vs ~101 s (16.6×: 0.36 vs
  7.5 s CPU per minute) — steady state dominates, as in production (12× per logged event, 4–9% of a core).
  js2wasi3-only errors: external converters `EPERM` symlink (F317) and its 2 "No converter for state_relay";
  13,448 publishes; once "Error while parsing to ZpiObject: RangeError: Attempt to access memory outside buffer
  bounds" — every recorded frame decodes identically under node and js2wasi3, also through herdsman's UNPI Parser
  in random chunks (3 seeds), and `net` chunks aren't reused: timing-dependent, not reproduced (follow-up).
- 2026-10-04: **profile** (a-03, `t79/out-wasm-perf`: perf 5.15 `-e cpu-clock -F 499 -g`, attached at the anchor for
  the 904 s window, 71,258 samples; the run's CPU 144.7 s vs 143.2 s before: repeatable to 1%). By binary: wasmtime
  72.7%, p.cwasm (compiled JS) 18.1%, libc 9.2%. **wasmtime's copying GC ≈ 70% of all CPU**: `CopyingHeap::forward`
  24.0%, `scan_field` 16.6%, `collect_increment` 13.6%, `RegisteredType::root` 4.2% (+ `drop` 1.1%),
  `trace_vmctx_roots` 4.0%, `VMGcHeader::kind` 2.5%, `array_len` 2.0%; libc's 9% is mostly memcpy-like code at
  0x1aeexx (likely the copying). Compiled JS top: `get_named` 1.6%, `find_own_named` 1.5%, `get` 0.9%,
  `box_number` 0.9%, `find_own` 0.8%, `Array.prototype.push` 0.6%. So F334's GC suspicion holds on the real
  workload, and dominates it. Next: GC variants (copying 1 GiB / 4 GiB heap, DRC, null = no GC) in parallel,
  each its own .cwasm (DRC: compile 155 s, 56 GB, 663 MB .cwasm); `run-replay.sh` takes WASM_FLAGS and ports.
- 2026-10-04: **GC variants** (same build, same replay, 903 s after start; `t79/out-gc-*`, run in parallel):
  copying 256 MiB (T69's setting) 143–145 s CPU, 16.5×, RSS 431 MB; **copying 1 GiB 35.9 s (4.1× node), RSS 1.3 GB**;
  copying 4 GiB 36.4 s, RSS 4.4 GB; null (never collects) 37.0 s, RSS 3.4 GB (≈ the program allocates ~3.4 GB in 15
  min); DRC: 100% of a core the whole window, only 1,395 of 13,476 publishes (collapsed; its .cwasm 663 MB). So at
  1 GiB the GC costs ~nothing (= null): three quarters of the gap was the 256 MiB heap (constant copying
  collections of a live set near its semi-space). node `--cpu-prof` (its 96 s CPU is profiler overhead; busy 10.4 s):
  idle 98.8%; busy: module loading ~17%, GC 6.6%, zigbee2mqtt 12.5%, herdsman 7.5%, logging (winston, logform,
  fecha) ~8%, timeService.recalculateTimeData 3.3%. Running: heap sweep 384/512/768 MiB, perf of the 1 GiB variant
  (the remaining 4×). Mistakes on the way: a `$m` expanded by the outer ssh shell (zero-size heap .cwasm; those runs
  killed by PID; `pkill -f` must not match one's own ssh command line).
- 2026-10-04: heap sweep: 384 MiB 42.8 s (RSS 639 MB), 512 MiB 39.1 s (765 MB), 768 MiB 37.6 s (1.0 GB); first
  minute 15.9 / 14.8 / 14.4 s, 1 GiB 13.6 s (node 4.5 s, 256 MiB 37.9 s). perf at 1 GiB (`t79/out-perf-c1g`): p.cwasm
  79%, wasmtime 19%; inclusive `utf8ToBytes` 26.8% (`Buffer.byteLength` 20.6%, `utf8Write` 15.1%), `decodeChunk`
  9.7%, RegExp `Symbol.match` 4.1%, `flags` 2.5%, `Symbol.replace` 2.1%; self: `get_named` 7.7%, `find_own_named`
  6.7%, `get` 3.9%, `find_own` 3.8%, `box_number` 3.3%, `is_subtype*` 3.4%, `get_interned_func_ref` ~3.5%.
  Done: results in `docs/zigbee2mqtt-feasibility.md`, follow-ups F347–F351, F329/F318 ticked.
