# T62: zigbee2mqtt build survey: what fails now, and the plan to the milestone

Status: todo (handoff written 2026-10-01, not started) | Depends on: T54–T61

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
- [ ] Every build/run failure for the whole graph recorded, grouped and counted.
- [ ] Scale numbers for the whole graph and for sub-graphs.
- [ ] Feasibility doc updated; follow-ups added; next task handoff written.

## Notes / Result
(to be written)

## Checklist
- [ ] setup + node baseline
- [ ] whole-graph build, errors grouped
- [ ] sub-graph scale curve, run-time probes
- [ ] doc, follow-ups, next handoff

## Progress
- 2026-10-01: handoff written after T61 (`main` at `031ea34`).
