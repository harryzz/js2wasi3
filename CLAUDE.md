# js2wasi3

Compiler: JavaScript → a WASI 0.3 Wasm component (WasmGC, native EH, P3 async). Written in Rust.

## Start of every session
1. Read `docs/design.md` (decisions D1–D5) and `tasks/README.md` (task index and status).
2. Work on **one** task file, `tasks/Txx-*.md`. Read its "Read first" list and nothing unrelated.
3. Run `scripts/check-tools.sh`. If tools changed, rerun `probes/run.sh` and update `TOOLS.md`.

## Cloud sessions (Claude Code on the web)
No cloud batch is open (`tasks/CLOUD-BATCH.md`: the T83 batch is finished and merged). User, 2026-10-05: cloud
sessions use the same plan limits, so speed tasks run locally now.

**Local sessions**: shapes (T83–T86) were reverted (branch `shapes`). T87–T98 are done (T91: `Map`/`Set` hashes; T92/T93: object tests without `is_subtype`; T94: `string_hash` memo;
2026-10-08/09). T95 (computed-key caches) was measured and dropped, no code kept. T96 (generic `get`/`set` without `is_subtype`) is done. T97 (typed-array `.length` fast path) and T98 (16 MiB GC heap reserved at start, `build --gc-reserve`) are done.
T99 (prototype-hit cache, F359) was measured and dropped on 2026-10-10 (replay 20.64 s vs 20.28 s), no code kept.
T100 (string constants as one GC root, F322) was measured and dropped on 2026-10-10 (benchmarks 3–7% slower), no code kept.
T101 (the Node.js compatibility page, `scripts/compat.mjs`, https://harryzz.github.io/js2wasi3/) is done: re-run
`node scripts/compat.mjs` after tasks that add node APIs. T102 (watchpoint flags, F390: iteration sees a replaced
`next`, `regexp_plain` one bit) is done. T103 (`var x;` written before every read stays `f64`, F389: mandelbrot 15.6× faster) is done. T104 (F276: a `var` read before its initializer is `undefined`) is done. T105 (F372: a per-site entry guess for
`set_named`: nbody −14%, richards −11%) is done. T106 (`let x;` stays `f64`: spectralnorm −32%) is done. T107 (F391: number arrays
unboxed: spectralnorm −49%, navier-stokes −19%) is done. T108 (F401: element reads in a `+`
under a numeric consumer unboxed: navier-stokes −47%) is done. T109 (F392, object layouts per
allocation site: splay −27%, binarytrees −26%, earley-boyer −24%, replay −1.2%) is done. T110 (F405, layout reads/writes inline) is paused: knucleotide +7.2% (F409, resolved by T113), code committed in a52761d. T111 (F406, number fields unboxed) is paused before implementation. T112 (root-cause map `docs/root-causes.md`, `build --count`, `scripts/gate.sh`, rules in `docs/design.md` "Speed work") is done. T113 (R1: plain objects a final `$plain`, object tests without `is_subtype`: deltablue −8.6%, richards −8.1%; primitives +2…+6 ns, F411; the gate counts instructions) is done. T114 (R6: regex literals compiled at build time into programs for one Wasm interpreter, as Hermes: octane-regexp 56.7× → 10.5× node) is done. T115 (array element reads inline on untyped receivers; non-arrays miss into `get_other`: nbody −12.7%, fannkuchredux −9.9% on a-03) is done. T111 (F406, number fields unboxed: number cells owned by the object, V8's double fields: nbody −20.8% on a-03) is done. T116 (the same components on node's V8 via jco vs wasmtime: wasmtime/V8 1.52×, its copying GC 4–8× V8's; `scripts/v8bench.sh`) is done. T117 (layout objects share keys/attrs: earley-boyer −22.7%, binarytrees −5.1%; splay +65% from wasmtime's grow-or-collect rule, F304) is done. T118 (F422, array element writes inline on untyped receivers; misses into `set_other`/`set_generic`: fannkuchredux −24.5%, spectralnorm −11.3% on a-03; mandelbrot +5.5% at −8.8% instructions, placement) is done. T119 (F428, `o.k++` keeps a constant key: richards −9.4% on a-03) is done. T120 (F429, typed arrays follow ECMA-262 10.4.5 for every key: canonical numeric keys never properties; a correctness fix) is done. T121 (F430–F433: `%TypedArray%`, typed `[[Set]]` with another receiver, freeze/seal of typed arrays) is done. T122 (wasmtime 49.0.3) is done. T123 (wasmtime 50.0.0-rc.1 measured: pidigits −26.4%, splay −9.1%, binarytrees −7.2%; pin 50.0.0 when final, F436) is done. T124 (F427 measured: doesn't pay, splay is ~92% wasmtime's collector; the target is F407, raytrace) is done. T125 (F85: StringToNumber always linked, the runtime's ToNumber of a string no longer traps; +11.6 KB in small modules, F437) is done. T126 (F106/F108: subclasses of Array/Object/RegExp, Array species, `console.log` names; test262 +14) is done. T127 (F438: Promise species in `then`, `P [Promise]` display; test262 +4) is done. T128 (F407: layouts learned per function object at run time, `construct` swapped once decided; adds over prototype defaults cached: raytrace −4.4%, deltablue −1.0% on a-03; follow-ups F439–F444) is done. T129 (gate fixes: paired runs, same path, instructions in every run, `scripts/micro-verdict.py`; laptop micro verdicts advisory, a-03 decides) is done. T130 (F441: inline reads from the site's remembered entry) was measured on a-03 and dropped: geomean 0.9969, `.wasm` +16.9%, 11 of 18 slower; no code kept. T000 (2026-10-17, `tasks/T000-general-speed-analysis.md`): general analysis of the ~6.8× gap to node on V8: ~99% of executed operations see one type per site; hand-written probes of a new design (struct per shape, `f64` fields, guarded fast paths) run 19–22× faster than our output and as fast as node on V8. T131 (whole-program type analysis, `crates/types`, `js2wasi3 types`, `scripts/typecover.sh`) is done: 95.7% of executed operations typed and right (best possible 99.3%). T132 (direct guarded method calls at one-target sites: geomean 0.9790 on a-03, richards −18.8%, deltablue −20.9%, `.wasm` +0.62%, test262 no changes) is done. T133 (objects as one struct per shape after today's header, `f64` fields, guarded typed reads/writes: geomean 0.9647 on a-03, nbody −46.1%, raytrace +2.7% accepted, `.wasm` +1.34%, test262 no changes) is done. T134 (re-planned after profiling: split number slots, a number-typed local/parameter keeps an `f64` next to its `jsval`: geomean 0.9327 on a-03 (minimum times), crypto −31.3%, navier-stokes −27.0%, fannkuchredux −20.5%, `.wasm` +0.7…+5.2%, test262 no changes) is done. T135 (Binaryen `wasm-opt -O3` on our output) was measured and dropped on 2026-10-19: `.wasm` −23% but time geomean 1.25 on the laptop (nbody +227%), no code kept (patch in `tasks/T135-files/`). T136 (element-to-element copies unboxed) was measured and dropped on 2026-10-19: geomean 0.9992 on a-03 (fannkuchredux's copies were never boxed), no code kept (patch in `tasks/T136-files/`); `scripts/gate.sh` now prints the geomean of minimum times (gate.tsv lists every run). T137 (`f.apply(x, arguments)` forwards the argument list, no `arguments` object: geomean 0.9665 on a-03, raytrace −44.5%, `.wasm` +0.03% at most, test262 no changes) is done. T138 (profile of fannkuchredux and crypto, no code: integers are `f64` everywhere; F474–F477) is done. T139 (F474: a split number slot boxed inline instead of `box_number`) was measured on the laptop and dropped on 2026-10-20: fannkuchredux −3.0% instructions, crypto +1.0% (patch in `tasks/T139-files/`). T140 (probe, no compiler change: fannkuchredux hand-written in four models of our code; the receiver typed once −42%, `i32` counters −27%, plus an `i32` element store 3.1× faster than today's shape on wasmtime; `probes/fannkuch-*`) is done. T141 (F476: a local holding only array literals is a typed `(ref null $array)` local, no type test/cast per element access: geomean 0.9881 on a-03, fannkuchredux −16.2%, `.wasm` +0.03% at most, test262 no changes) is done. T142 (F475, the smallest piece: array keys that are integer-valued locals skip the integer test: geomean 0.9827 on a-03, spectralnorm −12.9%, fannkuchredux −4.2%, `.wasm` −0.13…−0.45%, test262 no changes; `i32` locals not built) is done. T143 (F472: `arguments.length` / `arguments[k]` and `var a = arguments` aliases read the argument array, no `arguments` object: geomean 0.9957 on a-03, earley-boyer −6.7%, test262 no changes) is done. T144 (profile of richards and deltablue, no code: `== null` 23% of richards, `method_check` 21%/15%, prototype reads 40% of deltablue; F480–F482) is done. T145 (F480: `x == null` / `x != null` inline: geomean 0.9864 on a-03, richards −17.2%, `.wasm` +0.06% at most, test262 no changes) is done. T146 (wasmtime 50.0.0 pinned on the laptop and a-03, 2026-10-21: fixtures 281/281, test262 no changes; speed numbers before this date are from 49.0.3) is done. Speed gates run on a-03 and report the 18-program geomean, ms, instructions and `.wasm` size. Next: not chosen. Every speed change goes through `scripts/gate.sh`.
**User, 2026-10-12: all comparisons and data only from the standard tests and benchmarks (fixtures, test262, the 18
programs of `tests/bench`, micro-benchmarks).**

## Rules
- Stay inside the task's scope. Record anything left for later (in the task file and in the Follow-ups table of `tasks/README.md`); don't implement it.
- **Write progress every iteration, not only at the end.** After each step (build/test run, design choice,
  blocker), append a dated line to the task file's `## Progress` log. Set the task's status to `in progress`
  in `tasks/README.md` when you start it. The goal: a new session can pick up the task from the files alone.
- Tests per iteration: the task's own tests plus `wasm-tools validate` plus one `wasmtime` smoke run. Never the whole suite.
- At the end of a task:
  1. tick the task file's checklist and write its "Notes / Result" (what changed, commands run, what's unsupported, the next smallest step);
  2. set the status in the table in `tasks/README.md`;
  3. add every deferred item to the **Follow-ups** table in `tasks/README.md` (next free `F<n>`, found in, pick up in),
     and tick any follow-up this task resolved.
- The only test host is wasmtime 50.0.0 (T146). The JS oracle is `node` 24.21.0 (latest LTS; pinned in `TOOLS.md`).
- Differential tests: `scripts/diff.sh [fixture.js | dir]` runs node and js2wasi3 + wasmtime and compares
  stdout, stderr and exit status (default: all of `tests/fixtures`; `x.skip` skips `x.js` with a reason).
  Run it on the fixtures you touch; a new command fixture needs no expected-output file (node is the oracle).
  See "Testing" in `docs/design.md`.
- Subagents (Haiku 5.5, `.claude/agents/`): use `grep` to find call sites and `search` to map unfamiliar parts of
  the repo when the answer needs many files and only the conclusion matters; use `audit` to run fixtures
  (`scripts/diff.sh`, `wasm-tools validate`, a wasmtime smoke run) on committed code. Read the code being
  changed in the main session. Long runs (test262, `scripts/gate.sh`, anything on a-03) stay in the main
  session and still need a yes first.

## Layout
- `crates/frontend`: oxc parse + semantic
- `crates/ir`: IR + lowering
- `crates/backend`: core Wasm
- `crates/component`: WIT + packaging
- `crates/js2wasi3`: CLI
- `tests/fixtures/*.js`: small programs. Libraries have node's results in `*.out` where relevant; commands are checked by `scripts/diff.sh`
- `probes/`: hand-written WAT proving toolchain features (`probes/run.sh`)
