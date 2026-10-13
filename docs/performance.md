# Where the time goes: js2wasi3 vs node

Profiles of the 18 benchmark programs (`tests/bench`, `scripts/bench.sh`'s small sizes) under node 24.21.0 and
js2wasi3 + wasmtime 49.0.2, taken 2026-10-10 on a Xeon Gold 6240 server (72 threads, WSL2) with the build at
`e13d22c`. Run times and node ratios are on the [compatibility page](compatibility.md#benchmarks): at the time js2wasi3
was 2.9× (pidigits) to 54× (octane-regexp) node's wall time; after T103–T109 (2026-10-12, `a47ff0b`) 1.3×
(mandelbrot) to 57× (octane-regexp), most Octane programs 12–34×; after T110–T114 (2026-10-13, `d0a44b1`) 1.3×
(mandelbrot) to 31× (octane-raytrace), octane-regexp 10.5×, most Octane programs 11–31×.

**Summary.** The order of costs isn't node's. In node, property access and number arithmetic aren't costs of their
own: its optimizing JIT turns `o.x` into a hidden-class check and a load, and keeps doubles unboxed in registers, all
inside the program's own machine code. In js2wasi3 these are runtime helpers, and they make most of the gap:

1. **property access**: 32 s of the 94 s gap (31% of js2wasi3's time on average; node 1.6%);
2. **regex**: 20 s, mostly octane-regexp (an interpreter here; node compiles each pattern to machine code);
3. **numbers** (boxing every non-small-integer result, unboxing on every use): 11 s (16% on average; node 0);
4. **GC**: 11 s. Its *share* is the same as node's (11%), its seconds 13× more: more allocation (boxed numbers) and
   wasmtime's copying collector, which copies all live data each time (V8's young-generation scavenger copies only
   survivors of the nursery).

The program's own compiled code is only ~4× node's (7.5 s vs 2.0 s).

## Method

- `scripts/profile.sh <benchdir> <outdir>`: `perf record -e cpu-clock -g` (WSL has no PMU), all threads.
  - js2wasi3: `wasmtime run --profile=perfmap` of the `.cwasm` `scripts/bench.sh` built, 999 Hz.
  - node: `--perf-basic-prof --interpreted-frames-native-stack`, 4999 Hz (its runs are ~20× shorter).
  - Outputs of both equal for every program.
- `scripts/profile-compare.py <outdir> [per] [md]`: each sample charged to its leaf function, by category.
  - node: a sample goes to the JIT compiler or startup by the native frames above the first JS or builtin frame.
  - Seconds = samples / rate. CPU, not wall time: node's compiler and GC also run on worker threads (~26% of its
    samples).
- Averages give each program the same weight; seconds are sums over the 18 programs. "JS code" is the program's own
  compiled functions (node: TurboFan/Maglev code, interpreter, unnamed JIT code).
- With inputs this small node spends a quarter to half of its time on startup and compiling, so its steady-state
  lead is bigger than the ratios on the compatibility page.

## Totals

| category | node avg % | js2wasi3 avg % | node CPU s | js2wasi3 CPU s | gap s |
|---|---|---|---|---|---|
| property access | 1.6 | 30.5 | 0.08 | 32.15 | +32.07 |
| regex | 4.7 | 9.1 | 0.38 | 20.45 | +20.07 |
| numbers (box/unbox) | 0.0 | 16.3 | 0.00 | 11.42 | +11.42 |
| GC | 11.1 | 10.7 | 0.92 | 12.20 | +11.28 |
| JS code | 34.7 | 8.9 | 1.98 | 7.50 | +5.51 |
| operators | 0.0 | 7.1 | 0.00 | 4.89 | +4.89 |
| arrays/args | 0.0 | 4.9 | 0.00 | 4.43 | +4.43 |
| calls | 0.9 | 1.5 | 0.06 | 1.92 | +1.87 |
| startup/host | 11.5 | 2.9 | 0.59 | 2.04 | +1.45 |
| other runtime/builtins | 3.4 | 1.9 | 0.53 | 1.87 | +1.34 |
| BigInt | 3.6 | 5.0 | 0.23 | 0.83 | +0.60 |
| strings | 4.2 | 1.2 | 0.61 | 1.00 | +0.39 |
| JIT compiler | 24.3 | 0.0 | 1.32 | 0.00 | -1.32 |
| **total** | | | **6.7** | **100.7** | |

## Per program (% of each program's CPU)

js2wasi3:

| program | CPU s | property access | numbers (box/unbox) | GC | operators | arrays/args | regex | BigInt | JS code | top other |
|---|---|---|---|---|---|---|---|---|---|---|
| binarytrees | 9.21 | 25 | 0 | 57 | 0 | 2 | 0 | 0 | 7 | calls 5 |
| fannkuchredux | 8.98 | 36 | 20 | 0 | 11 | 22 | 0 | 0 | 11 | startup/host 0 |
| fasta | 0.87 | 20 | 19 | 0 | 26 | 0 | 0 | 0 | 22 | startup/host 6 |
| knucleotide | 5.15 | 25 | 6 | 4 | 10 | 4 | 0 | 0 | 6 | other runtime/builtins 28 |
| mandelbrot | 3.89 | 1 | 77 | 1 | 11 | 1 | 0 | 0 | 8 | startup/host 1 |
| nbody | 5.99 | 70 | 19 | 0 | 2 | 0 | 0 | 0 | 6 | other runtime/builtins 1 |
| octane-crypto | 5.83 | 30 | 32 | 0 | 4 | 7 | 0 | 0 | 26 | calls 0 |
| octane-deltablue | 3.69 | 77 | 2 | 1 | 3 | 2 | 0 | 0 | 10 | calls 5 |
| octane-earley-boyer | 10.25 | 39 | 0 | 31 | 5 | 3 | 0 | 0 | 10 | calls 5 |
| octane-navier-stokes | 3.39 | 27 | 42 | 10 | 8 | 5 | 0 | 0 | 6 | startup/host 1 |
| octane-raytrace | 7.34 | 69 | 3 | 4 | 2 | 7 | 0 | 0 | 8 | calls 4 |
| octane-regexp | 22.36 | 9 | 2 | 1 | 2 | 1 | 81 | 0 | 1 | other runtime/builtins 1 |
| octane-richards | 5.02 | 74 | 2 | 0 | 10 | 1 | 0 | 0 | 9 | calls 3 |
| octane-splay | 3.20 | 8 | 1 | 78 | 2 | 0 | 0 | 0 | 2 | startup/host 9 |
| pidigits | 0.93 | 0 | 0 | 1 | 0 | 0 | 0 | 89 | 0 | startup/host 9 |
| regexredux | 2.91 | 5 | 1 | 2 | 1 | 3 | 84 | 0 | 0 | startup/host 3 |
| revcomp | 0.48 | 14 | 26 | 0 | 11 | 24 | 0 | 0 | 17 | startup/host 6 |
| spectralnorm | 1.22 | 21 | 40 | 1 | 19 | 7 | 0 | 0 | 11 | startup/host 1 |

node:

| program | CPU s | property access | numbers (box/unbox) | GC | operators | arrays/args | regex | BigInt | JS code | top other |
|---|---|---|---|---|---|---|---|---|---|---|
| binarytrees | 0.51 | 0 | 0 | 27 | 0 | 0 | 0 | 0 | 53 | startup/host 11 |
| fannkuchredux | 0.38 | 1 | 0 | 1 | 0 | 0 | 0 | 0 | 87 | startup/host 6 |
| fasta | 0.27 | 2 | 0 | 6 | 0 | 0 | 0 | 0 | 36 | JIT compiler 28 |
| knucleotide | 1.26 | 0 | 0 | 12 | 0 | 0 | 0 | 0 | 8 | strings 37 |
| mandelbrot | 0.17 | 1 | 0 | 2 | 0 | 0 | 0 | 0 | 71 | startup/host 14 |
| nbody | 0.16 | 2 | 0 | 3 | 0 | 0 | 0 | 0 | 53 | JIT compiler 21 |
| octane-crypto | 0.37 | 2 | 0 | 5 | 0 | 0 | 0 | 0 | 51 | JIT compiler 35 |
| octane-deltablue | 0.24 | 3 | 0 | 8 | 0 | 0 | 0 | 0 | 19 | JIT compiler 51 |
| octane-earley-boyer | 0.78 | 0 | 0 | 27 | 0 | 0 | 0 | 0 | 35 | JIT compiler 26 |
| octane-navier-stokes | 0.28 | 3 | 0 | 5 | 0 | 0 | 0 | 0 | 42 | JIT compiler 38 |
| octane-raytrace | 0.33 | 5 | 0 | 12 | 0 | 0 | 0 | 0 | 30 | JIT compiler 38 |
| octane-regexp | 0.52 | 2 | 0 | 6 | 0 | 0 | 40 | 0 | 5 | JIT compiler 28 |
| octane-richards | 0.15 | 3 | 0 | 4 | 0 | 0 | 0 | 0 | 38 | JIT compiler 41 |
| octane-splay | 0.32 | 1 | 0 | 45 | 0 | 0 | 0 | 0 | 15 | JIT compiler 26 |
| pidigits | 0.35 | 0 | 0 | 4 | 0 | 0 | 0 | 65 | 7 | startup/host 12 |
| regexredux | 0.39 | 0 | 0 | 27 | 0 | 0 | 45 | 0 | 4 | strings 11 |
| revcomp | 0.11 | 2 | 0 | 3 | 0 | 0 | 0 | 0 | 27 | JIT compiler 36 |
| spectralnorm | 0.13 | 1 | 0 | 4 | 0 | 0 | 0 | 0 | 44 | JIT compiler 31 |

## What the big js2wasi3 costs are

- **Property access** (`get_named` 9.3% of all samples on average, `get` 7.5%, `find_own_named` 5.5%, `set_named`
  1.9%, `set` 1.7%). Objects are dictionaries with a per-site entry-index guess (T74). Most of the cost is the
  *hit* path in `get_named` itself: load the site's guess, compare the key, check for an accessor. The ~5% of full
  hash lookups (`find_own_named`) come from:
  - writes to existing properties: `set_named` has no guess and always hashes (15% of nbody, 11% of richards; F372;
    **fixed by T105**, 2026-10-11: nbody −14.2%, richards −10.6% on a-03);
  - prototype hits (methods): the guess caches own properties only (F321, F359; T99's prototype cache didn't pay);
  - sites that see more than one layout.
- **Numbers** (`box_number` 9.1%, `to_number` 7.0%). A number outside the i31 range or not an integer is a `$number`
  struct, and every operation on a `jsval` tests and unboxes it. Locals that only ever hold numbers are `f64`
  already (D1), but:
  - mandelbrot (77%): `var Zr, Zi, Cr, Ci, Tr, Ti` are declared without a value, so their type includes `undefined`
    and they stay `jsval`, although every read follows a write (F389; **fixed by T103**, 2026-10-10: 3,955 → 254 ms on
    a-03, node ~180 ms);
  - spectralnorm, fannkuchredux, fasta: `let i, j` without a value made the loop counters (and spectralnorm's
    `eval_A` parameters) `jsval` (**fixed by T106**, 2026-10-11: spectralnorm −32%, fannkuchredux −8%, fasta −5%);
  - nbody, navier-stokes, spectralnorm, crypto: the numbers live in object properties and arrays, which hold `jsval`s
    (F323, F391). Arrays: **T107** (2026-10-11) keeps number arrays unboxed: spectralnorm −49%, navier-stokes −19%,
    fannkuchredux −9%, crypto −7%, fasta −5% (a-03); **T108** (2026-10-11) reads elements unboxed in a `+` under a
    numeric consumer: navier-stokes another −47% (2,859 → 1,513 ms); object properties still box (F323).
- **Object construction** (T109, 2026-10-12): layouts per allocation site (constructors' `this.k =` keys, literal
  keys) pre-size the object and add keys without a search: splay −27%, binarytrees −26%, earley-boyer −24%,
  richards −3% (a-03); nbody unchanged (its time is reads and boxed number fields: F323, F405).
- **GC** (wasmtime's `CopyingHeap::forward`/`scan_field`/`collect`): leads splay (78%), binarytrees (57%),
  earley-boyer (31%). T98's 16 MiB reserve is in these numbers; F347 has the heap-size trade-off.
- **regex** (`$$reRun`, `re_alts_find`): a VM in the prelude; 81% of octane-regexp, 84% of regexredux (F336).
- **Calls are not the problem**: wasmtime's `-C inlining=y` (compile and run) made mandelbrot, nbody, richards,
  deltablue and binarytrees 0–3% faster, same output. The cost is the work inside the helpers.

## Why not node's techniques

node's speed comes from techniques that are known to work. What each one needs, and where js2wasi3 stands:

| node (V8) | what it needs | js2wasi3 |
|---|---|---|
| speculative JIT: optimize for the types seen, deoptimize when wrong | generating machine code while the program runs | not possible: a Wasm component's code is fixed when it is built (no code generation at run time in Wasm or WASI). Only ahead-of-time versions apply: static type inference, guarded fast paths |
| hidden classes + inline caches | a shape per object; the shape check inlined into the code | inline caches since T74. Hidden classes were built (T83–T86): micro-benchmarks faster (methods 119 → 85 ms, `JSON.parse` objects 57 → 27), but the zigbee2mqtt replay 6% slower (transition search), so reverted (branch `shapes`). A hit still costs a helper call and Wasm type tests (~15 ns, F359), where V8's is one compare and a load |
| unboxed doubles (registers in optimized code, double fields) | type feedback, or proof the value is always a number | locals proven numeric are `f64` (D1); the proof misses `var x;` (F389); object fields and arrays hold `jsval`s (F323, needs shapes) |
| generational GC (cheap young collections) | owning the collector | the host's WasmGC collector (D1); wasmtime 49 has a copying collector (copies all live data) and DRC (7–15× slower, T98). Not ours to change; we can allocate less and size the heap (T98, F347) |
| irregexp: each pattern compiled to machine code | run-time code generation | possible ahead of time: regex literals are known at build time and could be compiled to Wasm functions (F336) |

Running V8 itself inside Wasm doesn't help either: V8 can't JIT there, which leaves its interpreter (jitless mode).
Existing JS-in-WASI runtimes (componentize-js, StarlingMonkey) embed SpiderMonkey's interpreter in Wasm for the same
reason.

## Next steps, by expected gain per effort

1. ~~A write guess in `set_named` like `get_named`'s (F372): up to 15% of nbody, 11% of richards.~~ Done (T105:
   nbody −14.2%, richards −10.6%).
2. ~~`var x;` with every read after a write stays `f64` (F389): mandelbrot's 77%.~~ Done (T103: mandelbrot 15.6×
   faster, the other 17 benchmarks unchanged).
