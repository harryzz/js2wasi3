# Feasibility: zigbee2mqtt on js2wasi3 (WASI 0.3)

Sections: T93–T97 on the T79 replay (2026-10-09), T93/T94 profiled replays (2026-10-09), T92 `get_named` without `is_subtype` on the T79 replay (2026-10-09), T91 `Map` hashes on the T79 replay (2026-10-08), T90 property adds on the T79 replay (2026-10-06), T89 code-table calls on the T79 replay (2026-10-06), T86 `.length` paths on the T79 replay (2026-10-05), T85 hashed shape transitions on the T79 replay (2026-10-05), T83 shapes on the T79 replay (T84, 2026-10-05), T80+T81 on the T79 replay (T82, 2026-10-05), the profile of a recorded production session (T79, 2026-10-04), the production runs and test-rig runs of 2026-10-04 (after T76, T78), the production trial (2026-10-03), OTA downloads (T72), the frontend run (T71), the live run against a real coordinator and MQTT broker (T68, 2026-10-03), the measured survey of
2026-10-01 (T62, after T54–T61), then the original estimate of 2026-09-29 (kept as written).

## T93–T97 on the T79 replay 2026-10-09: 20.28 s

One unprofiled replay on a-03 of the T97 build (T93 object tests, T94 `string_hash` memo, T96 generic `get`/`set`
without `is_subtype`, T97 typed-array `.length` fast path). Run dir `~/js2wasi3-prod-run/t79/out-t97`.

| | CPU (903 s) | vs node (8.66 s) | ms CPU per event | publishes / errors |
|---|---|---|---|---|
| T90 | 21.47 s | 2.48× | 39.18 | 13,478 / 9 |
| T92 | 21.00 s | 2.42× | 38.32 | — |
| **T97** | **20.28 s** | **2.34×** | **37.01** | 13,478 / 9 |

- 0.72 s (3.4%) less than T92, 1.19 s (5.5%) less than T90; output = T90's (139 exact, 1 unmatched, the same 9
  error lines). Startup 11.0 s wall, 1.44 s CPU (as before).
- T98 (2026-10-09, after this run) reserves 16 MiB of GC heap at start (`build --gc-reserve`). The replay runs
  with `-O gc-heap-initial-size=536870912` (F347), so the reservation sits inside that heap and changes nothing
  here (not replayed). Without the flag, `--gc-reserve` is what a user gets by default; zigbee2mqtt's live set is
  larger than 16 MiB, so keep the 512 MiB flag (or a reservation of that order) for it.

## T93/T94 profiled replays 2026-10-09 (under perf, not comparable to unprofiled runs)

Replays with `PERF=` (perf `-g` at 499 Hz adds overhead): T93 (`out-t93`) 21.7 s, T94 (`out-t94`) 21.38 s CPU;
matching as T90 (139 exact, 1 unmatched). Self time (`-g none`), T93 → T94:

| | T93 | T94 |
|---|---|---|
| `get_named` | 8.01% | 8.06% |
| `find_own` | 5.14% | 5.27% |
| `find_own_named` | 4.56% | 4.98% |
| `get` (generic, computed keys) | 4.45% | 4.88% |
| `call_value` | 3.07% | 3.01% |
| `string_hash` | 2.59% | **1.56%** |
| `own_get` | 1.78% | 1.52% |
| `to_property_key` | 1.65% | 1.43% |
| `is_subtype*` (3 symbols) | 2.63% | ~2.6% |
| GC (`forward` + `scan_field` + `collect_increment`) | 6.0% | 5.9% |

- T94's memo removed ~40% of the hashing; the computed-key path (`get` → `lookup` → `own_get` → `find_own`, plus
  `to_property_key`, `string_hash`) is still ~14.7%, now mostly `find_own`'s probe and compare and `get`'s
  dispatch: a per-site cache for computed keys is the next step there (F363). Storing hashes in strings would save
  at most the remaining ~1.6%.

## T92 `get_named` without `is_subtype` on the T79 replay 2026-10-09: 21.00 s

One replay on a-03 of the T92 build (`get_named` tests and casts objects in one inline chain). Run dir
`~/js2wasi3-prod-run/t79/out-t92`.

| | CPU (903 s) | vs node (8.66 s) | ms CPU per event |
|---|---|---|---|
| T90 | 21.47 s | 2.48× | 39.18 |
| T91 (2 runs) | 21.51 / 21.26 s | 2.48× / 2.45× | 39.25 / 38.80 |
| **T92** | **21.00 s** | **2.42×** | **38.32** |

- 1–2% less than T91, within this replay's run-to-run spread; no loss (plain objects are this workload's
  property accesses, measured unchanged). Matching as T90 (139 exact, 7 by command, 2 by name, 1 unmatched);
  publishes and error lines not counted for this run.

## T91 `Map` hashes on the T79 replay 2026-10-08: 21.26 / 21.51 s

Two replays on a-03 of the T91 build (`Map`/`Set` hashes stored in the index slots, number keys hashed with 64-bit
mixing). Run dirs `~/js2wasi3-prod-run/t79/out-t91` and `out-t91b` (same `p.cwasm`).

| | CPU (903 s) | vs node (8.66 s) | first minute | peak RSS | publishes |
|---|---|---|---|---|---|
| T90 | 21.47 s | 2.48× | 6.2 s | 764 MB | 13,478 |
| T91 run 1 | 21.51 s | 2.48× | 6.4 s | 764 MB | 13,526 |
| **T91 run 2** | **21.26 s** | **2.45×** | — | — | 13,478 |

- CPU unchanged within noise (−1% / +0.2%): zigbee2mqtt doesn't spend its time in string-keyed `Map`s
  (knucleotide, which does: −13%).
- Run 2's output equals T90's (139 exact matches, 1 unmatched, the same 9 error lines). Run 1 differed (13,526
  publishes, 17 error lines, 7 unmatched requests: zigbee2mqtt's requests took recorded answers out of order, then
  `assocGetWithAddress` timeouts after 6,000 ms); the same build gave both results, so it is replay timing (run 1's
  startup took 12.2 s wall vs 10.5 s, with the same CPU). First sign that this replay isn't fully deterministic:
  compare a code change's output over two runs before calling it different.

## T90 property adds on the T79 replay 2026-10-06: 21.47 s

One replay on a-03 of 83c660b + T90 (a constant-key add skips the prototype-chain search when the site's cached
prototype and `G_EPOCH` still match). Run dir `~/js2wasi3-prod-run/t79/out-t90`.

| | CPU (903 s) | vs node (8.66 s) | first minute | peak RSS | publishes |
|---|---|---|---|---|---|
| T89 | 22.07 s | 2.55× | 6.3 s | 776 MB | 13,478 |
| **T90** | **21.47 s** | **2.48×** | **6.2 s** | 783 MB | 13,478 |

- 0.6 s (2.7%) less, same output (139 exact matches, the same 9 error lines). EarleyBoyer: ~13.2 → ~12.2 s
  (laptop), `find_own_named` 12.5% → 2.0% of its profile.

