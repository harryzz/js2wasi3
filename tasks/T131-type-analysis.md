# T131: whole-program type analysis (step 1 of the new design: measure what the compiler can find)

Status: done (2026-10-17) | Depends on: T000 | Local

## Goal
`tasks/T000-general-speed-analysis.md` measured that ~99% of executed operations in the 18 programs see one type
per site, and two hand-written probes (`probes/layout*.wat`) showed that the proposed design (a WasmGC struct per
object shape, `f64` number fields, guarded fast paths with a generic fallback) is 19–22× faster than our output
today and as fast as node on V8 for a typical kernel.

The open question: **can our compiler find those types itself, at build time?** This task builds the analysis only
(no code generation changes) and measures its coverage: the share of executed operations whose site the analysis
types with a single type that is the one seen at run time.

Steps after this one (not in this task; only if this step says yes): (2) struct-per-shape objects with a header
(flags + overflow dictionary) and all generic code reading them; (3) `f64` fields where proven; (4) guarded fast
paths for field reads/writes and method calls, typed function versions.

## Design (step 1)
- Where: a new crate `crates/types` on the oxc AST + semantic (the input of `crates/lower`), so lowering can use the
  results later by node/span. CLI: `js2wasi3 types <file.js>` prints one line per site.
- What it computes (flow-insensitive, whole program, inclusion-based, 0-CFA style as in Hopc's type analyses):
  abstract values = number / string / boolean / undefined / null / an allocation site (`new F`, `{…}`, `[…]`,
  a function) / unknown. Type sets for every variable, parameter, return, `this` per function, and property `k`
  of every allocation site (plus each function's `prototype` object, with lookups falling back to it). Calls
  resolve callees from their type sets (incl. methods through prototypes, `f.call`/`f.apply`); built-ins from a
  small table (Math → number, string methods, etc.); anything else is unknown.
- Guesses, not proofs: the generated code will guard every guess, so the analysis may be unsound where that's
  cheap (e.g. a write through an unknown receiver doesn't poison every object). Field representation (`f64`)
  needs the per-object flag of probe 2 anyway.
- Sites typed: property reads/writes (receiver), method-call receivers, element reads/writes (receiver, key),
  direct calls (callee), arithmetic/bit/comparison/equality (operand types).
- Measurement: `typeprobe` (T000's tool, moved to `crates/typeprobe`) records run-time types by **allocation site**
  (objects tagged where they are made) and numbers/strings/…; joined with the analysis output by source offset:
  coverage = executions where the analysis named exactly one type and it was the run-time type / all executions,
  per kind and per program. Target: close to the 97–100% upper bound of T000.

## Read first
- `tasks/T000-general-speed-analysis.md` (the measurements, the probes, the design).
- `crates/frontend/src/lib.rs` (`Program`: `ast`, `semantic`, `user_end`), how `crates/lower` walks the AST.
- `tests/bench/octane/raytrace.js` (Prototype.js classes, `initialize.apply`), `deltablue.js`
  (`superConstructor.call`): the hardest patterns for the analysis.

## Plan
- [x] `crates/types`: abstract values, constraint collection over the AST, fixpoint; `js2wasi3 types` output.
- [x] `crates/typeprobe`: allocation-site tags at run time; join with the analysis; report per kind and program.
- [x] Run on the 18 programs (laptop; node runs only, no gate); report coverage next to T000's upper bound.
- [x] Unit tests for the analysis on small fixtures (`tests/fixtures/objects`, `classes`).
- [x] Decide with the user: continue to step 2 or not.

## Rules that apply
- Dated Progress line after every step; status in `tasks/README.md`.
- No change to generated code in this task; `cargo test` must stay green.
- Only the standard tests and benchmarks; report plain numbers; never commit/push without asking.

## Progress
- 2026-10-17: task written (user: "yes write T131 and start step 1"); status `in progress`.

- 2026-10-17: `crates/types` written (flow-insensitive, 0-CFA; closure-making functions copied per call site, at
  most 16 copies; objects from `new` one per constructor; built-in table); `js2wasi3 types <file>` (parses the file as
  a script so offsets are the file's own; `parse_entry` wraps CommonJS code in a function). Fixes on the way:
  earley-boyer didn't converge (property reads on built-in functions made ever longer built-in names: limited;
  per-`new`-site objects gave 3M-element sets: one object per constructor) → 120 ms.
- 2026-10-17: `crates/typeprobe` (T000's tool moved in; `--sites`: objects tagged where made, `inst@<prototype tag>`
  for objects from `new`), `crates/typeprobe/report.mjs` (join), `scripts/typecover.sh` (18 programs). First full
  run started.
- 2026-10-17: first full run: typed and right 78.6% of executed operations (best possible 99.3%). Fixes, each
  measured on all 18 (outputs = node every time): sites keyed by start:end (`this.a.b` had merged two reads) and
  null/undefined receivers not a type → 83.3%; equality against null/undefined or between references counted as one
  compare → 84.8%; built-in prototypes answer only for their real methods (deltablue's `inheritsFrom` on
  `Object.prototype` was never found), `Object.defineProperty(o, "k", {value})` → 85.8%; promises (`async` returns,
  `then`, `await`), `Map`/`Set`, `require('fs')`, `Buffer`, unknown ≠ one type → 87.3%; per-receiver method dispatch,
  64 copies per factory, `typeof x === "t"` narrowing in `if`/`?:`/`&&`, unknown globals are functions only if
  built-in → 88.4%; a global nothing assigns gives no value (crypto's `DV`) → 94.9%; constructors that return an
  object (binarytrees' `TreeNode`), learned from the first run → **95.7%** (`out/typecover-t131.md`). Analysis time
  ≤ 185 ms per program (earley-boyer).
- 2026-10-17: cleanup: debug traces removed, clippy clean; narrowing also after `if (test) return …;` (rest of the
  list sees the test false); 8 unit tests (`cargo test -p js2wasi3-types`); rerun: unchanged, 95.7%.
- 2026-10-17: user: step 2 next (T132 written).

## Notes / Result
**Done 2026-10-17.** The compiler can find the types: over the 18 programs (3,917M executed operations, node runs
with `typeprobe --sites`), the analysis names one type at sites covering **95.7%** of executions, and it is the
run-time type in 95.7%; the best any one-type-per-site scheme could do is 99.3% (`out/typecover-t131.md`).

| kind | executed | typed and right | best possible |
|---|---|---|---|
| property read | 345M | 67.3% | 99.3% |
| property write | 103M | 97.7% | 100% |
| method-call receiver | 89M | 84.2% | 95.4% |
| element read / write | 467M / 245M | 97.6% / 99.9% | 100% |
| call target | 69M | 98.4% | 100% |
| arithmetic / bit / compare | 1,150M / 240M / 343M | 99.7% / 100% / 97.9% | 100% |
| equality | 153M | 92.1% | 86.8% (null/reference compares need no type) |

Per program: 99.9–100% for fannkuchredux, mandelbrot, nbody, spectralnorm, navier-stokes, knucleotide, binarytrees,
fasta, crypto; richards 97.4%; revcomp 91.6%; splay 90.6%; deltablue 80.3%; raytrace 73.3%; earley-boyer 49.0%;
regexp 41.2% (F446). Analysis time ≤ 185 ms per program. No change to generated code.

What was added:
- `crates/types` (`analyze`, `analyze_source`): flow-insensitive, inclusion-based, 0-CFA over the oxc AST; functions
  that make closures copied per call site (≤ 64 copies); one abstract object per constructor; two runs (the first
  finds replaced prototypes and constructors that return objects); `typeof` narrowing; a built-in table (Math,
  String/Array/Map/Set/Promise methods, `Object.create`/`defineProperty`/`keys`, `require('fs')`, Buffer).
- `js2wasi3 types <file>`: one line per site (`start:end`, kind, groups, tags).
- `crates/typeprobe` (T000's tool; `--sites` mode), `crates/typeprobe/report.mjs`, `scripts/typecover.sh`.

Not done (follow-ups): F446 (remaining misses), F447 (run on the lowering's program), F448 (more narrowing), F449
(classes). Next smallest step: T132's probe of the two object layouts.
