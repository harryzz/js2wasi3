# T78: run-time benchmarks against node (Benchmarks Game, Octane 2)

Status: todo (handoff written 2026-10-04) | Depends on: none (independent of T77)

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