## T89 code-table calls on the T79 replay 2026-10-06: 22.07 s

One replay on a-03 of 808ef05 (shapes reverted; T87 traps, T88 `var`/parameter fix, T89: `$function` code as
code-table slots called with `call_indirect`, `F.prototype` direct). Run dir `~/js2wasi3-prod-run/t79/out-t89`.

| | CPU (903 s) | vs node (8.66 s) | first minute | peak RSS | publishes |
|---|---|---|---|---|---|
| T82 | 22.59 s | 2.6× | 6.5 s | 782 MB | 13,478 |
| **T89** | **22.07 s** | **2.55×** | **6.3 s** | 776 MB | 13,478 |

- 0.5 s (2%) less, same output (the same 9 error lines). EarleyBoyer gained 30% from the same change: zigbee2mqtt
  calls through function values far less per unit of work; its time is in property access (T84's profile).

## T86 `.length` paths on the T79 replay 2026-10-05: 22.18 s, the 21.5 s bar not reached

One replay on a-03 of 3239533 (T83 + T85 + T86: cached `.length` of typed arrays, plain objects' own `length` read
and written directly). Run dir `~/js2wasi3-prod-run/t79/out-t86`.

| | CPU (903 s) | vs node (8.66 s) | first minute | peak RSS | publishes |
|---|---|---|---|---|---|
| T82 (no shapes) | 22.59 s | 2.6× | 6.5 s | 782 MB | 13,478 |
| T85 (shapes, hashed transitions) | 22.63 s | 2.6× | 6.4 s | 777 MB | 13,478 |
| **T86** | **22.18 s** | **2.56×** | **6.3 s** | 777 MB | 13,478 |

- 0.45 s less than T85 (the profile suggested up to ~1.1 s for the `.length` paths); same output.
- The user's rule (T85): ≤ 21.5 s keeps the shapes work, else revert T83 + T85 + T86. **Reverted** 2026-10-05;
  the code is on branch `shapes` (8532edb). `main` is back to T82's code (22.59 s).

## T85 hashed shape transitions on the T79 replay 2026-10-05: back to T82

One replay on a-03 of 4c2b37d (T83 + T85: a shape hashes its transitions past 8, up to 4,096 instead of 64).
Run dir `~/js2wasi3-prod-run/t79/out-t85`.

| | CPU (903 s, 548 frames) | vs node (8.66 s) | first minute | peak RSS | publishes |
|---|---|---|---|---|---|
| T82 (no shapes) | 22.6 s (22.59) | 2.6× | 6.5 s | 782 MB | 13,478 |
| T83 shapes (2 runs) | 23.9 / 24.0 s | 2.8× | 6.8 s | 778 MB | 13,478 |
| **T85** | **22.6 s (22.63)** | **2.6×** | **6.4 s** | 777 MB | 13,478 |

- T83's loss is gone, but no gain over T82 on the real workload (the micro-benchmark gained 1.7–8×: object building
  isn't where zigbee2mqtt spends its time). RSS over the run grows as T82's (761 → 777 MB vs 769 → 781 MB).
- User's decision (2026-10-05): keep shapes and do F363 (computed keys, ~3.6 s in T84's profile) next; if the
  replay after F363 isn't clearly below T82 (≤ 21.5 s), revert all the shapes work.

## T83 shapes on the T79 replay 2026-10-05 (T84): no gain

One replay on a-03 of `main` at a77d0da (T80 + T81 + T83's shapes and shape-keyed inline caches), same recording
and 512 MiB heap; T82's run and node weren't re-run. Run dir `~/js2wasi3-prod-run/t79/out-t83`.

| | CPU (903 s, 548 frames) | vs node (8.66 s) | CPU per frame | first minute (CPU at 61 s) | peak RSS | publishes |
|---|---|---|---|---|---|---|
| js2wasi3 T80+T81 (T82) | 22.6 s | 2.6× | 41.2 ms | 6.5 s | 782 MB | 13,478 |
| **js2wasi3 + T83 shapes** | **23.9 s** | **2.8×** | **43.6 ms** | **6.8 s** | 778 MB | 13,478 |

- No improvement on the real workload: +6%, confirmed by a second run (23.96 s, `t79/out-t83-perf`, meant for perf
  but started without it), although
  T83's micro-benchmark gained on literals, methods and `JSON.parse` objects (1.4–2.1×). Same output (publishes, the
  same 9 error lines). Full test262 on the same commit: 0 regressions (T84).
- Why (perf on a third run, `t79/out-t83-perf`, 24.5 s with perf, 11,301 samples; self time):
  - **New cost: `shape_child` 5.7%** (~1.4 s, about the whole slowdown): finding or making a transition walks the
    shape's list of children comparing keys. Mostly from `define` (objects the runtime builds: iterator results
    `{value, done}` from async generators and array iterators) and `define_named` on literals whose site missed (F362).
  - **Misses are still common**: `get_named` 8.0% self, `find_own_named` 3.8% (1.9% via `lookup_named`, prototype
    walks), spread over many sites (the logger's `debug`, …): polymorphic sites (F357), unshaped receivers (F360).
  - **Computed keys aren't cached**: `get` → `lookup` → `own_get` → `find_own` ~12% self plus `string_hash` 2.2%,
    `to_property_key` 1.3% (F363).
  - Also: wasmtime `is_subtype*` ~2.8% and `get_interned_func_ref`/`FuncRefTable` ~2.2% (F350), GC ~4.5%,
    RegExp `$$reRun`/`$$reSearch` 2.9%, `box_number` 1.2%.

## T80+T81 on the T79 replay 2026-10-05 (T82): 2.6× node

One replay run on a-03 (same recording, 512 MiB GC heap) of `main` at 3c9aacb: T80 (native UTF-8 in `node:buffer`)
and T81 (RegExp fast paths) together. The baseline and node weren't re-run (T79's runs, below). Run dir
`~/js2wasi3-prod-run/t79/out-t80t81`.

| | CPU (903 s, 548 frames) | vs node (8.66 s) | CPU per frame | first minute (CPU at 61 s) | peak RSS | publishes |
|---|---|---|---|---|---|---|
| node 24.21.0 (T79) | 8.66 s | 1× | | 4.5 s | 289 MB | 13,567 |
| js2wasi3 before (T79, 512 MiB) | 39.1 s | 4.5× | 71.3 ms | 14.8 s | 784 MB | 13,476 |
| **js2wasi3 with T80+T81** | **22.6 s** | **2.6×** | **41.2 ms** | **6.5 s** | 782 MB | 13,478 |

- 42% less CPU in the steady window, 56% less in the first minute; RSS unchanged (peak of `cpu.csv`, 10⁶ bytes; the T79 table below gives MiB). The saving (16.5 s) is about
  what T79's profile gave UTF-8 encoding (~36%) and RegExp (~9%), so no new profile was taken.
- Same output as before: publishes within 2 of the baseline (node 13,567: F317), the same 9 error/exception lines
  in `z2m.log`.
