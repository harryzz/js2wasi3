# T135: Binaryen wasm-opt on our output (F467)

Status: done (2026-10-19): measured and dropped, no code kept | Depends on: T134 | Local (+ a-03 for a gate, after asking)

## Goal
Find out whether Binaryen's `wasm-opt` (133, `TOOLS.md`) makes our output faster on the 18 programs. V8's WasmGC
porting post (https://v8.dev/blog/wasm-gc-porting) reports Java WasmGC benchmarks 1.9× faster with it. An
experiment first: if it pays, decide how to use it (a build step, or the passes that matter done in our backend).

## Read first
- `TOOLS.md` (binaryen 133), `crates/component` (how the core module is wrapped), `scripts/bench.sh`.

## Plan
- [x] Wrap a `wasm-opt`-processed core module into the same component (does `--core` + `wasm-tools component new`
  give the same component as `build`?).
- [x] Does `wasm-opt` accept our modules (GC, EH, the P3 parts)? Which flags.
- [x] Instructions and `.wasm` size on the 18 (laptop), outputs = node; then decide.

## Notes / Result
- **Dropped** (user, 2026-10-19). `wasm-opt -O3` on our core module (laptop, the 18 programs, outputs = node):
  `.wasm` −18…−26% (mean −23.3%), but time geomean **1.21** (worse): fannkuchredux −23.3%, spectralnorm −26.7%,
  knucleotide −12.5%, crypto −5.1% faster; nbody +227%, pidigits +147%, earley-boyer +121%, raytrace +67%,
  mandelbrot +51%, fasta +30%, binarytrees/regexp/deltablue/splay +14…18% slower (`out/wasm-opt-t135-laptop.md`).
  The slow ones collect far more often (nbody: ~19% collector, ~30% wasmtime's type registry walking the globals at
  each collection); the cause is an interaction of `-O2`'s passes (none alone, not inlining, not `dae`, not the GC
  reserve), not found.
- **No code kept.** The experiment's `build` hook (`JS2WASI3_CORE_FILTER`), the wrapper script and the feature
  list are in `tasks/T135-files/` (`git apply tasks/T135-files/core-filter.patch`; run as
  `JS2WASI3_CORE_FILTER="tasks/T135-files/wo.sh $(cat tasks/T135-files/features.txt) -O3" js2wasi3 build …`).
- **Learned:** Binaryen 133's `-all` emits proposals wasmtime 49 rejects (compact imports, exact types from custom
  descriptors): list the features. `-O4` fails on our code (Flatten: unsupported `br_on_cast`). Closed-world/GUFA
  add ~nothing over `-O3` (crypto). T98's GC reserve (`array.new_default` + `drop`) is dead code to an optimizer.
- **If picked up again:** bisect pass pairs of `-O2` on nbody to find the extra allocation; then a reduced pass
  list might keep the size cut and the fannkuchredux/spectralnorm gains (F470).

## Progress
- 2026-10-19: started (user: "add follow-ups and try wasm-opt", after the V8 blog posts). F467–F469 added.
- 2026-10-19: `build` hook `JS2WASI3_CORE_FILTER=<command>` (the core module through `<command> in out` before packaging; experiment only). wasm-opt 133 with `-all` emits proposals wasmtime 49 rejects (compact imports, exact types from custom descriptors): explicit `--enable-*` list (sign-ext, mutable-globals, nontrapping-float-to-int, simd, bulk-memory(-opt), exception-handling, tail-call, reference-types, multivalue, gc, extended-const, multimemory). crypto (laptop): `-O3` instructions 31.22G → 29.87G (−4.3%), `.wasm` 594,683 → 460,843 (−22.5%), output same; `-O3 --closed-world` the same, `--gufa` the same, `--converge` −4.9%; `-O4` fails (Flatten: unsupported BrOn). Running the 18 with `-O3`.
- 2026-10-19: the 18 with `-O3` vs the same compiler without the filter (laptop, `scripts/bench.sh -r 2`,
  `out/wasm-opt-t135-laptop.md`; outputs all = node): `.wasm` **−18…−26%** (mean −23.3%) but geomean **1.21**
  (worse). Instructions: fannkuchredux −23.5%, spectralnorm −11.2%, knucleotide −6.3%, regexredux −4.8%, crypto
  −4.4%; nbody +100.8%, earley-boyer +100.2%, pidigits +89.1%, mandelbrot +47.0%, raytrace +44.8%, fasta +21.5%,
  deltablue/splay/regexp +10…11%, binarytrees +4.7%.
- 2026-10-19: why (nbody): its `-O3` build spends ~19% in wasmtime's collector and ~30% in its type registry
  (`Global::trace_root` → `Global::_ty`/`RegisteredType::root` per global per collection): it collects far more.
  Not the GC reserve (T98's dropped `array.new_default` is removed by `vacuum`/`precompute`: +2.4%; base with
  `--gc-reserve 0` +2.4% too), not inlining (`--skip-pass=inlining*`: still +120.8%), not `dae` (alone +12.6%;
  `-O3` without it still +121.6%). Single passes ~0% (optimize-casts, local-subtyping, heap2local, gsi,
  global-refining, optimize-instructions, remove-unused-brs, …); `-O1` +14.9%, `-O2` +121.8%: an interaction.
  Stopped there (asked the user).
- 2026-10-19: user: close the experiment (option 2). Hook reverted, kept as `tasks/T135-files/core-filter.patch`; task done.
