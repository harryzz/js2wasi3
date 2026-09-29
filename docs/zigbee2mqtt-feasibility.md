# Feasibility: zigbee2mqtt on js2wasi3 (WASI 0.3)

Investigated 2026-09-29 (after T45) against [zigbee2mqtt](https://github.com/koenkk/zigbee2mqtt) 2.14.1 (commit
`3c3d8c1`), production dependencies installed with `npm install --omit=dev --ignore-scripts`. Parked: nothing here
is scheduled; the items feed the Follow-ups table (F194, F200, F98, F182, F191).

## Verdict
Possible, but a multi-task project. Realistic scope: a **network (TCP) Zigbee coordinator**, **plain MQTT over TCP**,
**no web frontend** at first, **no "external JS"** (user converters/extensions). Roughly 6–8 tasks the size of T41
or T45; most of them (CommonJS, node built-ins, sockets) are useful for any real node program.

## What it is
| Package | JS size | Notes |
|---|---|---|
| zigbee2mqtt `lib/` | ~13k lines of TypeScript | compiled by `tsc` to **CommonJS** (`index.js` uses `require`, no `"type": "module"`) |
| `zigbee-herdsman` | 3 MB | Zigbee stack; adapters z-stack, ember, zboss, deconz, zigate, zoh |
| `zigbee-herdsman-converters` | 11 MB (20 MB with maps) | 1,560 device files, loaded lazily by a computed `require("./devices/" + name)` |
| `mqtt` | 1.6 MB | MQTT client |
| others (~95) | small | `winston`, `ajv`, `js-yaml`, `ws`, `srvx`, `readable-stream`, `buffer`, `events`, `fflate`, `semver`, … |

Native addons: `@serialport/bindings-cpp` (serial ports), `unix-dgram` (syslog).

## Blockers, in order
1. **CommonJS (F194).** Nearly the whole graph is CommonJS (the app's `tsc` output, herdsman, converters, `mqtt`,
   `winston`, `ajv`), with `__importStar`/`__importDefault` helpers, JSON `require`s and cycles. The converters' lazy
   device loading is `Promise.resolve(\`./devices/${name}\`).then(s => require(s))`: needs T41-style specifier
   patterns (or `--include`) for `require`.
2. **node's built-in modules (F200).** Counted over the graph: `assert` 75, `fs` 40, `path` 38, `events` 37, `util` 36,
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

## Suggested order
1. **Scale probe** (small, first): bundle zigbee2mqtt into one ES module with esbuild (a one-off tool outside the
   repo) and build it with js2wasi3: see what breaks and how compile time and output size behave on ~16 MB.
2. CommonJS (F194), with computed `require` patterns.
3. node built-ins: the pure-JS ones, `process`, `os`, `path`.
4. `fs` (sync) over `wasi:filesystem`.
5. `net.Socket` over `wasi:sockets` in the event loop.
6. `crypto` subset, `WeakMap`, `Intl` time-zone stub.
7. zigbee2mqtt patches: precompiled ajv schemas, external JS off, frontend off.
8. Milestone: zigbee2mqtt on wasmtime (`-S inherit-network`) with a TCP coordinator and a local mosquitto.
