# T103: `var x;` stays `f64` when every read follows a write (F389)

Status: done (2026-10-11) | Depends on: T102 | Local

## Goal
mandelbrot spends 77% of its time in `box_number`/`to_number` (`docs/performance.md`, the profile of 2026-10-10;
node: 0%). The cause: `var Zr, Zi, Cr, Ci, Tr, Ti;` are declared without a value, so the lowering stores
`Const::Undefined` into their slots (`Kind::Any` joins), and they become `jsval` although every read comes after a
numeric write. With a definite-assignment check, such a `var` doesn't observe `undefined`, and the type inference
(D1: a slot is `f64` exactly when every stored value is a number) keeps it `f64`.

The aim is the smallest sound rule, not a general data-flow framework: a `var` (function-local, not captured, not a
parameter) with no initializer, where every read is dominated by a write on all paths, in straight-line code, `if`/
`else`, and loop bodies whose first use of the variable is a write. Anything else keeps today's `undefined` store.

Out of scope: `i32` locals (F335), array element kinds (F391), captured variables and globals (record them as
follow-ups if a benchmark shows them).

## Read first
- `crates/lower/src/lib.rs`: the module doc's "Type inference" (lines 8–12); `enum Kind` (~319); the
  `var` hoisting pass that observes `Const::Undefined` for `decl.init.is_none() || undefined_first` (~1855–1870);
  `var_decl` (~3739; `var_undefined`, T88's parameter case ~3798).
- `docs/performance.md`: mandelbrot's row and the "numbers" findings (lines ~65, ~115, ~144).
- `docs/design.md` D1 (the value representation and the `f64` rule).
- `tests/fixtures/loops.js`, and fixtures that read a `var` before writing it (`grep -rln "var [a-z_]*;" tests/fixtures`).

## Plan
- [x] Measure first (a-03, perf; `scripts/profile.sh <benchdir> <outdir> mandelbrot`): confirm the 77% and which `var`s it is; check
  the other benchmarks for `var x;` locals read only after a write (nbody, spectralnorm, fannkuch, navier-stokes).
  (The 2026-10-10 profile was reused, not re-run; the IR showed the `var`s; the 18-program before/after covers the rest.)
- [x] A definite-assignment pass over a function's body (oxc AST + semantic's references): the set of `var`s whose
  every read is preceded by a write on all paths; conservative on `try`/`catch`, `switch`, labels, `break`/`continue`,
  closures (captured: skip), `eval`/`with` (skip), and reads in a loop's test/update before the body's write.
- [x] Those `var`s skip the `Const::Undefined` observation and the `var_undefined` entry; the rest unchanged.
- [x] Fixture `tests/fixtures/syntax/var-definite.js` (15 cases): reads before writes on one branch,
  in a loop's second iteration, after `break`, in `catch`/`finally`, `typeof x` before a write, a closure reading
  it, a `var` redeclared in a loop, `for (var i in …)`): all equal to node.
- [x] `scripts/diff.sh` on the fixture dirs the change touches; `cargo test -p js2wasi3-lower`; test262 subsets
  `language/statements/variable`, `language/statements/for`, `language/expressions/typeof`; the full run on a-03
  (ask).
- [x] Before/after on a-03: mandelbrot plus the 18 benchmarks (5 interleaved runs, as in T102).

## Acceptance
- Every touched fixture = node; test262 0 regressions.
- mandelbrot's box/unbox share well below 77%; no benchmark slower beyond noise.

## Notes / Result
**What changed** (`crates/lower` only):
- New `definite.rs`: one forward pass per function (declaration, expression, arrow with a block body) over its
  statements with the set of variables assigned so far (`None` after `return`/`throw`/`break`/`continue`). Writes:
  `x = e`, `var x = e`. `if` and `?:` meet by intersection, `&&`/`||`/`??` keep the left side's state; loops,
  `switch`, non-loop labels and `try` give the state before their bodies (plus the loop test or the discriminant);
  `finally` is checked from the state before the `try`. Anything else checks its reads against the state at its start
  and adds nothing, and a read the pass never reaches isn't safe: a missing case only loses precision. Candidates:
  `var`s outside the root scope, not captured, every declaration a `VariableDeclarator` (no parameters, no
  `function` redeclarations).
- `closures::Analysis::definite`, filled in `lower_with` after the closure analysis.
- `var_decl`: `var x;` of a plain local in `definite` stores nothing (no `undefined`, no `var_undefined` entry), so
  the type inference keeps it `f64` when every stored value is a number.

**Result.** mandelbrot (a-03, 5 interleaved runs): 3,955 → 254 ms (15.6×; node ~180 ms, so ~1.4× node instead of
~22×); its inner loop has no boxing left. The other 17 benchmarks within ±1%, every output the same.

