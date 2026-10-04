# T78: run-time benchmarks against node (Benchmarks Game, Octane 2)

Status: in progress (started 2026-10-04) | Depends on: none (independent of T77)

## Goal
Standard, third-party run-time benchmarks to measure js2wasi3 against node, chosen by others (not by us, unlike
T74's zigbee-herdsman-converters benchmark, which turned out unrepresentative: F329). One run of the finished
program, **compile time excluded** (js2wasi3: a precompiled `.cwasm` on wasmtime 49.0.2; node 24.21.0). Output
checked against node's; a program js2wasi3 can't build or run is reported (with the reason, as a follow-up), not
silently dropped. The user's question that started it: is there a regex benchmark for node to compare with.
Comparisons with other compilers (scriptc: vercel-labs/scriptc, embeds quickjs-ng's libregexp) are a later step
and need the user's OK to install them.

## The suites
1. **The Computer Language Benchmarks Game** (benchmarksgame-team, BSD-3; programs:
   https://benchmarksgame-team.pages.debian.net/benchmarksgame/ , sources in the `benchmarksgame` repository on
   salsa.debian.org, `bencher/programs/<name>/*.node`). Take a **single-threaded** node program per benchmark
   (some node versions use `worker_threads`, which js2wasi3 lacks: F238); record which program number.

   | program | what it tests | official input |
   |---|---|---|
   | **regex-redux** | regexes: 9 alternation counts, then replacements, over fasta output | stdin: fasta 5,000,000 |
   | k-nucleotide | strings, `Map` | stdin: fasta 25,000,000 |
   | reverse-complement | strings/bytes, output | stdin: fasta 25,000,000 |
   | fasta | string building, output | 25,000,000 |
   | binary-trees | allocation, GC | 21 |
   | fannkuch-redux | integer arrays, permutations | 12 |
   | n-body | floating point, objects | 50,000,000 |
   | spectral-norm | floating point, arrays | 5,500 |
   | mandelbrot | floating point, bit output | 16,000 |
   | pidigits | BigInt | 10,000 |

   Inputs: start smaller where a run would take minutes on this laptop; keep the official size too where feasible;
   record both. stdin programs: check js2wasi3 reads all of stdin (`fs.readFileSync(0)` / `process.stdin`) and
   writes large stdout; missing pieces are findings.
2. **Octane 2** (V8's suite, BSD; https://github.com/chromium/octane): **RegExp** (regexes and strings recorded
   from real websites), Richards, DeltaBlue, RayTrace, NavierStokes, Crypto, Splay (GC), EarleyBoyer. Skip
   Mandreel, zlib, TypeScript, Box2D, PdfJS, CodeLoad, Gameboy unless they work (huge or `eval`/`new Function` at
   run time, F191) and say why. Octane's shell harness uses `load()`/`print`: make one file per benchmark (`base.js`
   + the benchmark + a small driver calling `BenchmarkSuite.RunSuites` with `print` → `console.log`), the same file
   for node and js2wasi3. Octane reports a score (higher is better) and also check run time.

## How to measure
- Laptop noise is ~30% run to run (T74/T75): run each program 5× alternating node / js2wasi3, report the minimum
  and median; machine idle (no `diff.sh`, test262 or builds at the same time). `a-03` could give quieter numbers
  (ask the user first; `reference-a03` memory).
- js2wasi3: `build` + `wasmtime compile` once (not timed, but record size), run
  `wasmtime run -Sp3 --allow-precompiled <x>.cwasm` (try also `-O gc-heap-initial-size=268435456`, T69: the copying
  GC can thrash with a small heap; report both if they differ). Wall time from outside (`/usr/bin/time` isn't
  installed on the laptop: use `date +%s%N` or a small node runner; peak RSS from `/proc` sampling as in
  `~/js2wasi3-prod-run/sample.sh` if wanted).
- Report: a table per suite (time node, time js2wasi3, ratio; Octane scores), in a new `docs/benchmarks.md`, with
  versions, machine, inputs and commands; a script `scripts/bench.sh` (or `tests/bench/run.sh`) that reproduces it.
- For the slowest ratios: a `perf` profile (`--profile=perfmap`, T69 setup; `tasks/T74-files/perf-cats.py`), the top
  items as follow-ups (not fixed in this task).

## Plan
1. Fetch the programs into `tests/bench/` with their licence texts and source URLs (check the licences first;
   BSD-3 / BSD both allow copying with the notice), plus input generators (fasta → `regex-redux`, `k-nucleotide`,
   `reverse-complement` inputs; generated at run time, not committed if large).
2. Runner script; correctness check against node's output (normalize nothing: equal bytes, else a finding).
3. regex-redux and Octane RegExp first (the user's question), then the rest.
4. `docs/benchmarks.md`; follow-ups for unsupported programs and the biggest slowdowns (with profiles); README /
   `tasks/README.md` notes.

## Acceptance
- [ ] All 10 Benchmarks Game programs and the 8 Octane benchmarks above run under node and js2wasi3 (or are listed
      with the reason they don't), outputs equal node's.
- [ ] `docs/benchmarks.md` with the tables, versions, inputs, commands; a script that reproduces it.
- [ ] Follow-ups for each unsupported feature found and each large slowdown (with its profile's top items).

## Notes / Result
(to be written)

## Progress
- 2026-10-04: handoff written. Context: T74–T76 sped up a self-made benchmark but not zigbee2mqtt's real run
  (`docs/zigbee2mqtt-feasibility.md`, "Production runs 2026-10-04"); the user asked for standard benchmarks with
  node versions, regexes first; scriptc (vercel-labs) publishes no benchmark results (checked 2026-10-04: its
  `scripts/bench-builds.mjs` measures build latency of two small TypeScript apps).
- 2026-10-04: started. Sources: Benchmarks Game repo (salsa, commit 40296663, 2025-03-07) `public/download/benchmarksgame-sourcecode.zip`
  (the `bencher/programs` dir only has 4 non-node programs); Octane github chromium/octane commit 570ad1cc (2017-04-12).
  Picked single-threaded node programs: binarytrees #7, fannkuchredux #8, fasta #8, knucleotide #8, mandelbrot #8,
  nbody #8, pidigits #3, regexredux #4, revcomp #2, spectralnorm #8 → `tests/bench/benchmarksgame/<name>.js` (+ LICENSE.md;
  `tests/bench/package.json` `"type":"commonjs"`: several are sloppy code). Octane: base.js + 8 benchmarks + LICENSE in
  `tests/bench/octane/`, `driver.js` (modes `score` / `fixed` = deterministicIterations, no warm-up).
- 2026-10-04: first builds (release): 15/18 build. Build failures: knucleotide (`node:readline`, F238), Octane EarleyBoyer
  (`"\000"` octal escapes, sloppy only, F232), Octane RayTrace (`Object.extend = …`: new property on a built-in
  constructor). Small-input runs vs node: same output for binarytrees, fasta, pidigits; ReferenceError for implicit
  globals (sloppy assignment) in fannkuchredux (`f`), nbody (`py = pz = 0`), spectralnorm (`i = j = 0`) (F232);
  mandelbrot: `process.stdout.write(Uint8Array)` rejected (only Buffer/string); regexredux: `readFileSync('/dev/stdin')`
  ENOENT; revcomp: no `process.stdin` (F239); `readFileSync(0)` EBADF. Plan: report these, plus run minimally patched
  copies (marked) so every program still gets a time.
- 2026-10-04: no stdin at all (F239: `process.stdin` undefined, fd 0 EBADF), so the stdin programs get patched copies
  in `tests/bench/patched/` that read `input.fa` from a preopened dir (regexredux, knucleotide; revcomp: a tiny
  `stdin.read(n)`/`on('readable')` object over the file). Patched for strict mode: fannkuchredux, nbody,
  spectralnorm, knucleotide (declarations only); mandelbrot: `write(Buffer.from(u8.buffer))`.
- 2026-10-04: Octane needs more: base.js assigns `Math.random = …` (unsupported: assigning a property of a built-in,
  build error at top level, run-time "global `Math` isn't available" inside a function; also `Object.extend = …` in
  RayTrace) and implicit globals (`alert`, `setupEngine`, crypto's `nValue`…, earley-boyer's `result`), NavierStokes
  uses sloppy `this` (=global) in `checkResult`, EarleyBoyer has `eval` in the unused `sc_jsNew` (F191) and octal
  escapes. Fix: `octane/prelude.js` (declarations, `__random = Math.random`) + `s/Math\.random/__random/g` on the
  concatenation, and patched copies of raytrace, navier-stokes, earley-boyer. Found the implicit globals by running
  node on the file with `"use strict"` prepended until no ReferenceError.
- 2026-10-04: first Octane `fixed` runs (deterministicIterations, no warm-up), outputs equal: Richards node 0.98 s vs
  149 s (152×), DeltaBlue 0.64 s vs 134 s (209×), RayTrace 1.36 s vs 301 s (221×). Too long for 5 runs: the runner
  needs an iteration scale (`score` mode runs ~1 s per benchmark instead).
- 2026-10-04: `scripts/bench.sh` written (programs table with small/official sizes, Octane `fixed 0.05` by default,
  patched copies marked `*`, outputs compared on the first run, min/median of `-r` runs, `results.tsv`). Laptop
  calibration (1 run): binarytrees 16 0.33 s vs 13.6 s (42×), fannkuchredux* 10 0.45 vs 9.5 s (21×), fasta 250k
  0.29 vs 1.9 s (6.5×), all equal output. User OK'd the 5-run pass on a-03: synced (rsync, release build there),
  1-run calibration there first (a-03: binarytrees 20×, fannkuchredux* 22×, fasta 6.6×).
- 2026-10-04: a-03 calibration so far: knucleotide* fasta 250k > 300 s timeout (node 1.3 s), nbody* 500k 55 s vs
  0.15 s (363×), pidigits 2000 6.9×. mandelbrot* failed: `process.stdout.write` takes strings only (Buffer and
  Uint8Array both rejected: `crates/lower/builtins/process.js` `$$processWriter`), `fs.writeSync(1, …)` EBADF; the
  patched copy now prints the header + a checksum of the bitmap (same compute loop).
- 2026-10-04: root-cause probe (laptop, user asked): nbody-like loop (`b.x += b.vx * dt` over 2 objects, 6M updates):
  node 45 ms, js2wasi3 1.89 s. IR: every `b.x` is `$get(obj, "x")` / `$set`, every float result `f64->jsval` (boxed).
  perf: 39% wasmtime GC fixed costs (trace_vmctx_roots 13.5%, type_registry root 12.3%, RefType::from_wasm_type,
  RegisteredType drop, copying forward), get_named 9.9%, box_number 4.4%, to_number 3.4%. With
  `-O gc-heap-initial-size=256MiB`: 0.91 s (2.1× faster); then get_named 17.6%, box_number 7.9%, to_number 5.8%,
  get 4.3%, set_named 4.2%, find_own_named 3.9%, ordinary_set_named 3.1%, get_length 2.8%, js_add 2.5%.
  A plain f64 loop (typed locals) is fast. So: (1) very frequent collections with large per-collection fixed cost,
  fed by (2) boxing every non-i31 number, and (3) property access as runtime hash lookups (no shapes/inline caches).
- 2026-10-04: a-03 calibration done (1 run): revcomp* 68×, spectralnorm* 44×, Octane fixed 0.05: Richards 59×,
  DeltaBlue 38×, RayTrace* 118×, NavierStokes* 33×, Crypto 28×, Splay 14×, RegExp 74×; EarleyBoyer*: wrong result
  under js2wasi3 (`TypeError: Cannot read properties of undefined (reading 'cdr')`, node: done) → to narrow down;
  knucleotide* (fasta 250k) and regexredux* (fasta 500k) > 300 s.
- 2026-10-04: regexredux root cause (laptop): superlinear (fasta 5k 7.9 s, 20k 60 s; node 40-60 ms). 1 GiB GC heap:
  1.65 s / 7.8 s, still superlinear; then perf: memset 53% + memmove 26%, 81.5% of samples inside
  `readFileSync(…, 'ascii')` → Buffer `asciiSlice` (crates/frontend/node/buffer.js:1014: `ret += String.fromCharCode(…)`
  per byte) → `concat`, which copies the whole flat string each time: O(n²) string building with `+=` (node: ropes).
  Diagnostic: the same program reading `'utf8'`: 20k 0.92 s, 100k 5.6 s (linear; was 62.7 s at 20k), GC heap size
  no longer matters there. So two separate problems: quadratic `+=` string building (concat copies; latin1Slice etc.
  have the same loop) and the GC fixed costs/boxing/property lookups from the nbody probe.
- 2026-10-04: user code too: `let s = ""; for (…) s += "a"` (sc.js): 25k 0.65 s, 50k 1.56 s, 100k 3.74 s, 200k 10.3 s
  (node 200k: 66 ms incl. startup). `+=` on strings is O(n²) in general (flat strings, concat copies both sides).
- 2026-10-04: user: fix the quadratic `+=` now (under T78), the rest as follow-ups. Change: a string variable
  that is appended to (`x += e`, `x = x + e…`; closures analysis `appended`) and is uncaptured (a local) or
  top-level (a global) holds the string or a `$strbuf` (new runtime type 28; `$bigint` is now 29):
  `str_buf_append` appends in place (buffer doubles), `str_buf_get` makes the string once per run of appends,
  `str_buf_len` gives `.length` without making it (IR intrinsics StrBufGet/StrBufAppend/StrBufLen;
  `Place::Buf`/`BufGlobal` in the lowering; `buf_append` rewrites the store; it refuses pieces that write the
  variable, and for globals pieces that run any JS code). sc.js (`s += "a"` top-level): 200k 10.3 s → 19 ms,
  2M 64 ms. sb.js (function locals, `.length`, `x = x + i + ","`): same output as node, validates.
- 2026-10-04: regexredux* fasta 20k after the fix: 62.7 s → 1.31 s (100k 5.8 s, 500k 28.0 s vs node 0.55 s), same output. This shell sets FORCE_COLOR=3 (node colors console.log into files): bench.sh now runs node with `env -u FORCE_COLOR`. scripts/diff.sh run started and stopped by the user before its summary (user: cache node's results instead of rerunning node every time).
- 2026-10-04: regexredux (patched, fasta 100k; node 0.12 s): after the `+=` fix 4.23 s; profile: the regex VM
  started at every position (alternation patterns have no first instruction to skip on). `$$reFirstSet`
  (`code[10]`, first-unit filter): 2.78 s. `$$reRun` keeps the `code` fields it read last and returns an `f64`
  (an explicit `return -1` after its loop; the implicit `undefined` made the result boxed): 2.14 s. Fixed
  alternations (`$$reAltsProg`, `code[11]`) matched without the VM, in JS: 1.39 s; natively (`re_alts_find`,
  `re_in_ranges` in runtime.rs; `Intrinsic::ReAltsFind`): 0.92 s (possibly measured under load). Buffer
  `latin1`/`ascii` decoding native (`TypedOp::Latin1`, `__intrinsic_typed_latin1`): 0.55-0.60 s (user's 5 runs),
  output equal. Checks: tmp re.js (38 patterns: match/exec/replace, `i`, classes, negated, `^`/`\b`/lookarounds,
  `y`, `u`, overlapping alternatives) and bd.js (ascii/latin1/binary, offsets, ranges) equal node's.
- 2026-10-04: user: node's results shouldn't be recomputed each run: `scripts/diff.sh` caches them in
  `out/node-cache/` (key: node version, fixture and its side files, package.json, a multi-file case's files).
  Full `scripts/diff.sh` (filled the cache, 524 s): 237 passed, 2 failed (node/fetch.js, node/fs-streams.js: the
  10 s timeout under the full parallel load; alone both pass), 19 skipped. `cargo test`: all pass (no IR snapshot
  changed). Follow-ups F331-F339 added. Benchmarks (5-run tables, docs/benchmarks.md) still to do.
- 2026-10-04: zigbee2mqtt on the test rig with the build after 5757a57 (p78.cwasm): stopped after 10 min,
  `process.memoryUsage is not a function` (the Health extension's 10-minute check; F340). Rerun with
  `ZIGBEE2MQTT_CONFIG_HEALTH_INTERVAL=600`: 2 h 49 min, 5.8 s CPU, RSS 461 MB, health `ok` 50-130 ms; the
  coordinator dropped the TCP link around 14:55-15:05 and zigbee2mqtt kept a half-open socket (keepalive at the
  OS default 2 h: `setKeepAlive`'s delay ignored, F341). Stopped by the user. Fixed both: `process.memoryUsage()`
  (+ `.rss()`, `cpuUsage()`) with 0s; `setKeepAlive(enable, initialDelay)` sets `set-keep-alive-idle-time` (ka.js:
  `ss` shows a 15 s keepalive timer). `scripts/diff.sh tests/fixtures/node`: 41/41 pass (5 needed `-j 2`: the
  10 s timeout under load).
- 2026-10-04 evening: production run with the b2e6e8b build (52 devices, docs/zigbee2mqtt-feasibility.md "Production
  run 2026-10-04 evening"): 19.4 ms CPU per event vs 18.4 ms after T76 (node 1.6 ms): T78's fixes don't reach the
  real workload; `Intl` crash at 5:00 (F330); RSS one step 499 → 758 MB in minute 3 (not explained). zwasm v2.7.0
  probed as a second runtime: can't run js2wasi3 output yet (TOOLS.md "Other runtimes").
