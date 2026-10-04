# T80: zigbee2mqtt speed on the T79 replay

Status: todo (handoff written 2026-10-04) | Depends on: T79 (the recording and the replay tools)

## Goal
Make js2wasi3's zigbee2mqtt cheaper in CPU **and** memory on the real workload, every step measured on T79's replay
of the recorded production session (never a proxy benchmark: memory "validate speed targets"). One change at a time,
before/after on the replay, against node on the same replay.

## Read first
- `tasks/T79-zigbee-profile.md` (Notes / Result) and the first section of `docs/zigbee2mqtt-feasibility.md`.
- `tasks/README.md` F304, F347–F352 (and F321, F323, F334, F339 for step 5).
- Memory: a-03 machine (sync, paths, perf, `pkill -f` pitfall), no wasmtime upstream reports.

## Baseline (T79, a-03, the 903 s after zigbee2mqtt's start, 548 device frames)
| | CPU | vs node | RSS |
|---|---|---|---|
| node 24.21.0 | 8.66 s | 1× | 289 MB |
| js2wasi3, 256 MiB GC heap (old default) | 143–145 s | 16.5× | 431 MB |
| js2wasi3, 512 MiB (now the default, F347) | 39.1 s | 4.5× | 765 MB |
| js2wasi3, 1 GiB | 35.9 s | 4.1× | 1.3 GB |

Profile at 1 GiB (perf, inclusive): UTF-8 encoding in `node:buffer` JS ~36% (`Buffer.byteLength` 20.6%,
`utf8Write` 15.1%, both via `utf8ToBytes`), RegExp built-ins ~9%, wasmtime libcalls ~7%, property-access helpers
~27% self, `box_number` 3%, GC ~3%.

## Why this order (user discussion 2026-10-04)
wasmtime's collector is a textbook Cheney semi-space copier (docs 49.0.2; Bytecode Alliance article July 2026: no
generational GC planned) whose grow-vs-collect rule grows the heap only once the live data fills a semi-space (F304).
We don't report upstream (user: wasmtime doesn't accept work with/from AI); we work around it (F347's run flag, done)
and fix what is ours: how much the compiled program allocates and how big its live set is. A program-side heap
growth trick is possible but **not planned** (user: it hides the problem, gains us nothing).

| step | what | effort | expected (replay) |
|---|---|---|---|
| 0 | fresh **baseline at 512 MiB** with today's build (also checks F347's untested script changes) | ~20 min on a-03 | ~39 s CPU, 765 MB |
| 1 | F347 — done 2026-10-04 (512 MiB in README and the zigbee2mqtt run scripts, untested) | done | |
| 2 | **F348: native UTF-8 encoding** in `node:buffer` | small | ~36% of the CPU → ~2.9× node |
| 3 | F349: RegExp fast paths for unmodified RegExp objects | small–medium | ~9% → ~2.6× with step 2 |
| 4 | re-measure and re-profile; then F350 (wasmtime libcalls `is_subtype`, `get_interned_func_ref`) | medium | ~7% |
| 5 | decide with fresh numbers: F323/F334 unboxed numbers, F321 shapes, F352 one-byte strings | large each | property access ~27% self; live set/RSS |

WasmGC limits that make step 5 larger (workable): struct layouts fixed at compile time (shapes as arrays of slots),
no interior pointers (shared substrings need our own slice type), i31 is 31 bits.

## Step 2 pointers (F348)
- `crates/frontend/node/buffer.js`: `byteLength` (line ~388), `utf8Write` (~829), `utf8ToBytes` (~1906; builds a JS
  array with one element per byte), `utf8Slice` (~927, decoding: F339, may go in the same step).
