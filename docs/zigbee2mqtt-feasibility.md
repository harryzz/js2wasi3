# Feasibility: zigbee2mqtt on js2wasi3 (WASI 0.3)

Sections: the production trial (2026-10-03), OTA downloads (T72), the frontend run (T71), the live run against a real coordinator and MQTT broker (T68, 2026-10-03), the measured survey of
2026-10-01 (T62, after T54–T61), then the original estimate of 2026-09-29 (kept as written).

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
(−34%; find 533 → 466 ms). About 20% of the remaining run is the benchmark's own checksum loop (F326). Not yet
re-measured on the live installation.

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
