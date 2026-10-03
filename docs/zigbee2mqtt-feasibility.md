# Feasibility: zigbee2mqtt on js2wasi3 (WASI 0.3)

Two sections: the measured survey of 2026-10-01 (T62, after T54–T61) first, then the original estimate of
2026-09-29 (kept as written).

## Survey 2026-10-01 (T62, js2wasi3 after T61)

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