- T78 did the same for Latin-1: `byteSlice` calls `__intrinsic_typed_latin1(buf, start, end, mask)` →
  `TypedOp::Latin1` (`crates/lower/src/lib.rs` ~8554 maps the intrinsic name; `crates/ir/src/lib.rs` ~1550 types,
  ~1770 names; `crates/backend/src/typed.rs` ~777 emits it). Add native ops for: the UTF-8 length of a string
  (unpaired surrogates count as U+FFFD, 3 bytes), encoding a string range into a Uint8Array at an offset with a byte
  limit (node's partial-write rules: never a partial character), and decoding (F339, replacement characters as
  node).
- Tests: `tests/fixtures/node/{buffer,buffer-edges,string-decoder}.js`, `tests/fixtures/builtins/text-coding.js` (diff.sh);
  add a fixture for surrogates, partial writes at the limit, 4-byte characters.

## How to measure (on a-03; T79's tools)
1. Sync the repo (memory "a-03": rsync, never push to get code there), then on a-03
   `~/js2wasi3-prod-run/build-p.sh` (release js2wasi3, the zigbee2mqtt build with all device modules into
   `p.wasm`, `wasmtime compile` with 512 MiB into `p.cwasm`: ~5 min, 15 GB). `t62/` must exist there first (it does).
2. `~/js2wasi3/tasks/T79-files/run-replay.sh ~/js2wasi3-prod-run/rec-20261004-1941 wasm t79/out-<name>` (and `node`
   once for the day's node number) → `result.txt` (`cpu_steady_s`), RSS in `cpu.csv`, first minute from `cpu.csv`.
   Run it under `nohup` (15 min); parallel runs need their own `MQTT_PORT`/`ZIGBEE_PORT`/`FRONTEND_PORT`, and a
   `.cwasm` compiled with the same `WASM_FLAGS`. Check the publish count in `z2m.log` (js2wasi3 13,476, node 13,567:
   the difference is F317's external converters).
3. Profile: `PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf` on the run (attached for the window after start),
   then `perf report -i perf.data --children --sort sym -g none --stdio` (inclusive per JS function) and `--no-children
   --sort dso,sym` (self).
4. Allocation (optional, user questioned its value): the same build with `-C collector=null` and a big heap measures
   total bytes allocated (its RSS growth; T79: ~3.4 GB / 15 min). Better, undecided: allocation counters per kind in
   js2wasi3 (a build option) and the live set (the smallest heap at which collections stop dominating).
Report per step: CPU (whole window and first minute), RSS, publishes, node's numbers.

## Acceptance
- [ ] Step 0 baseline; steps 2–3 done, each with replay CPU, RSS, publishes before/after (and node's).
- [ ] Step 4 measured; step 5 decided with numbers (which large item first, or none), recorded as follow-ups/tasks.
- [ ] `docs/zigbee2mqtt-feasibility.md` updated; fixtures of the touched built-ins pass `scripts/diff.sh`.

## Notes / Result
(to be written)

## Progress
- 2026-10-04: handoff written after T79 (user: most of the gap is ours to fix; record the table as tasks).
- 2026-10-04: F347 done without testing (user): 512 MiB (`536870912`) in README, `tasks/T68-files/{build,restart}-run.sh`,
  `tasks/T71-files/build-run.sh`, `tasks/T79-files/run-replay.sh` (default `WASM_FLAGS`), and outside the repo
  `~/js2wasi3-prod-run/{run,run-prod,run-prod3,run-test,run-test78,run-test79,dissect}.sh` and a-03's `build-p.sh`.
  Existing `.cwasm` files (p79.cwasm, a-03's p.cwasm) were compiled with 256 MiB: recompile before running them with
  the new flags (wasmtime refuses a mismatch). The T74–T76 micro-benchmark scripts and `scripts/bench.sh -g` keep
  256 MiB (they reproduce past numbers). Program-side heap growth: possible, not planned (user).
- 2026-10-04: correction to T79's wording: `decodeChunk` (9.7%) is readable-stream's writable turning string chunks
  into Buffers, i.e. a caller of the same UTF-8 *encoding* path, already inside the ~36%. Next session: step 0.
