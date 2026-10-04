# Run-time benchmarks (T78)

Third-party benchmark programs, run under node and js2wasi3 by `scripts/bench.sh`.

## benchmarksgame/

The Computer Language Benchmarks Game (BSD-3, `benchmarksgame/LICENSE.md`):
https://benchmarksgame-team.pages.debian.net/benchmarksgame/ . The sources come from
`public/download/benchmarksgame-sourcecode.zip` in https://salsa.debian.org/benchmarksgame-team/benchmarksgame
(commit 40296663, 2025-03-07), one single-threaded node program per benchmark (js2wasi3 has no `worker_threads`):

| file | program |
|---|---|
| binarytrees.js | binarytrees.node-7 |
| fannkuchredux.js | fannkuchredux.node-8 |
| fasta.js | fasta.node-8 |
| knucleotide.js | knucleotide.node-8 |
| mandelbrot.js | mandelbrot.node-8 |
| nbody.js | nbody.node-8 |
| pidigits.js | pidigits.node-3 |
| regexredux.js | regexredux.node-4 |
| revcomp.js | revcomp.node-2 |
| spectralnorm.js | spectralnorm.node-8 |

## octane/

Octane 2 (BSD, `octane/LICENSE`): https://github.com/chromium/octane (commit 570ad1cc, 2017-04-12): `base.js` and
the benchmarks Richards, DeltaBlue, RayTrace, NavierStokes, Crypto, Splay, EarleyBoyer and RegExp, unchanged.
`driver.js` (ours) replaces Octane's shell runner (`run.js`: `load`, `print`); `prelude.js` (ours) declares what
the sloppy scripts assign without a declaration. `scripts/bench.sh` runs `prelude.js` + `base.js` + a benchmark +
`driver.js` as one file, with `Math.random` renamed `__random` (F331).

## patched/

Minimal changes for what js2wasi3 can't run yet, each a follow-up in `tasks/README.md`; `scripts/bench.sh` uses a
patched copy when there is one:

- fannkuchredux, nbody, spectralnorm, knucleotide: declarations for variables assigned without one (sloppy mode,
  F232);
- knucleotide, regexredux, revcomp: the input from the file `input.fa`, not stdin (F332);
- mandelbrot: a checksum of the bitmap instead of binary stdout (F332);
- raytrace: `Object_extend` instead of `Object.extend = …` (F331);
- navier-stokes: a local `result` instead of `this.result` (sloppy `this`, F232);
- earley-boyer: `\xNN` instead of octal escapes (F232), no `eval` in the unused `sc_jsNew` (F191).
