# T106: `let x;` written before every read stays `f64` too (T103 for `let`)

Status: done (2026-10-11) | Depends on: T105 | Local

## Goal
Picking F391 (unboxed number arrays) up, the IR of spectralnorm showed a cheaper cause for its 40% "numbers":
`eval_At_times_u` declares `let i, j` without a value, so `i` and `j` hold `undefined` first and are `jsval`; they
are passed to `eval_A(i, j)`, whose parameters become `jsval` too, and its whole formula runs through `$js_add`
with boxed numbers. T103 proved `var`s only. A `let` declared without a value whose every read follows a write is
the same case.

## Design
- `definite.rs`: candidates also include uncaptured block-scoped `let` bindings (not the root scope's, every
  declaration a `VariableDeclarator`). `let x;` (no initializer) **resets** `x` in the walk (it's `undefined` again
  each time the declaration runs, e.g. in a loop body); `var x;` stays a no-op.
- `var_decl`: `let x;` of a plain local in `definite` stores nothing.
- Not here: F391 (the array elements themselves are still boxed: `Au[i] += …`), recorded with what this measures.

## Read first
- `crates/lower/src/definite.rs`, `var_decl` / `declare_store` in `crates/lower/src/lib.rs`, T103/T104 notes.

## Plan
- [x] Candidates + the reset; `var_decl`.
- [x] Fixture `tests/fixtures/syntax/let-definite.js` (`let x;` read after a write, in a branch, in a loop body
  across iterations, TDZ read before the declaration, a closure) = node; IR types as expected.
- [x] IR before/after of the 32 programs: which locals change type; `scripts/diff.sh` on the touched dirs;
  test262 (full run on a-03, ask); a-03 benchmarks for the programs whose IR changed.

## Acceptance
- Fixtures = node; test262 0 regressions; spectralnorm faster; nothing slower.

## Notes / Result
**What changed** (`crates/lower` only): `definite.rs`'s candidates include uncaptured, non-`const` block-scoped
variables; `let x;` without an initializer unassigns `x` in the walk (it runs `x = undefined` each time, e.g. per
loop pass); `var_decl` skips the `undefined` store of a `let x;` in `Analysis::definite` as for `var x;`. (T104's
`maybe_undefined` stays for `var`s only: a `let` read early is a TDZ error.)

**Result** (a-03, 5 interleaved runs, outputs the same): spectralnorm 1,279 → 867 ms (−32.2%: `eval_A` is all `f64`
now), fannkuchredux −8.0%, fasta −5.3%; navier-stokes +5.4% and richards +1.4%, whose hot code is the same IR (only
the prelude's number formatting changed): a layout effect (the same source with one unused function under HEAD's
compiler: −1.7%; perf: the same runtime helpers on top, shares shifted), F399. The other 11 within noise.

**Tests.** `tests/fixtures/syntax/let-definite.js` = node; `scripts/diff.sh` on `syntax/`, `exceptions/`,
`closures/`, `generators/`, `async/`, `classes/`, `objects/`, `builtins/`, `loops.js`, `semantics.js`: all pass;
`cargo test -p js2wasi3-lower` ok; full test262 on a-03: no changes (28,105 of 28,984).

**Not done.** F391 (number arrays unboxed: spectralnorm's `Au[i] += …` still boxes each element). F398 (`switch`
gives the state before it: a `let w;` set in every case stays `jsval`). F399 (layout).

**Next smallest step.** None here. F391 is the next numbers item (navier-stokes 42%, crypto 32%).