**Tests.** `tests/fixtures/syntax/var-definite.js` = node (and the IR is `f64` exactly where every read follows a
write); `scripts/diff.sh` on `syntax/`, `exceptions/`, `closures/`, `generators/`, `async/`, `loops.js`,
`semantics.js`: all pass; `cargo test -p js2wasi3-lower` ok. test262: the statement/expression subsets (2,690 tests)
and the full run on a-03 (`~/t262-t103.log`): no changes since the baseline (28,105 of 28,984).

**Not done / found.** F398: the conservative spots (after `do`-`while`, writes inside unmodelled expressions,
command globals, captured `var`s); no benchmark needs them. F276 (a `var x = 1` in a branch that didn't run reads
`0`, a miscompile) is still open and can now use `Analysis::definite`: a `var` not in it is the one that must start
as `undefined`.

**Next smallest step.** F276 with this analysis (a correctness fix; check that no benchmark local changes type), or
the next speed item: F372 (a write guess in `set_named`: up to 15% of nbody) or F391 (unboxed number arrays).

## Progress
- 2026-10-10: task created as the handoff after T102 (F389 picked: the largest single measured share, 77% of
  mandelbrot, and a contained change in `crates/lower`). Not started.
- 2026-10-10: started. Reused the 2026-10-10 profile (77%, `docs/performance.md`) instead of a new a-03 run (no
  redundant runs). Implemented `crates/lower/src/definite.rs` (one forward pass per function; writes: `x = e`,
  `var x = e`; branches meet by intersection; loops/`switch`/labels/`try` give the state before their bodies;
  anything not modelled checks its reads against the state at its start and adds nothing; a read never reached
  is unsafe), `Analysis::definite`, and `var_decl` skips the `undefined` store for those. mandelbrot's
  `Zr, Zi, Cr, Ci, Tr, Ti` are `f64` now (`js2wasi3 ir`: no `jsval->f64`/`$js_add` in the inner loop). Fixture
  `tests/fixtures/syntax/var-definite.js` = node (15 cases; the IR shows `f64` exactly where every read follows a
  write). `scripts/diff.sh` on `syntax/`, `exceptions/`, `closures/`, `generators/`, `async/`, `loops.js`,
  `semantics.js`: all pass.
- 2026-10-10: `cargo test -p js2wasi3-lower` ok. test262 subsets (release, -j 4, local): `language/statements/
  {variable,for,do-while,while,try,switch,labeled,if,break,continue,block,for-in,function}`, `language/expressions/
  {typeof,assignment,conditional,logical-and,logical-or,compound-assignment}`: 2,690 tests, "No changes since the
  baseline". Next: a-03 before/after (mandelbrot + the 18 benchmarks) and the full test262 run (asked).
- 2026-10-10: a-03 (user: both runs now): T103 built there (`~/js2wasi3-t103`); before/after vs the T102 compiler (`~/js2wasi3-t102` = HEAD's) on the 18 benchmarks (`~/t103-bench.sh`, 5 interleaved runs, `~/t103-bench.log`) started; the full test262 follows it.
- 2026-10-10: a-03 before/after (`~/t103-bench.log`; the first start lacked `bash -l` (no `wasmtime` on PATH): killed, restarted), 5 interleaved runs, median ms, every output the same:
  | program | T102 ms | T103 ms | change |
  |---|---|---|---|
  | mandelbrot | 3,955 | 254 | -93.6% |
  | spectralnorm | 1,283 | 1,284 | +0.1% |
  | fannkuchredux | 9,227 | 9,218 | -0.1% |
  | nbody | 6,241 | 6,214 | -0.4% |
  | binarytrees | 9,347 | 9,326 | -0.2% |
  | fasta | 964 | 960 | -0.4% |
  | knucleotide | 5,257 | 5,235 | -0.4% |
  | pidigits | 993 | 984 | -0.9% |
  | regexredux | 3,062 | 3,088 | +0.8% |
  | revcomp | 518 | 523 | +1.0% |
  | octane-richards | 5,146 | 5,163 | +0.3% |
  | octane-deltablue | 3,803 | 3,807 | +0.1% |
  | octane-raytrace | 7,448 | 7,383 | -0.9% |
  | octane-navier-stokes | 3,379 | 3,381 | +0.1% |
  | octane-crypto | 5,902 | 5,904 | +0.0% |
  | octane-splay | 3,540 | 3,540 | +0.0% |
  | octane-earley-boyer | 10,331 | 10,334 | +0.0% |
  | octane-regexp | 22,586 | 22,648 | +0.3% |
  mandelbrot 15.6× faster (node ~180 ms: now ~1.4× node, was ~22×); the other 17 within ±1% (noise). Full test262 started on a-03 (`~/t262-t103.log`, `--update --causes 25 subset`).
- 2026-10-11: full test262 on a-03 (1,302 s, -j 36): no changes since the baseline; `baseline.json` copied back
  (identical). Docs: `design.md` (D1), `keywords.md` (`var`), `performance.md` (numbers, next steps), test262
  `HISTORY.md`, `tasks/README.md` (T103 done, F389 ticked, F276 note, F398), `CLAUDE.md`. Done.
