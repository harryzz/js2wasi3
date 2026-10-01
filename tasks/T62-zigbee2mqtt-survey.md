# T62: zigbee2mqtt build survey: what fails now, and the plan to the milestone

Status: done (2026-10-01) | Depends on: T54–T61

## Goal
Build zigbee2mqtt with js2wasi3 as it stands after T61 and turn the result into a measured, ordered plan:
1. every build error and run-time failure, grouped by cause, with counts and the packages that hit them;
2. scale numbers: compile time (release), `.wasm` size, `wasmtime compile` time, and how they grow with the graph;
3. an updated `docs/zigbee2mqtt-feasibility.md` (written 2026-09-29, before T54–T61) and the next tasks as follow-ups
   (or task files), in the order that unblocks the most.
This is a survey: fix only what is tiny and clearly in the way of seeing further (record everything else).

## What we know (at handoff)
- `docs/zigbee2mqtt-feasibility.md`: zigbee2mqtt 2.14.1 (commit `3c3d8c1`), production deps
  (`npm install --omit=dev --ignore-scripts`); ~16 MB of JS; `lib/` is tsc output (CommonJS); `zigbee-herdsman`,
  `zigbee-herdsman-converters` (1,560 device files loaded by a computed `require`), `mqtt`, `winston`, `ajv`,
  `js-yaml`, `ws`, `readable-stream`, …; native addons `@serialport/bindings-cpp`, `unix-dgram`.