## Progress
- 2026-10-11: task created (user: "Next up"; started on F391, found this first: `let i, j` in spectralnorm's
  `eval_At_times_u` makes `eval_A`'s parameters `jsval`; `js2wasi3 ir` of `~/t104/a/spectralnorm.js`).
- 2026-10-11: implemented: candidates include uncaptured non-`const` block-scoped variables; `let x;` unassigns `x`
  in the walk; `var_decl` skips the `undefined` store of a definite `let x;` too. spectralnorm's `eval_A` is now all
  `f64` (params `f64`, no `$js_add`). Fixture `tests/fixtures/syntax/let-definite.js` = node (two TDZ cases dropped:
  a use before the declaration is a build error today, "planned: later"). IR before/after (HEAD = T105 vs T106)
  of the 32 programs: locals now `f64`: spectralnorm `eval_At_times_u` `i, j` (and `eval_A`'s parameters),
  fannkuchredux `fannkuch` `i, r, j`, fasta `repeatFasta` `i`, `randomFasta` `i, j`, prelude
  `Number.prototype.toPrecision` `e` and `$$typedCreate` `length` (in every program using them). `scripts/diff.sh`
  on `syntax/`, `exceptions/`, `closures/`, `generators/`, `async/`, `classes/`, `objects/`, `builtins/`,
  `loops.js`, `semantics.js`: all pass; `cargo test -p js2wasi3-lower` ok.
- 2026-10-11: a-03 (user: both runs now): `~/js2wasi3-t105` (HEAD) vs `~/js2wasi3-t106` on the 18 benchmarks (`~/t106-bench.sh`, 5 interleaved runs, `~/t106-bench.log`), then the full test262 (`~/t262-t106.log`), one detached script (`~/t106-all.sh`).
- 2026-10-11: a-03 before/after (T105 vs T106, 5 interleaved runs, median ms, every output the same):
  | program | T105 ms | T106 ms | change |
  |---|---|---|---|
  | mandelbrot | 251 | 248 | -1.2% |
  | spectralnorm | 1,279 | 867 | -32.2% |
  | fannkuchredux | 8,986 | 8,271 | -8.0% |
  | nbody | 5,213 | 5,222 | +0.2% |
  | binarytrees | 9,170 | 9,161 | -0.1% |
  | fasta | 951 | 901 | -5.3% |
  | knucleotide | 5,119 | 5,100 | -0.4% |
  | pidigits | 992 | 986 | -0.6% |
  | regexredux | 2,991 | 3,022 | +1.0% |
  | revcomp | 518 | 506 | -2.3% |
  | octane-richards | 4,517 | 4,582 | +1.4% |
  | octane-deltablue | 3,756 | 3,704 | -1.4% |
  | octane-raytrace | 7,259 | 7,283 | +0.3% |
  | octane-navier-stokes | 3,347 | 3,528 | +5.4% |
  | octane-crypto | 5,791 | 5,762 | -0.5% |
  | octane-splay | 3,493 | 3,497 | +0.1% |
  | octane-earley-boyer | 10,230 | 10,231 | +0.0% |
  | octane-regexp | 22,094 | 22,094 | +0.0% |
  spectralnorm −32.2%, fannkuchredux −8.0%, fasta −5.3% (the locals above). But navier-stokes +5.4% (runs
  3,341–3,387 vs 3,508–3,558: no overlap) and richards +1.4% (4,504–4,520 vs 4,565–4,586). navier-stokes' IR
  differs only in the prelude's number formatting (`$$expForm`, `toPrecision`'s `e`, `$$refine`'s `L`), called once
  to print the result: the hot code's IR is the same. Suspect: machine-code layout (other functions' sizes move the
  hot loops). To check after test262: the old compiler on navier-stokes with an unrelated function added.
- 2026-10-11: full test262 on a-03 (1,298 s): no changes since the baseline; `baseline.json` copied back (identical). Started the navier-stokes check (`~/t106-ns.sh`, `~/t106-ns.log`): 7 interleaved runs of HEAD, T106 and HEAD + an unused function, and perf of HEAD vs T106.
- 2026-10-11: navier-stokes check (`~/t106-ns.log`, 7 interleaved runs, median): HEAD 3,452, T106 3,551 (+2.9%),
  HEAD + an unused function 3,391 ms (−1.7%); perf of HEAD vs T106: `get` 20.8 / 18.7%, `box_number` 16.4 / 17.7%,
  `to_number` 13.8 / 13.3%: the same helpers, shifted. richards' IR also differs only in `$$expForm`. So layout
  (F399), not T106's code. Docs updated. Done.