- Build on a-03: `js2wasi3 build` 257 s / 1.7 GB, `wasmtime compile` with 512 MiB 50 s / 14.5 GB (F347's scripts work).
- Next candidates (T79's profile, now larger shares): property-access helpers (~27% self then, F321 shapes),
  wasmtime libcalls (~7%, F350), `box_number` (3%, F323/F334), one-byte strings (F352).

## Profile of a recorded production session 2026-10-04 (T79): the GC heap, then UTF-8

Since T79 the zigbee2mqtt run scripts and README use a 512 MiB GC heap (`-O gc-heap-initial-size=536870912`, F347);
wasmtime's rule for growing the heap (F304) is a limitation we work around, not report. T80+T81 (measured in T82, above): 22.6 s.

Production's zigbee2mqtt (52 devices) was recorded once for 15 minutes under node through two recording proxies
(coordinator and MQTT; `tasks/T79-files/`), then replayed offline on a-03 into node and into js2wasi3's build
(after T77 stage A): a fake coordinator answers each request with the recorded answer and replays the devices'
frames on the recorded timeline; a private mosquitto holds the recorded retained messages. node on the replay
publishes exactly what the recorded session published (13,567); repeated js2wasi3 runs agree within 1%.
CPU in the 903 s from zigbee2mqtt's start (548 device frames):

| js2wasi3 variant (same build) | CPU | vs node (8.66 s) | first minute (node 4.5 s) | RSS |
|---|---|---|---|---|
| copying GC, 256 MiB heap (used so far) | 143–145 s | 16.5× | 37.9 s | 431 MB |
| copying, 384 MiB | 42.8 s | 4.9× | 15.9 s | 639 MB |
| copying, 512 MiB | 39.1 s | 4.5× | 14.8 s | 765 MB |
| copying, 512 MiB, with T80+T81 (T82, 2026-10-05) | 22.6 s | 2.6× | 6.5 s | 763 MB |
| copying, 768 MiB | 37.6 s | 4.3× | 14.4 s | 1.0 GB |
| copying, 1 GiB | 35.9 s | 4.1× | 13.6 s | 1.3 GB |
| null collector (never collects) | 37.0 s | 4.3× | | 3.4 GB |
| DRC collector | 100% of a core, fell behind (1,395 of 13,476 publishes) | | | 1.0 GB |

- **With 256 MiB, wasmtime's copying collector is ~70% of all CPU** (`forward` 24%, `scan_field` 17%,
  `collect_increment` 14%, root tracing and type registry ~14%; perf on the replay): the live data nearly fills a
  semi-space, so it collects constantly. From 512 MiB on, the GC costs about what no GC costs: **3.7× less CPU for
  +330 MB RSS**. The program allocates ~3.4 GB in 15 minutes.
- **What's left (1 GiB, perf)**: compiled JS 79%, wasmtime 19% (GC ~3%, `is_subtype`/`get_interned_func_ref`
  libcalls ~7%). By inclusive time: UTF-8 encoding in `node:buffer`'s JS — `Buffer.byteLength` 20.6% and `utf8Write`
  15.1% (both via `utf8ToBytes`, which builds an array of bytes; reached from `Buffer.from(string)`, readable-stream's
  `decodeChunk`, socket writes) — ~36% of all CPU, from MQTT publishes, their echoes (zigbee2mqtt subscribes to its own
  topics) and the info-level log lines; RegExp built-ins
  ~9% (`Symbol.match` 4.1%, the `flags` getter 2.5%, `Symbol.replace` 2.1%); property-access helpers ~27% self
  (`get_named`, `find_own_named`, `get`, `find_own`, …), `box_number` 3.3%.
- node's profile of the same replay is idle 98.8%; its busy 10 s are spread out: module loading, its GC 6.6%,
  zigbee2mqtt 12.5%, herdsman 7.5%, logging ~8%.
- js2wasi3-only errors on the replay: external converters (F317) and once a herdsman frame parse error that
  doesn't reproduce frame by frame (F351).

## Production run 2026-10-04 evening (after T78): unchanged per event, `Intl` crash after 5 minutes

The user stopped production zigbee2mqtt; a fresh read-only snapshot of its data dir (17:20), the same overrides,
laptop and wasmtime flags as in the morning runs. Build: js2wasi3 `b2e6e8b` (T78's string `+=` buffers, regex
first-unit filter and native alternation search, native Buffer `latin1`/`ascii` decoding, F340, F341), zigbee2mqtt
2.14.1 with all device modules: 65.5 MB of Wasm (build 395 s, `wasmtime compile` 263 s). 52 devices. Planned 10
minutes; scripts and logs outside the repository (`~/js2wasi3-prod-run/run-prod3.sh`, `snapshot3.sh`, `prod3-wasm*`).

| | T76 build (morning) | T78 build (evening) |
|---|---|---|
| "Zigbee2MQTT started!" after | 23 s | 33 s |
| CPU, first 255 s | 100.3 s | 106.2 s |
| log lines (discovery publishes), first 255 s | 5,445 (600) | 5,462 (599) |
| **CPU per handled event** | **18.4 ms** | **19.4 ms** (node: 1.6 ms) |
| RSS | 506 MB | 494-499 MB in the first 2 minutes, then one step to 758 MB |
| errors | external converters' symlink (F317) | the same, only that |
| end | 10 minutes | `Intl` crash at 5:00 (F330) |

Per minute:

| minute | CPU | of one core | RSS peak | log lines |
|---|---|---|---|---|
| 1 | 50.7 s | 85% | 494 MB | 2,207 |
| 2 | 46.6 s | 78% | 499 MB | 1,262 |
| 3 | 5.1 s | 9% | 758 MB | 874 |
| 4 | 2.3 s | 4% | 758 MB | 818 |
| 5 | 3.4 s | 6% | 759 MB | 1,176 |

- **T78's fixes didn't change the real run**: ~19 ms per event, ~12× node, as before. They sped up the benchmarks
  they targeted (regexredux fasta 100k: over 120 s → 0.55 s; 200,000 string appends: 10.3 s → 19 ms); zigbee2mqtt's
  message handling spends its time elsewhere (property lookups, number boxing, GC: F334). The real workload needs
  its own profile (F329).
- Startup burst: about 2 minutes at ~80% of a core (discovery configs and retained state), then 4-9% of a core for
  800-1,200 events a minute.
