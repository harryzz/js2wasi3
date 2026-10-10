# Where the time goes: js2wasi3 vs node

Profiles of the 18 benchmark programs (`tests/bench`, `scripts/bench.sh`'s small sizes) under node 24.21.0 and
js2wasi3 + wasmtime 49.0.2, taken 2026-10-10 on a Xeon Gold 6240 server (72 threads, WSL2) with the build at
`e13d22c`. Run times and node ratios are on the [compatibility page](compatibility.md#benchmarks): js2wasi3 is 2.9×
(pidigits) to 54× (octane-regexp) node's wall time, most Octane programs 16–43×.

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
  - writes to existing properties: `set_named` has no guess and always hashes (15% of nbody, 11% of richards; F372);
  - prototype hits (methods): the guess caches own properties only (F321, F359; T99's prototype cache didn't pay);
  - sites that see more than one layout.
- **Numbers** (`box_number` 9.1%, `to_number` 7.0%). A number outside the i31 range or not an integer is a `$number`
  struct, and every operation on a `jsval` tests and unboxes it. Locals that only ever hold numbers are `f64`
  already (D1), but:
  - mandelbrot (77%): `var Zr, Zi, Cr, Ci, Tr, Ti` are declared without a value, so their type includes `undefined`
    and they stay `jsval`, although every read follows a write (F389);
  - nbody, navier-stokes, spectralnorm, crypto: the numbers live in object properties and arrays, which hold `jsval`s
    (F323).
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

1. A write guess in `set_named` like `get_named`'s (F372): up to 15% of nbody, 11% of richards.
2. `var x;` with every read after a write stays `f64` (F389): mandelbrot's 77%.
3. Regex literals compiled to Wasm ahead of time (F336): octane-regexp, regexredux.
4. Numbers in object fields and arrays without boxing (F323): needs shapes (F321) for fields.
