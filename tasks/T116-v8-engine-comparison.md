# T116: the same components on node's V8 (jco) vs wasmtime: engine and GC comparison

Status: done | Depends on: T111 | Local (+ a-03 only after asking)

## Goal
Measure how much of the gap to node is wasmtime's engine (its copying GC, root R5; `is_subtype` casts, R1;
Cranelift) and how much is our generated code: run the same `.wasm` on wasmtime 49.0.2 and on node 24.21.0's V8
(through jco's WASI 0.3 shim), on the standard benchmarks only. A measurement: wasmtime stays the only test host.

## Research (2026-10-13)
- Wasmer (latest v7.5.0) has no component model / WASI 0.2–0.3: not usable. Its V8 backend would add nothing over
  node's V8.
- jco: `@bytecodealliance/preview3-shim` (WASI 0.3.0; ours is 0.3.1) runs command components on node; core
  modules are only rewritten for multi-memory (ours pass through with GC); exceptions need
  `--bindgen-enable-wasm-exnref` (node 24.21: `--experimental-wasm-exnref` on by default). jco's `run` is P2;
  P3 commands are run by a script like jco's `packages/jco-transpile/test/p3/cli-runner.js`.

## Plan
- [x] Install jco + preview3-shim (user: yes, 2026-10-13) outside the repo; record in `TOOLS.md`.
- [x] One benchmark: transpile, run on node, output = node's JS run.
- [x] (Ask) all 18: wasmtime vs V8 times, GC/cast profiles; results in `docs/performance.md`.

## Notes / Result
- **Tools**: `scripts/v8run.mjs` (runner: preview3-shim host, cwd preopened, startup/run split), `scripts/v8bench.sh`
  (bench.sh's 18 programs, sizes, sources, fasta inputs; node JS, wasmtime, V8 alternating; `out/v8bench/`).
  jco needs `--bindgen-enable-wasm-exnref` and explicit `--map …@0.3.1` (it knows WASI 0.3.0 only); each jco
  output dir gets `{"type":"module"}`.
- **Result** (laptop, `out/v8bench-all.md` / `.tsv`, build `7afb3a3`): all 18 outputs = node on both engines.
  wasmtime / V8 run: geomean **1.52×** (binarytrees 2.92×, splay 2.65×, earley-boyer 2.33×, pidigits 2.12×,
  nbody 1.96×, regexp 1.94×, deltablue 1.85×, raytrace 1.84×; mandelbrot 0.62×, revcomp 0.79×, fannkuch 0.95×,
  fasta 1.01×). V8 run / node JS: geomean **6.8×** (wasmtime / node 10.3×).
- **GC** (binarytrees, splay, earley-boyer; nbody as control): wasmtime's copying collector 2.3–4.4 s vs V8's
  main-thread pauses 0.55–0.61 s: **4–8×**. Without GC wasmtime is still 1.3–2.0× slower (nbody 2.0× with no GC).
- **Meaning**: about a third of our time on wasmtime is the engine (GC above all, then code generation / casts);
  the larger part is our code (6.8× node even on V8). Written in `docs/performance.md` ("Same components on node's
  V8"); jco in `TOOLS.md`.
- **Not done**: where V8 gains on non-GC code (nbody/earley-boyer 2×): a V8 profile against wasmtime's (F426).
  a-03 not used (the user: where it runs doesn't matter for an engine comparison).

## Progress
- 2026-10-13: created (user: "install and try one benchmark").
- 2026-10-13: installed jco 1.37.0, preview3-shim 0.9.0, preview2-shim 0.28.0 in `~/.local/jco` (own package.json; a first `npm install` without one went to `~/` via an old `~/node_modules`: removed again, home restored).
- 2026-10-13: jco 1.37 knows only WASI `0.3.0` (`SUPPORTED_P3_VERSIONS`); our `@0.3.1` imports need explicit
  `--map wasi:<pkg>/*@0.3.1=@bytecodealliance/preview3-shim/<pkg>#*` (else the 0.2 shim). Runner: a copy of
  jco's P3 test runner (args, terminals, `$init`, `wasi:cli/run@0.3.1`.`run()`), in the session scratchpad.
  `jco transpile --bindgen-enable-wasm-exnref`: no errors; the core module runs unchanged on node 24.21's V8.
- 2026-10-13: nbody (patched, 500,000 steps, `7afb3a3` build, laptop, wall time, 3 runs): outputs of V8 and
  wasmtime = node's. node JS 113–164 ms; wasmtime 2,036–2,694 ms; V8 (jco) 1,275–1,476 ms. Startup (N=1): node
  JS 33–58, wasmtime 15–19 (precompiled), V8 208–311 ms (jco's JS glue + compiling the 197 KiB module).
  So on V8 our module runs ~1.6× faster than on wasmtime (net of startup ~1.0–1.25 s vs ~2.0 s), still ~9× node's JS.
- 2026-10-13: `scripts/v8run.mjs` (runner, preopens the cwd, startup/run split) and `scripts/v8bench.sh` (bench.sh's 18 programs and inputs; node JS, wasmtime, V8 alternating). Trial `-r 1`: nbody wasmtime 2,709 / V8 run 1,430 ms (1.89×), knucleotide 7,216 / 4,232 (1.71×), richards 3,601 / 2,360 (1.53×); outputs = node. Full run `-r 3` (`out/v8bench-all.md`).
- 2026-10-13: all 18 (`-r 3`): geomean wasmtime/V8 run 1.52×, V8 run/node 6.8×; GC: `--trace-gc` pauses vs wasmtime `perf` collector share (4–8×). Docs written. Done.