- Memory: one step of ~260 MB during minute 3, then flat; it looks like wasmtime growing the GC heap, not a leak.
  Not verified (nor whether T78's `$strbuf` buffers add to it).
- Startup 33 s vs 23 s: one run each, not checked.
- **Crash (F330)** at 5:00: `Uncaught ReferenceError: node's global 'Intl' isn't available`, as in the morning's
  runs. Production runs end within minutes until `Intl` (T77).
- The MQTT listener on `zigbee2mqtt/#` logged 11,239 messages.

## Production runs 2026-10-04 (after T76): CPU per event, and the `Intl` crash

The user stopped production zigbee2mqtt again; a fresh read-only copy of its data dir, the same overrides, laptop and
wasmtime flags as on 2026-10-03 (above). Runs: the T76 build (10 min), then the 2026-10-03 trial build twice
(5 min planned each). Scripts and logs outside the repository (`~/js2wasi3-prod-run/run-prod.sh`, `snapshot.sh`).

**Raw totals mislead.** The same coordinator, devices and broker don't give the same work: zigbee2mqtt republishes
a Home Assistant discovery config only when the retained one differs, so the work depends on which zigbee2mqtt ran
last (production runs 2.14.2, the builds 2.14.1), and devices report when they like. Discovery publishes in the first
~4 minutes: 200 (10-03), 600 (T76 build today, after production had run), 1 (trial build today, right after the T76
build had left 2.14.1's configs). So compare **CPU per handled event** (every MQTT publish and device message is one
log line at level info) over the same window, first 255 s:

| run | CPU | log lines | discovery | CPU per line | vs node |
|---|---|---|---|---|---|
| node, 10-03 | 6.6 s | 4,118 | 199 | **1.6 ms** | 1× |
| trial build (before T74), 10-03 | 75.4 s | 4,033 | 200 | **18.7 ms** | ~12× |
| T76 build, 10-04 | 100.3 s | 5,445 | 600 | **18.4 ms** | ~12× |
| trial build, 10-04 (2nd try) | 43.3 s | 4,232 | 1 | 10.2 ms | not comparable (almost no discovery) |

- **T74–T76 didn't change the real run**: ~18.5 ms per event before and after, ~12× node. Their benchmark
  (zigbee-herdsman-converters' definitions and exposes, 3.6 s → 1.3 s) is a small slice of a real run; steady-state
  message handling dominates. Next speed work starts from a recorded real workload replayed offline (F329, F318).
- The T76 build: started in 23 s (10-03: 24 s), ~40 s at 100% of a core, then 16.7% of a core over 10 minutes,
  RSS 506 / 500 MB; errors as on 10-03 (external converters' symlink, F317; one retained `set` timed out, F318).
- **Crash (F330)**: both runs of the trial build stopped (after 85 s and 257 s) with `Uncaught ReferenceError:
  node's global 'Intl' isn't available`: a device read the coordinator's time cluster (`genTime`), and
  zigbee-herdsman's `getTimeClusterAttributes()` uses `Intl.DateTimeFormat` (and `@date-fns/tz`, which formats with
  `timeZoneName: "longOffset"`). The T76 build has the same code path; no device asked during its 10 minutes. Earlier
  runs (about 25 minutes on production in total, and the test rig, whose sensor never reads the time) hadn't met it.
  Fix: a full `Intl` in js2wasi3 (T77).

## Test rig after T78 2026-10-04: health check ends the program after 10 minutes

A long test-rig run with the build after T78 (5757a57) stopped 10 minutes after start: zigbee2mqtt's Health extension
runs every `health.interval` minutes (default 10) and calls `process.memoryUsage().rss`, which js2wasi3 lacks
(`Uncaught TypeError: process.memoryUsage is not a function`, F340). Earlier runs were 10 minutes or shorter and
never reached the first check. Fixed after the run (F340: `process.memoryUsage()` with 0s, WASI can't measure it).
The rerun with `ZIGBEE2MQTT_CONFIG_HEALTH_INTERVAL=600` ran 2 h 49 min (5.8 s CPU, RSS 461 MB, health checks every 60 s
`ok` in ~50-130 ms) until it was stopped; at about 2 h the coordinator's ESPHome stream server showed 0 TCP clients
while zigbee2mqtt still held an established socket: js2wasi3 ignored `setKeepAlive(true, 15000)`'s delay, so the
dead link would have been noticed only after the OS default 2 h of idle time (F341, fixed after the run).

## Test rig after T76 2026-10-04 (10 minutes, one device, frontend)

js2wasi3 after T76 (zigbee2mqtt 2.14.1 with all device modules: 65.5 MB of Wasm, build 291 s, `wasmtime compile`
159 s, 343 MB `.cwasm`; wasmtime 49.0.2, `-O gc-heap-initial-size=268435456`) on the test rig: the test coordinator
(CC2652P, Z-Stack 20230507, TCP) and the shared MQTT broker with test topics only (base topic `zigbee2mqtt-js2wasi3`,
discovery `homeassistant-js2wasi3`), starting from a test data dir saved on 2026-10-03 with the SNZB-02 (EFEKTA
firmware) joined; frontend on port 18080, log level info. Alongside: a read-only listener on the test topics,
`bridge/request/health_check` every 60 s, CPU/RSS every 5 s, a frontend client (pages, an asset, the `/api`
WebSocket) after start and after ~5 minutes.

| | js2wasi3 + wasmtime |
|---|---|
| "Zigbee2MQTT started!" after | 2 s (network resumed, 1 device) |
| device | its state published at start; at 05:44 it woke, `last_seen` updated and zigbee2mqtt configured it: the known `genPowerCfg.configReport` timeout of this firmware (as on 2026-10-03) |
| errors | that configure only |
| health check (10 requests) | all `ok`, answered in 50–73 ms |
| frontend | both client runs equal: pages (brotli), 404s, an asset, the WebSocket with the bridge topics and the device |
| CPU | 5.8 s in 10 minutes; 0.2% of a core after the first minute |
| RSS peak / mean | 451 / 446 MB (256 MiB of it the initial GC heap) |

With one device this isn't comparable to the production trial's 52-device numbers below; the startup burst there
(F318) is what T74–T76 work toward (the exposes benchmark: 3.6 s → 1.3 s). A first run of this session started
from the config-only `tasks/T71-files/data` (no devices); it also removed stale retained discovery entries under
`homeassistant-js2wasi3/` (the test namespace) for devices it didn't know, as zigbee2mqtt does.

## Production trial 2026-10-03 (after T72, wasmtime 49.0.2)

The user's production installation, for two 5-minute runs (production zigbee2mqtt stopped for the trial, restarted
after), js2wasi3 against node on the same data. Scripts, logs and the data copy stay outside the repository.

**Setup**
- Production: zigbee2mqtt 2.14.2 (zigbee-herdsman 10.10.0, zigbee-herdsman-converters 26.115.1) in a container on an
  aarch64 board. Coordinator: **SMLIGHT SLZB-06P7** (CC2652P7, Z-Stack 3.x.0 firmware 20240710) over TCP; MQTT
  without TLS; Home Assistant discovery on; frontend on; 3 external converters (a 435 KB generated converter for
  custom-firmware switches, a TRV override, a TY0201 sensor) and an `external_extensions/` file without a `.js`
  name (not loaded, on node either).
- Network: **52 devices**: 18 routers (mains), 34 end devices (28 battery, 5 mains, 1 DC); by manufacturer: Tuya 26,
  eWeLink (SONOFF) 8, IKEA 7, custom-firmware switches 5, and one each of SONOFF, Aqara (LUMI), Shelly, EfektaLab,
  Espressif, Texas Instruments (a CC2652 router). One group.
- Trial host: an x86-64 laptop (Intel Core i7-8565U, 4 cores / 8 threads, WSL2), wasmtime 49.0.2, node 24.21.0.
- Both sides ran **zigbee2mqtt 2.14.1** (zigbee-herdsman 10.9.2, zigbee-herdsman-converters 26.105.0,
  zigbee2mqtt-windfront 2.15.0; T62's npm copy) on a copy of production's data directory (configuration, database,
  coordinator backup, state, external converters, OTA files), against production's coordinator and MQTT broker.
  Overrides in the copy: frontend on `0.0.0.0:18080` without its auth token (port 80 needs root), log level `info`.
  RSS and CPU sampled every 5 s from `/proc`.

**Patches for the js2wasi3 build** (outside the compiler; node ran zigbee2mqtt unpatched):
- T62/T66 (`tasks/T66-files/patch.sh`, level 2, `T63_GLOBALS=0`): ajv's settings validators generated ahead of time
  (ajv standalone: no `new Function` at run time, F191), the dependency-version `import()` in `dist/util/utils.js`
  stubbed, native addons stubbed (`@serialport/bindings-cpp`, `unix-dgram`: TCP coordinators and no syslog only),
  `zemismart.js`'s `TextEncoder` line; T62's `main.js` driver (`index.js`'s `start()` with static requires, so the whole graph is linked at build time,
  and its git hash check; no dist rebuild).
- T71 (`tasks/T71-files/patch.sh`): srvx real, the frontend package imported by name (`zigbee2mqtt-windfront`).
- Trial only: `dist/extension/externalJS.js` imports an external converter from `ext/<name>` (the 3 files copied
  there and built with `--include 'ext/*.js'`) instead of `import()` of a temporary copy.
- Build: `--command main.js --include 'node_modules/zigbee-herdsman-converters/dist/devices/*.js' --include
  'ext/*.js'`: 5.6 min, 62 MB; `wasmtime compile -O gc-heap-initial-size=268435456`: 3.4 min, 9.6 GB peak (334 MB).
  Run: `wasmtime run -O gc-heap-initial-size=268435456 -Sp3 -S inherit-network -S allow-ip-name-lookup -S http`.

**Results** (5 minutes each)

| | node | js2wasi3 + wasmtime |
|---|---|---|
| network | resumed | resumed |
| "Zigbee2MQTT started!" after | 2 s | 24 s |
| CPU, first minute | a few s in total | ~45 s at 100% of a core (from ~10 s to ~50 s) |
| CPU, total in 5 min | 8.0 s | 78.2 s |
| CPU after the first minute, mean | 1.5% of a core | 11.6% (an earlier 10-minute run: ~13% at 9 min) |
| RSS peak / mean | 286 / 209 MB | 496 / 490 MB (256 MiB of it the initial GC heap, T69) |
| MQTT messages seen under the base topic | 8,288 | 8,168 |
| errors | none | 2 (below) |

What works like node: the network resume (no new network formed), the 52 devices with zigbee-herdsman-converters'
definitions, MQTT, Home Assistant discovery (~4,200 publishes), the frontend, live reports (switch actions,
metering, TRVs, sensors) and commands. What doesn't:
1. **External converters** fail to start: zigbee2mqtt symlinks its `node_modules` into `external_converters/` with
   an absolute target, which wasmtime's sandbox refuses (`EPERM: operation not permitted, symlink`). 9 devices then
   have other definitions than under node: the 5 custom-firmware switches fall back to generic ones (4–6 Home
   Assistant entities instead of 15–25), the TY0201 sensor (8 instead of 5), the 3 TRVs keep the built-in TRV801Z
   definition instead of the override (28 instead of 26) (F317). The build has the converters; the symlink isn't needed.
2. **Commands during the startup burst time out**: the broker holds retained `…/set` messages (an installation
   detail: for two switches and a bulb), which every start executes; node sends them fine, js2wasi3 failed one per
   run (`SRSP - AF - dataRequest after 6000ms`), as its single core is busy for ~45 s and the coordinator's reply
   is read too late (F318, with F303's property-access speed).
3. **Local time is UTC** (F184): `last_seen` reads `…+00:00` where production writes `…+03:00` (same instant).
4. Stopping it with a signal (`timeout`) ends wasmtime without zigbee2mqtt's shutdown, so `bridge/state` stays
   `online` (retained) until the next start (F319: WASI has no signals).
Not tried: the production version (2.14.2), an aarch64 host (the board has ~3 GB free; `wasmtime compile` needs
~9.6 GB, so a cross-compiled `.cwasm` would be needed), OTA updates, runs longer than 10 minutes.

**Speed work since the trial** (zigbee-herdsman-converters' definitions and exposes for the trial's 52 device types,
`tasks/T74-files/bench.sh`; exposes ×200, node 62–73 ms): T74 property access 3,602–3,712 → 2,529–2,732 ms (−29%); T75
operators, conversions and boxing 2,438 → 1,941 ms (−20%); T76 JSON, RegExp `replace`, regex search 1,950 → 1,285 ms
(−34%; find 533 → 466 ms). About 20% of the remaining run is the benchmark's own checksum loop (F326). On the live
installation (2026-10-04, "Production runs" above) the CPU per handled event didn't change (~18.5 ms, ~12× node): the
benchmark wasn't representative.

## OTA downloads 2026-10-03 (T72: `fetch`)

zigbee-herdsman's `dist/controller/helpers/ota.js` is the graph's only `fetch` user: the OTA index
(`Koenkk/zigbee-OTA`'s `index.json`, ~600 KB) and firmware images. With `fetch` over `wasi:http`, the build imports
`wasi:http/types` and `client` (herdsman's module loads at start), so runs need wasmtime's `-S http` (added to the
T68/T69/T71 scripts); T68's live run builds (148 s) and starts like node. The live check
(`bridge/request/device/ota_update/check`) can't reach `fetch` with the test device: the EFEKTA SNZB-02 has no `genOta`
output cluster, so herdsman answers "No endpoint found with OTA cluster support" on node too (F316). Instead
`tasks/T72-files/run-ota.sh` runs herdsman's helpers directly: `getOtaIndex`, `getOtaFirmware` of the smallest image
(sha512 checked, `parseOtaImage`), a bad hash, a 404 index and a DNS failure print node's output.

## Frontend 2026-10-03 (T71: `http` server, `ws`, srvx)

With `frontend: {enabled: true, host: 127.0.0.1, port: 18080}` in the test-only config
(`tasks/T71-files/build-run.sh`; patches: T66's, without the srvx stub, and the computed
`import(settings.frontend.package)` written as `import("zigbee2mqtt-windfront")`), the release build takes 91.6 s
(peak 774 MB) and `wasmtime compile` 40.9 s. zigbee2mqtt starts like node (`Started frontend on port
127.0.0.1:18080`, `Zigbee2MQTT started!`) and a client (`client.cjs`: `/` plain and brotli, `/index.html`, 404s
from `/missing` and `/device_icons/…`, a `.br` asset, the `/api` WebSocket for 5 s with a health check) gets the
same answers from both: the pages, the WebSocket's bridge topics (`bridge/state`, `bridge/info`, `bridge/devices`,
…, `bridge/response/health_check`) and its close. Not tried: the onboarding page (F311), `https` (F306).

## Live coordinator 2026-10-03 (T68, js2wasi3 after T62–T66 and `Error.captureStackTrace`)

**zigbee2mqtt 2.14.1 runs on wasmtime 49.0.1 against a real Z-Stack coordinator and MQTT broker, and does what node
does.** Rig: a test-only CC2652P (Z-Stack 3.x.0 20230507) behind ESPHome's `stream_server` (`tcp://…:6638`) and an
MQTT broker shared with a production instance, so the config ([`tasks/T68-files/data`](../tasks/T68-files/data)) uses
only test topics (`zigbee2mqtt-js2wasi3`, `homeassistant-js2wasi3`) and its own network (PAN, extended PAN, key,
channel 25). Build: T66's (no built-in stubs; T62's level-1/2 patches still in, F273), `T63_GLOBALS=0`, 105 s /
725 MB, `wasmtime compile` 52 s / 2.1 GB. Scripts: [`tasks/T68-files/`](../tasks/T68-files) (`build-run.sh`,
`restart-run.sh`, `watch.sh`, `mqtt-probe.js`); details in [T68](../tasks/T68-capture-stack-trace.md).

What runs, node and js2wasi3 alike: socket connect, bootloader skip, `SYS ping`, version, every NV read, network
start (`zigbee-herdsman started (reset)` on a new network, `(resumed)` after), the coordinator backup, `Currently 0
devices are joined`, the MQTT connect, `bridge/state` online, `bridge/info`/`devices`/`groups`/… (retained), Home
Assistant discovery (the same payloads, or "already discovered" from retained configs), and on an MQTT restart
request the whole stop path (offline state, MQTT disconnect, herdsman stop, final backup, `closing`) and a second
start. Normalized logs (timestamps, data paths, unpi frames stripped): 2,062 lines for a start and 4,094 for a
start-restart-start equal node's apart from:
- `Starting zigbee-herdsman (T62 stub)`: the version is read with `import()` of `${depend}/package.json`, still
  patched (F273);
- startup is slow: 3–5 s from the MQTT connect to `Zigbee2MQTT started!` (node: < 1 s), so mqtt.ts's 2 s
  republish timer fires and `bridge/state` online is published twice. Compute, not I/O: mqtt.js round trips are
  ~2× node's (F299). **T69**: half of it was wasmtime's copying GC thrashing (the live set, ~30 MiB, fills the
  semi-space before the heap grows); with `-O gc-heap-initial-size=268435456` (now in `tasks/T68-files/`) and T69's
  type tests, connect → started is 1 s with one `online` publish, as in node;
- the old socket's `close` on a restart is logged after the new socket opens (node: before the restart, F300).
  **T70**: T62's driver now does `index.js`'s git hash check (`exec`), and shell commands fail as with a real
  `/bin/sh` (exit 127 after a turn): `Port closed` is where node logs it;
- SIGTERM/SIGINT don't reach the guest, so a host kill skips zigbee2mqtt's stop path (F298);
- timestamps are UTC (node: local time, F182).
Nothing reached the production topics (watched for the test instance's names and PAN); retained test topics were
cleared afterwards.

## Live device 2026-10-03 (after T70: a temperature/humidity sensor on the test network)

A real end device, a SONOFF SNZB-02 with EFEKTA firmware (`SNZB-02_EFEKTA`, a sleepy battery device, taken from the
production network for the test), joined the test coordinator under **node** (permit join for 254 s on the test topic
`zigbee2mqtt-js2wasi3/bridge/request/permit_join`, closed right after): interview 2 s, identified from
zigbee-herdsman-converters' `devices/efekta.js`. Configuring fails on node too: the firmware never answers
`genPowerCfg.configReport` (battery reporting, 10 s timeout), and that aborts the configure before temperature and
humidity are bound (production's record of the same device: three bindings, no configured reporting). Binding them
by hand (`bridge/request/device/bind`, the device woken by a button press) made it report: 21.95 °C, 45.54 %.

Then **js2wasi3** on a copy of node's data directory (the device in `database.db`):
- The usual build (no device modules) **fails to start**: `Error: Cannot find module './devices/efekta'`. Definitions
  of known devices are resolved at start-up, so F272 stops zigbee2mqtt as soon as one device is joined.
- Built with `--include 'node_modules/zigbee-herdsman-converters/dist/devices/*.js'` (`Z2M_DEVICES=1` in
  `tasks/T68-files/build-run.sh`; 261 s and 1.6 GB peak memory to build, a 60 MB module, `wasmtime compile` 133 s and
  9.3 GB peak) it starts like node (`Currently 1 devices are joined`, the device identified), and decodes the reports
  like node (`msTemperatureMeasurement {"measuredValue":2065}` → `"temperature":20.65`); the state payloads have the
  same fields and values. The configure retries and `Data confirm error (…,240,…)` (0xF0 MAC transaction expired: a
  message queued for the sleeping device that it didn't poll for in time) are the device's, as under node. Not
  checked: a humidity report (none came in 3 minutes), whether node's first state publish after a restart also has
  `linkquality: null`.
Nothing reached the production topics; the retained test topics were cleared afterwards.


Same release: zigbee2mqtt 2.14.1 (master `3c3d8c1`, still the latest), from the npm tarball (`dist/` included),
`npm install --omit=dev --ignore-scripts`: 99 packages, ~34 MB of `.js`/`.json` (with inline source maps).
zigbee-herdsman-converters has **390** device modules (8.7 MB of JS; the old "1,560 files" counted `.d.ts` and maps
too). Scripts and exact commands: [`tasks/T62-files/`](../tasks/T62-files) and [T62](../tasks/T62-zigbee2mqtt-survey.md).

### Verdict
Closer than estimated. With source patches and stubs standing in for what's missing (all listed below), the
**whole program builds, validates, and runs on wasmtime 49.0.1 to the same point as node**: it reads and migrates
`configuration.yaml`, starts logging (winston, the same lines as node), loads zigbee-herdsman, validates the settings and
logs `zh:zstack:znp: Opening TCP socket with 127.0.0.1:6638`. node gets `ECONNREFUSED` there; js2wasi3 needs
`net.Socket`. Every gap is known and counted; none looks like it needs a design change. Scale works but is slow: 3 min to build,
1.7 min and 8.8 GB to `wasmtime compile`, with every device module.

### Target path (node 24.21.0)
An empty data dir starts the onboarding web server (`http`, port 8080). With a `configuration.yaml` (frontend off,
`serial.port: tcp://127.0.0.1:6638`, `adapter: zstack`; `ZIGBEE2MQTT_DATA` names the dir) and
`Z2M_ONBOARD_NO_SERVER=1`, node migrates v4→v5, opens the TCP socket, fails with `ECONNREFUSED` and exits 1. That is
the path the survey follows. `index.js` asks git for the commit hash (`child_process.exec`) and loads `dist/` with
`import()`, so build errors there become run-time rejections: the survey builds a driver (`main.js`: `index.js`'s
`start()` with static `require`s).

### Build errors (whole graph, 2.14.1)
**Link phase: 42** (one run lists them all since T62):
| Cause | Count | Packages |
|---|---|---|
| node built-ins js2wasi3 lacks | 37 | `net` 7, `crypto` 9, `http`/`https`/`http2` 8, `tls` 4, `dgram` 3, `zlib` 3, `child_process` 2, `url` 2, `dns` 1, `perf_hooks` 1: mqtt, ws, srvx, winston(-syslog), socks, multicast-dns, zigbee-herdsman, zigbee-on-host, @serialport/bindings-cpp, zigbee2mqtt |
| `new Function` of run-time code | 1 | ajv (settings schema) |
| `import()` the build can't work out | 2 | zigbee2mqtt: external JS (temp file), `${depend}/package.json` (bare pattern) |

**Lowering: 150** (with the built-ins stubbed; ~40 of them follow from another error, marked "cascade"):
| Cause | Count | Packages |
|---|---|---|
| web globals named in code (`typeof x !== "undefined"` guards included): `navigator` 31, `URL` 21, `Response` 7, `ReadableStream` 6, `Blob` 5, `Headers` 3, `TextDecoder` 3, `TextEncoder` 2, `fetch` 2, `crypto` 1, `WebSocket` 1, `MessageChannel` 1 | 83 | debug (27, browser build), mqtt, srvx, zigbee-herdsman, ws, fflate, worker-timers, broker-factory, zigbee2mqtt |
| `Reflect.decorate` (tsc's `__decorate`, a `typeof` check) | 15 | zigbee2mqtt |
| `Object.getOwnPropertyDescriptors` | 7 | readable-stream 2/3, js-yaml, broker-factory |
| `DataView` | 5 | zigbee-herdsman, converters (philips), srvx, zigbee2mqtt |
| `Intl` | 3 | zigbee-herdsman (time zone), @date-fns/tz |
| `console._stdout`/`_stderr` | 6 | winston console transport |
| a rest parameter with `arguments` | 1 | winston `Logger.log` (was a compiler panic until T62) |
| `delete a?.[b]` | 1 | zigbee2mqtt settings |
| a `var` read in a `for` condition before its declaration in the body | 1 | @babel/runtime regenerator |
| `Proxy` | 1 | srvx |
| "used before its declaration" (cascade of the above) | ~38 | zigbee2mqtt, srvx, winston, ws, herdsman, converters |

Device modules (`--include`): 2 more (`TextEncoder`/`TextDecoder` in `zemismart.js`). Packages not on the run path
also fail alone: argparse (`arguments` in a class constructor), tslib (`Reflect.decorate`), require-from-string
(`module`), worker-factory (`MessageChannel`).

### Run-time failures (after the stubs), in order
1. fflate: `new TextEncoder` at load.
2. `@serialport/bindings-cpp`: node-gyp-build throws at load (no native build for wasm32); `unix-dgram` likewise.
3. `require(".")`: js2wasi3 took `.` as a package name (fixed in T62).
4. ajv compiles the settings schema with `new Function` at load (patched: ajv standalone validators generated at
   build time, same errors as node).
5. srvx needs `globalThis.Request`/`Response`/`Headers` at load (frontend/onboarding only: stubbed).
6. winston: `%TypedArray%.prototype[Symbol.toStringTag]`'s getter is missing (safe-stable-stringify at load).
7. `child_process.exec` (git hash).
8. `error.stack` traps (the built-in sentinel): zigbee2mqtt logs it on a start failure (62 `.stack` reads in the graph).
9. `URL` (herdsman checks `tcp://host:port`), then `url.urlToHttpOptions`.
10. `net.Socket`: the end of the road (node's `ECONNREFUSED` point).

Each package alone (`require("<pkg>")`, wasmtime vs node, with the stubs): 90 of 106 print the same as node; 5 don't
build (argparse, require-from-string, tslib, worker-factory, worker-timers-worker); of the 11 that differ, 7 have no
main (both fail, other messages) and nan prints a path relative to the working directory (wasmtime's is `/`); the
real ones are winston, winston-syslog, safe-stable-stringify (item 6) and srvx (item 5).

### Scale
| Build | Compile (release) | Peak RSS | `.wasm` | `wasmtime compile` | Peak RSS |
|---|---|---|---|---|---|
| zigbee2mqtt, no device modules | 73 s | 0.7 GB | 32 MB | 20 s | 1.7 GB |
| zigbee2mqtt, all 390 device modules | 186 s | 1.5 GB | 59 MB | 102 s | 8.8 GB |
| converters + 50 / 100 / 200 device modules (0.5 / 1.6 / 3.2 MB JS) | 56 / 69 / 89 s | 0.6–0.9 GB | 25 / 28 / 35 MB | 17 / 21 / 30 s | 1.3–2.2 GB |
| zigbee-herdsman alone | 33 s | 0.35 GB | 16 MB | 8 s | |
| mqtt alone | 14 s | 0.13 GB | 5 MB | 4 s | |

- The compiler is linear in the graph: ~12–13 s and ~3.9 MB of Wasm per MB of device JS. **Lowering is 96% of the
  time**: whole-program type inference runs the lowering 11–12 times until its facts settle (~6 s per round without
  devices, ~14.5 s with them; 31.5k / 62.6k functions); parse + link 1.6 / 3.6 s, backend 0.8 / 1.7 s, packaging
  0.3 / 0.5 s.
- `wasmtime compile` grows faster than the module in memory (1.7 → 8.8 GB for 32 → 59 MB).
- Run time to the socket: ~0.6–1 s on wasmtime (precompiled) vs 0.6 s in node.

### Next tasks, in the order that unblocks the most
1. **Small language and built-in gaps** (T63): `error.stack`, unknown globals as run-time ReferenceErrors (typeof
   guards), missing statics read as `undefined` (`Reflect.decorate`), `Object.getOwnPropertyDescriptors`,
   `%TypedArray%.prototype[@@toStringTag]`, a rest parameter with `arguments`, `delete a?.[b]`, `var` read before its
   declaration in a loop, `console._stdout`/`_stderr`, error locations in uncaught errors.
   *Done in T63 (2026-10-01)* except error locations (F275): 7 patch blocks of `tasks/T62-files/patch.sh` went
   away (`tasks/T63-files/`), and without the web-global stubs the graph builds with no errors; it then stops at
   herdsman's `new URL(path)` (`Invalid TCP path`).
2. **`URL`/`URLSearchParams` and node's `url`; `TextEncoder`/`TextDecoder`; `DataView`.** (T64)
   *Done in T64 (2026-10-02)*: without the `web.js` stubs (`T63_GLOBALS=0 tasks/T63-files/build-run.sh`) the
   graph builds (79.7 s, 32.3 MB) and runs to `Opening TCP socket with 127.0.0.1:6638`, then the T62
   `net.Socket` stub, as with the stubs.
3. **node built-ins that only need to load** (`child_process`, `dns`, `dgram`, `tls`, `http(s)`, `http2`, `zlib`,
   `perf_hooks`, `module`) and a **`crypto` subset** (`randomBytes`, `randomUUID`, `createHash`, AES).
   *Done in T65 (2026-10-02)*: with only `net` stubbed (`T63_GLOBALS=0 tasks/T65-files/build-run.sh`) the graph
   builds (89.0 s, 32.5 MB) and runs to `Opening TCP socket with 127.0.0.1:6638`, then the `net.Socket` stub.
4. **`net.Socket` over `wasi:sockets`** in the event loop: the milestone's blocker.
   *Done in T66 (2026-10-02)*: with no built-in stubs (`T63_GLOBALS=0 tasks/T66-files/build-run.sh`) the graph
   builds (93.8 s, 32.6 MB) and, with no coordinator listening, ends like node: `Socket error Error: connect
   ECONNREFUSED 127.0.0.1:6638`, `Error while opening socket`, exit 1.
5. **Compiler speed**: fewer inference rounds; `wasmtime compile` memory.
6. **Milestone**: zigbee2mqtt patches (ajv standalone, srvx lazy, version lookup, external JS off, native addons)
   and the device modules (`--include`), run against a TCP coordinator stand-in and mosquitto.

## Original estimate (2026-09-29)

Investigated 2026-09-29 (after T45) against [zigbee2mqtt](https://github.com/koenkk/zigbee2mqtt) 2.14.1 (commit
`3c3d8c1`), production dependencies installed with `npm install --omit=dev --ignore-scripts`. Parked: nothing here
is scheduled; the items feed the Follow-ups table (F194, F200, F98, F182, F191).

### Verdict
Possible, but a multi-task project. Realistic scope: a **network (TCP) Zigbee coordinator**, **plain MQTT over TCP**,
**no web frontend** at first, **no "external JS"** (user converters/extensions). Roughly 6–8 tasks the size of T41
or T45; most of them (CommonJS, node built-ins, sockets) are useful for any real node program.

### What it is
| Package | JS size | Notes |
|---|---|---|
| zigbee2mqtt `lib/` | ~13k lines of TypeScript | compiled by `tsc` to **CommonJS** (`index.js` uses `require`, no `"type": "module"`) |
| `zigbee-herdsman` | 3 MB | Zigbee stack; adapters z-stack, ember, zboss, deconz, zigate, zoh |
| `zigbee-herdsman-converters` | 11 MB (20 MB with maps) | 1,560 device files, loaded lazily by a computed `require("./devices/" + name)` |
| `mqtt` | 1.6 MB | MQTT client |
| others (~95) | small | `winston`, `ajv`, `js-yaml`, `ws`, `srvx`, `readable-stream`, `buffer`, `events`, `fflate`, `semver`, … |

Native addons: `@serialport/bindings-cpp` (serial ports), `unix-dgram` (syslog).

### Blockers, in order
1. **CommonJS (F194).** Nearly the whole graph is CommonJS (the app's `tsc` output, herdsman, converters, `mqtt`,
   `winston`, `ajv`), with `__importStar`/`__importDefault` helpers, JSON `require`s and cycles. The converters' lazy
   device loading is `Promise.resolve(\`./devices/${name}\`).then(s => require(s))`: needs T41-style specifier
   patterns (or `--include`) for `require`.
2. **node's built-in modules (F200; T55 did `assert`, `path`, `events`, `util`, `os`, `tty`, `process`; the rest: F238).** Counted over the graph: `assert` 75, `fs` 40, `path` 38, `events` 37, `util` 36,
   `buffer` 36 (`Buffer.` ~1,500 uses), `stream` 34, `os` 23, `net` 15, `crypto` 13, `http`/`https`/`tls`, `dgram`,
   `child_process`, `tty`, `zlib`, `worker_threads`.
   - pure JS, from npm polyfills compiled with the program: `events`, `buffer`, `stream` (`readable-stream`),
     `util`, `assert`, `path`, `string_decoder`;
   - real WASI implementations: `fs` (sync API used: `readFileSync`, `writeFileSync`, `existsSync`, `renameSync`,
     `unlinkSync`, `rmSync`, `readdirSync`, `mkdirSync`, `copyFileSync`, `symlinkSync`, `statSync`, `lstatSync`,
     `openSync`/`fsyncSync`/`closeSync`) over `wasi:filesystem`; `net.Socket` (TCP client) over `wasi:sockets`;
     `crypto` (`randomBytes`/`randomUUID` via `wasi:random`, `createHash`, AES `createCipheriv` for Green Power);
     `process` (`env`, `argv`, `exit`, `uptime`, `memoryUsage`, signal handlers: stubs) and `os` (`platform`,
     `totalmem`, `cpus`, …: stubs);
   - stubs: `child_process` (git hash at start), `dgram` (mDNS adapter discovery, optional).
3. **Sockets in the event loop.** MQTT and the coordinator are long-lived TCP streams: `wasi:sockets` reads/writes
   must join the command's P3 callback loop (like timers and WIT `stream<T>`, T19/T20). New runtime work.
4. **Language/runtime gaps.**
   - `WeakMap` (F98) (`mqtt`, `abort-controller`, `event-target-shim`, …), `FinalizationRegistry` (`mqtt`).
   - `Intl.DateTimeFormat().resolvedOptions().timeZone` (herdsman's time service, F182): a `"UTC"` stub is enough.
   - `ajv` compiles the settings schema at run time with `new Function` (F191): generate the validators at build
     time with ajv's standalone mode (a patch to `lib/util/settings.ts`).
   - External converters/extensions are written to a temp file and `import()`ed at run time (F191): disable.
5. **Scale (the biggest unknown).** Only small programs have been compiled so far; ~16 MB of JS (all 1,560 device files
   included) through whole-program inference and closure analysis may be slow and give a very large `.wasm`.
   Expect performance work.
6. **Hardware.** WASI has no serial-port API (no termios, no baud rate), so USB sticks can't be opened directly. Use
   a network coordinator (herdsman's adapters accept `tcp://host:port`) or `ser2net` on the host.
7. **Optional, later.** The web frontend (`srvx` HTTP server + `ws` WebSockets + static files) needs TCP listen and an
   HTTP server; onboarding starts one too (skipped when a config file exists). `mqtts://` needs TLS (no stable
   `wasi:tls` in WASI 0.3.1).

### Suggested order
1. **Scale probe** (small, first): bundle zigbee2mqtt into one ES module with esbuild (a one-off tool outside the
   repo) and build it with js2wasi3: see what breaks and how compile time and output size behave on ~16 MB.
2. CommonJS (F194), with computed `require` patterns.
3. node built-ins: the pure-JS ones, `process`, `os`, `path`.
4. `fs` (sync) over `wasi:filesystem`.
5. `net.Socket` over `wasi:sockets` in the event loop.
6. `crypto` subset, `WeakMap`, `Intl` time-zone stub.
7. zigbee2mqtt patches: precompiled ajv schemas, external JS off, frontend off.
8. Milestone: zigbee2mqtt on wasmtime (`-S inherit-network`) with a TCP coordinator and a local mosquitto.