- Done since that survey: CommonJS with computed `require` patterns and `--include` (T54), node built-ins `path`,
  `events`, `util`, `assert`, `os`, `tty`, `process` (T55), `Buffer`/`buffer`/`string_decoder` (T56), linear-time string
  constants and compile-time fixes (T57, T58), `fs` over `wasi:filesystem` in commands (T59), `stream` (node 24's own),
  fs streams, `fs.opendir` (T60), `WeakMap`/`WeakSet`, abort/event globals, `console.assert`, npm `readable-stream`
  in packages (T61).
- Known still missing (Follow-ups): `crypto`, `net`/`dgram`/`http`/`https`/`tls`, `child_process`, `zlib`,
  `worker_threads`, `url`, `querystring` (F238); `FinalizationRegistry`/`WeakRef`; `Intl` (F182: a time-zone stub is
  enough); `new Function`/`eval` of run-time code (F191: ajv compiles schemas at run time; external converters);
  `console.trace` (F253); sockets in the event loop (no task yet); `globalThis.X` for linker-bound globals (F259).
- Scale is the biggest unknown: the largest programs so far are ~1.7 MB of Wasm (`fs-extra` probe: release build
  a few seconds). F255: the compiler emits ~220 KB/s; a stream program takes 4.4 s to build and 1.9 s in
  `wasmtime compile`. The device files alone are ~11 MB of JS.
- A built-in js2wasi3 lacks is a build error where its `require` always runs, else thrown when it runs (T54's rule):
  expect many build errors at first. `--include` covers computed `require`s the build can't work out.

## Plan
1. Set up in the scratchpad: zigbee2mqtt at the latest release (record the version and commit), `npm install
   --omit=dev --ignore-scripts`; node 24.21.0 runs it far enough to need a config (record what node does with an
   empty `data/` dir, e.g. onboarding vs. `configuration.yaml`: aim for the path that doesn't start a web server).
2. Build the entry (`index.js` → `lib/` tsc output) with `target/release/js2wasi3 build --command`; collect the
   diagnostics (all of them: the build reports many per run), group by message, count per package. Record compile
   time and peak memory (`/usr/bin/time` may be missing: use `date +%s%N` and `/proc`-based or `ps` sampling).
3. Shrink to find the scale curve: build sub-graphs (`zigbee-herdsman` alone, the converters with N device files via
   `--include` globs, `mqtt` alone, `winston`, `ajv`) and record time/size for each; note any superlinear phase
   (T57/T58's method: `WASMTIME_LOG=wasmtime_internal_cranelift=debug` for wasmtime; for the compiler, time per phase if
   it can print it, else `perf` if available).
4. Where a sub-graph builds, run it on wasmtime 49.0.1 against node with a tiny driver script and record the first
   run-time failures.
5. Write the results: update `docs/zigbee2mqtt-feasibility.md` (a new dated section; keep the old one), add follow-ups
   (next free `F<n>`), and propose the next tasks in order (likely: `crypto` subset; `url`/`querystring`; `net.Socket`
   over `wasi:sockets` in the event loop; `child_process`/`dgram`/`zlib` stubs; ajv standalone patch; compiler speed).
   Write the next task's handoff file.

## Read first
- `docs/zigbee2mqtt-feasibility.md`, `tasks/README.md` (Follow-ups: F182, F191, F238, F253, F255, F259).
- `tasks/T54-commonjs.md` (computed `require`, `--include`), `tasks/T61-weakmap-abort.md` (Notes / Result).

## Tests
No new fixtures required unless something is fixed (then a fixture for it); `cargo test` if code changes. Keep the
probe scripts in the scratchpad and record exact commands in Progress so the next session can rerun them.

## Acceptance
- [x] Every build/run failure for the whole graph recorded, grouped and counted.
- [x] Scale numbers for the whole graph and for sub-graphs.
- [x] Feasibility doc updated; follow-ups added; next task handoff written.

## Notes / Result
**Result.** The full numbers, tables and the ordered plan are in
[docs/zigbee2mqtt-feasibility.md](../docs/zigbee2mqtt-feasibility.md) ("Survey 2026-10-01"). In short: with the
missing pieces stubbed or patched in a copy of the package (`tasks/T62-files/patch.sh`), zigbee2mqtt 2.14.1 builds as
one command, validates, and runs on wasmtime 49.0.1 to node's own stopping point with this config: `zh:zstack:znp:
Opening TCP socket with 127.0.0.1:6638` (node: `ECONNREFUSED`; js2wasi3: `net.Socket` missing). Unpatched: 42 link
errors, then 150 lowering errors (~40 of them cascades), then 10 run-time failures in order; all grouped and counted in
the doc. Scale: 73 s / 32 MB without the device modules, 186 s / 1.5 GB / 59 MB with all 390; `wasmtime compile` 20 s
/ 1.7 GB and 102 s / 8.8 GB. Lowering is 96% of the build (11–12 inference rounds over the whole program).

**What changed in the compiler** (tiny fixes that were in the way of seeing further):
- `frontend/src/link.rs`: a module that fails to parse or rewrite (e.g. `new Function` of unknown code) is reported
  with the rest of the graph and replaced by an empty CommonJS stand-in: one build lists every link error (was: the
  first one only).
- `frontend/src/link.rs`: `require(".")`/`require("..")` (and `x/.`, `x/..`) are relative and directories, as in
  node (was: "Invalid module "." is not a valid package name"; `@serialport/bindings-cpp`'s `win32.js`).
- `lower/src/lib.rs`: a function with a rest parameter and `arguments` is reported unsupported; its uses as a value
  (methods) indexed a missing `nested` entry and panicked, which hid every diagnostic of the build (winston).
- Tests: `crates/js2wasi3/tests/build.rs` `errors_of_every_module_are_reported`,
  `rest_parameter_with_arguments_is_a_diagnostic` (fixtures `tests/fixtures/unsupported/report-all/`); `diff.sh`
  fixture `commonjs/resolve` (`lib/dot.js`).

**Scripts** (`tasks/T62-files/`, rerun from a scratch dir `$S`: copy the directory to `$S/t62`; node 24.21.0, npm,
wasmtime 49.0.1 on `PATH`; `JS2WASI3=<repo>/target/release/js2wasi3`):
- `t62/setup.sh`: `npm pack zigbee2mqtt@2.14.1`, `npm install --omit=dev --ignore-scripts` into `$S/zp/package`;
  generates `t62/stubs/<builtin>.js` (`gen-stubs.mjs`: node 24's export names, functions throw "T62 stub: …";
  `child_process.exec` reports ENOENT, `url.urlToHttpOptions` works) and `t62/ajv/*.js` (`ajv-standalone.cjs`).
- `t62/patch.sh` (`T62_LEVEL=2` for everything): copies to `$S/zp/patched` and applies each stand-in, one commented
  block per failure (built-ins → stubs; web globals bound from `stubs/web.js` on line 1 of the files that name them,
  per `errors-p1.txt`; `getOwnPropertyDescriptors` inline; tsc `__decorate`; winston; `delete a?.[b]`; the babel
  regenerator; native addons; ajv standalone; srvx; the TypedArray `toStringTag` getter; `error.stack` in one log).
- `t62/build-run.sh`: patch, build `main.js` (the driver), `wasmtime compile`, run with `t62/data/configuration.yaml`;
  prints build/compile/run time and peak RSS (`measure.sh`: `/proc` sampling; no `/usr/bin/time` here).
  `T62_INCLUDE='node_modules/zigbee-herdsman-converters/dist/devices/*.js'` adds the device modules.
- `t62/each-pkg.sh` (each package alone vs node), `t62/scale.sh` (converters with N device modules),
  `t62/trace.sh` (a copy with `T62 load/done <file>` on stderr around every module: how the load-time failures were
  located, since uncaught errors carry no location).
- Phase times came from temporary `eprintln!`s (not committed): parse+link, each lowering round, backend, packaging.

**Unsupported / found** (follow-ups F261–F274; F255 measured): see the Follow-ups table. Not new: F182 (`Intl` time
zone, herdsman), F191 (ajv `new Function`: worked around with ajv's standalone mode, which needs `requiresRestart` as
a code keyword), F152 (`Proxy`, srvx).

**Next smallest step:** T63 ([T63-small-gaps.md](T63-small-gaps.md)): the small language and built-in gaps that
cost the most errors and the first run-time failures (`error.stack`, unknown globals behind `typeof`, missing statics
as `undefined`, `Object.getOwnPropertyDescriptors`, the TypedArray `toStringTag` getter, rest parameter +
`arguments`, `delete a?.[b]`, `var` before its declaration in a loop, `console._stdout`, error locations).

## Checklist
- [x] setup + node baseline
- [x] whole-graph build, errors grouped
- [x] sub-graph scale curve, run-time probes
- [x] doc, follow-ups, next handoff

## Progress
- 2026-10-01: handoff written after T61 (`main` at `031ea34`).
- 2026-10-01: started. Tools installed in `~/tools` (wasmtime 49.0.1, wasm-tools 1.259.0, node 24.21.0 from their
  release tarballs; wkg/wit-bindgen/wac/wasm-opt not needed here). zigbee2mqtt 2.14.1 is still the latest release
  (master `3c3d8c1`); used the npm tarball (`npm pack zigbee2mqtt@2.14.1`: has `dist/`), `npm install --omit=dev
  --ignore-scripts` (99 packages, 80 MB). Probe scripts in the scratchpad `t62/` (copied into Notes at the end).
- 2026-10-01: node baseline: an empty data dir starts the onboarding web server (port 8080); with a
  `configuration.yaml` (`ZIGBEE2MQTT_DATA`, not `Z2M_DATA`; frontend off, `serial.port: tcp://127.0.0.1:6638`,
  `adapter: zstack`) and `Z2M_ONBOARD_NO_SERVER=1` it migrates v4→v5, opens the TCP socket, gets ECONNREFUSED and
  exits 1. That's the target path. `index.js` runs git via `child_process` and loads `dist/` with `import()` (so
  build errors there become run-time rejections): the survey builds a driver `main.js` (index.js's `start()` with
  static `require`s).
- 2026-10-01: whole graph, link phase: stopped at the first parse-time error (ajv's `new Function`). Fix (tiny):
  a module that fails to parse/rewrite is reported with the rest and replaced by an empty CommonJS stand-in, so one
  run lists every link error: 42 (built-ins 37, `new Function` 1, unknown `import()` 2).
- 2026-10-01: with the missing built-ins stubbed (`t62/gen-stubs.mjs`: node 24's keys, functions throw), the
  compiler panicked in lowering (`nested[&scope]`): a function with a rest parameter and `arguments` is reported
  unsupported but its later uses indexed the missing entry (winston's `Logger.log`). Fix (tiny): those uses return
  `Unsupported`. Then 150 lowering errors (71 s), grouped in Notes.
- 2026-10-01: level-2 patches (web globals from a stub module, `getOwnPropertyDescriptors` inline, tsc
  `__decorate`, winston, a `delete a?.[b]`, babel regenerator's `var`): the whole graph builds: 73 s, 721 MB peak,
  32 MB `.wasm`; `wasmtime compile` 19–21 s, 1.7 GB peak, 132 MB `.cwasm`.
- 2026-10-01: run time: native addons (`@serialport/bindings-cpp`, `unix-dgram`) stubbed; `require(".")` failed
  (js2wasi3 took `.` as a package name): fixed (tiny; `.`/`..` relative and directories, as node; fixture
  `commonjs/resolve`). ajv: the four settings validators generated with ajv's standalone mode
  (`t62/ajv-standalone.cjs`; `requiresRestart` as a code keyword; same errors as node on 3 inputs). Next failure:
  `TypeError: Cannot read properties of undefined (reading 'prototype')` at load, no location: running each
  package alone to find it.
- 2026-10-01: each package alone (`each-pkg.sh`, level 2): 90 of 106 equal node; winston/safe-stable-stringify:
  `%TypedArray%.prototype[Symbol.toStringTag]` getter missing; srvx: Fetch globals at load. With `trace.sh` the
  whole-program failure was srvx (via `dist/util/staticFileServer.js`): stubbed. Then `child_process.exec` (git hash:
  stub reports ENOENT), `error.stack` traps (the built-in sentinel), `URL` (minimal stand-in), `url.urlToHttpOptions`:
  the run reaches `Opening TCP socket with 127.0.0.1:6638` and fails on `net.Socket`, node's ECONNREFUSED point.
- 2026-10-01: device modules: the converters' `Promise.resolve(…).then(s => require(s))` over a nested template isn't
  followed by the build (no error; the 32 MB build has none of the 390 device modules). With `--include`:
  `scale.sh` (50/100/200 modules: 56/69/89 s, 25/28/35 MB; ~12–13 s and ~3.9 MB per MB of JS) and the whole program
  with all 390: 186 s, 1.5 GB, 59 MB; `wasmtime compile` 102 s, 8.8 GB, same run-time result.
- 2026-10-01: compiler phases (temporary timers): lowering 70.9 s of 73 s (11 rounds of ~6 s, 31.5k functions);
  with devices 178 s of 186 s (12 rounds of ~14.5 s, 62.6k functions); parse+link 1.6/3.6 s, backend 0.8/1.7 s,
  packaging 0.3/0.5 s. node runs `main.js` to the socket in 0.6 s; wasmtime (precompiled) 0.6–1 s.
- 2026-10-01: scripts copied to `tasks/T62-files/` and rerun from a fresh directory (`setup.sh` + `build-run.sh`):
  same result. Docs, follow-ups F261–F274, T63 handoff.
- 2026-10-01: checks: `cargo test -p js2wasi3-frontend -p js2wasi3-lower -p js2wasi3`: all pass except the 4
  `*_compose_with_wac` tests (`wac` isn't installed in this container: "wac is installed (TOOLS.md)");
  `scripts/diff.sh tests/fixtures/commonjs`: 14 passed; `hello.js` builds, validates and runs on wasmtime 49.0.1.