3. ~~Regex literals compiled ahead of time (F336).~~ Done (T114, 2026-10-13): build-time programs run by one Wasm
   interpreter, as Hermes: octane-regexp 56.7× → 10.5× node, regexredux 6.8× → 4.5× (a-03).
4. Numbers in object fields without boxing (F323): needs shapes (F321). Arrays: done (T107).

## Same programs under scriptc

[scriptc](https://github.com/vercel-labs/scriptc) 0.2.7 compiles TypeScript and JavaScript ahead of time to native
executables (LLVM) or to `wasm32-wasi` modules (WASI preview 1, linear memory), each with its own runtime library
(reference counting and a cycle collector); with TypeScript types it compiles typed values directly. Run 2026-10-10 on
the machine above with `scripts/vs-scriptc/prep.sh` and `run.sh`:

- **One source file per program**, the same bytes for every runtime (checksummed before and after the run):
  - set A: the 18 benchmarks as `scripts/bench.sh` builds them, and scriptc's 13 single-file workloads
    (`benchmarks/runtime/workloads`, commit `1bf547a`, its arguments) with the types stripped by node
    (`module.stripTypeScriptTypes`);
  - set B: the 13 workloads' `.ts` files. node and scriptc read them; js2wasi3 has no TypeScript front end and runs
    its build of the same file with the types stripped (set A's module).
- Two changes in the shared benchmark files so that scriptc can build them: `var window;` before Octane's base.js
  (`Cannot find name 'window'`) and `Number(process.argv[i])` for `+process.argv[i]` (scriptc: `comparing non-number,
  non-string values are not supported yet`). node's output on every shared file equals its output on the original.
- Builds: `scriptc build` (default release mode, no `--dynamic`: no QuickJS in the executables);
  `SCRIPTC_TARGET=wasm32-wasi` with `@scriptc/runtime-wasm32-wasi` 0.2.7 and zig 0.17.0 as the linker; js2wasi3
  `build` (release). Both Wasm kinds precompiled (`wasmtime compile`) and run by wasmtime 49.0.2.
- 3 interleaved rounds per program (node, scriptc native, scriptc Wasm, js2wasi3), wall time, median. A runtime's
  output (stdout + stderr + exit status) must equal node's byte for byte, otherwise it is "fails" and not timed.
  "no build": the compiler rejects the file.

**Set A: JavaScript (identical .js/.mjs file for all four)**

| program | node s | scriptc native s | scriptc Wasm s | js2wasi3 s | js2wasi3 / scriptc Wasm |
|---|---|---|---|---|---|
| binarytrees | 0.38 | 6.32 | 10.36 | 9.14 | 0.88× |
| fannkuchredux | 0.39 | 36.49 | 69.46 | 9.02 | 0.13× |
| fasta | 0.25 | 5.18 | 8.53 | 0.96 | 0.11× |
| knucleotide | 1.20 | 2.85 | 4.74 | 5.18 | 1.09× |
| mandelbrot | 0.18 | 4.43 | 7.08 | 3.90 | 0.55× |
| nbody | 0.15 | 46.01 | 95.54 | 6.06 | 0.06× |
| pidigits | 0.36 | fails | fails | 0.99 |  |
| regexredux | 0.42 | 1.03 | 1.38 | 3.01 | 2.18× |
| revcomp | 0.09 | fails | fails | 0.52 |  |
| spectralnorm | 0.12 | 9.12 | 15.57 | 1.26 | 0.08× |
| octane-richards | 0.13 | fails | fails | 5.02 |  |
| octane-deltablue | 0.14 | fails | fails | 3.74 |  |
| octane-raytrace | 0.21 | no build | no build | 7.27 |  |
| octane-navier-stokes | 0.20 | fails | fails | 3.44 |  |
| octane-crypto | 0.26 | no build | no build | 5.83 |  |
| octane-splay | 0.21 | no build | no build | 3.62 |  |
| octane-earley-boyer | 0.51 | no build | no build | 10.21 |  |
| octane-regexp | 0.40 | fails | fails | 22.33 |  |
| json-records | 0.58 | 1.34 | 2.10 | 3.91 | 1.86× |
| regex-logs | 0.28 | 0.23 | 0.41 | 2.55 | 6.18× |
| ast-interp | 0.29 | fails | fails | 2.38 |  |
| records-sort | 0.29 | 2.77 | 4.42 | 4.93 | 1.12× |
| template-render | 0.21 | 0.19 | 0.29 | 1.10 | 3.81× |
| functional-pipeline | 0.35 | 2.72 | 3.96 | 4.66 | 1.18× |
| async-pipeline | 0.11 | fails | fails | 0.28 |  |
| word-graph | 0.60 | 1.27 | 1.97 | 2.49 | 1.26× |
| alloc-trees | 0.16 | 6.77 | 13.19 | 2.06 | 0.16× |
| numeric-kernels | 0.17 | fails | fails | 0.95 |  |
| validate-errors | 0.78 | 1.37 | 2.22 | 3.30 | 1.49× |
| csv-numbers | 0.62 | 0.42 | 0.77 | 4.78 | 6.23× |
| log-lines | 0.72 | 0.35 | 0.82 | 0.60 | 0.73× |

geomean over the 18 programs all four run: scriptc native / node 6.69×, scriptc Wasm / node 11.35×, scriptc Wasm / native 1.70×, js2wasi3 / node 8.59×, js2wasi3 / scriptc Wasm 0.76×

runs with node's output: node 31/31, scriptc native 18, scriptc Wasm 18, js2wasi3 31 of 31

**Set B: TypeScript (identical .ts file; js2wasi3: the same file with node's type stripping)**

| program | node s | scriptc native s | scriptc Wasm s | js2wasi3 s | js2wasi3 / scriptc Wasm |
|---|---|---|---|---|---|
| json-records | 0.60 | 0.23 | 0.47 | 3.85 | 8.29× |
| regex-logs | 0.36 | 0.19 | 0.36 | 2.54 | 7.11× |
| ast-interp | 0.33 | 0.25 | 0.53 | 2.37 | 4.51× |
| records-sort | 0.33 | 0.17 | 0.35 | 4.89 | 14.06× |
| template-render | 0.31 | 0.10 | 0.17 | 1.11 | 6.42× |
| functional-pipeline | 0.38 | 0.14 | 0.31 | 4.62 | 14.71× |
| async-pipeline | 0.15 | 0.04 | 0.09 | 0.27 | 3.09× |
| word-graph | 0.66 | 0.28 | 0.48 | 2.50 | 5.19× |
| alloc-trees | 0.23 | 0.08 | 0.22 | 2.04 | 9.34× |
| numeric-kernels | 0.23 | 0.07 | 0.12 | 0.95 | 7.63× |
| validate-errors | 0.84 | 0.22 | 0.41 | 3.29 | 8.03× |
| csv-numbers | 0.68 | 0.27 | 0.50 | 4.76 | 9.50× |
| log-lines | 0.79 | 0.15 | 0.39 | 0.61 | 1.56× |

geomean over the 13 programs all four run: scriptc native / node 0.36×, scriptc Wasm / node 0.75×, scriptc Wasm / native 2.06×, js2wasi3 / node 4.99×, js2wasi3 / scriptc Wasm 6.63×

runs with node's output: node 13/13, scriptc native 13, scriptc Wasm 13, js2wasi3 13 of 13

scriptc's failures, the same natively and in Wasm: raytrace, crypto, splay, earley-boyer don't build (TypeScript
errors in Octane's code: `Cannot find name 'DV'`, `Variable 'Flog' is used before being assigned`, `Cannot find
namespace 'SplayTree'`, `Cannot find name 'jsstring2string'`); at run time pidigits (`comparing non-number,
non-string values`, BigInt), revcomp (`'string.toString' ... has no scriptc lowering yet`), richards, navier-stokes,
regexp (`value.toPrecision is not a function`), deltablue (`inheritsFrom is not a function`), ast-interp (`spreading
'unknown' into Math.max`), async-pipeline (`Promise.all with this combined result type`), numeric-kernels (`Checked
array construction exceeds the native allocation limit`).

**What the measurements say** (only what the tables show):
- JavaScript, both in Wasm on the same wasmtime: js2wasi3 runs all 31 programs with node's output, scriptc 18; on
  those 18 js2wasi3 takes 0.76× scriptc's time (geomean): less on 8 (nbody 0.06×, spectralnorm 0.08×, fasta 0.11×,
  fannkuchredux 0.13×, alloc-trees 0.16×, mandelbrot 0.55×, log-lines 0.73×, binarytrees 0.88×), more on 10
  (csv-numbers 6.2×, regex-logs 6.2×, template-render 3.8×, regexredux 2.2×, json-records 1.9×, validate-errors 1.5×,
  word-graph 1.3×, functional-pipeline 1.2×, records-sort 1.1×, knucleotide 1.1×).
- The same 13 workloads with types (set B) and without (set A): scriptc's Wasm takes 4.8× longer without them
  (geomean of the 10 that run both ways; alloc-trees 60×, functional-pipeline 13×, records-sort 13×, csv-numbers
  1.5×, regex-logs 1.1×). With types it is 0.75× node's time, and js2wasi3 (the same module either way) 6.6× its time.
- scriptc's Wasm build against its native build: 1.70× (JavaScript), 2.06× (TypeScript), geomean.
- Not shown by these runs: which part of js2wasi3 makes up its gap on any one program. For that, the profiles above.
