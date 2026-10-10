# T103: `var x;` stays `f64` when every read follows a write (F389)

Status: todo | Depends on: T102 | Local

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
- [ ] Measure first (a-03, perf; `scripts/profile.sh <benchdir> <outdir> mandelbrot`): confirm the 77% and which `var`s it is; check
  the other benchmarks for `var x;` locals read only after a write (nbody, spectralnorm, fannkuch, navier-stokes).
- [ ] A definite-assignment pass over a function's body (oxc AST + semantic's references): the set of `var`s whose
  every read is preceded by a write on all paths; conservative on `try`/`catch`, `switch`, labels, `break`/`continue`,
  closures (captured: skip), `eval`/`with` (skip), and reads in a loop's test/update before the body's write.
- [ ] Those `var`s skip the `Const::Undefined` observation and the `var_undefined` entry; the rest unchanged.
- [ ] Fixture `tests/fixtures/syntax/var-definite.js`: reads before writes on one branch,
  in a loop's second iteration, after `break`, in `catch`/`finally`, `typeof x` before a write, a closure reading
  it, a `var` redeclared in a loop, `for (var i in …)`): all equal to node.
- [ ] `scripts/diff.sh` on the fixture dirs the change touches; `cargo test -p js2wasi3-lower`; test262 subsets
  `language/statements/variable`, `language/statements/for`, `language/expressions/typeof`; the full run on a-03
  (ask).
- [ ] Before/after on a-03: mandelbrot plus the 18 benchmarks (5 interleaved runs, as in T102).

## Acceptance
- Every touched fixture = node; test262 0 regressions.
- mandelbrot's box/unbox share well below 77%; no benchmark slower beyond noise.

## Notes / Result

## Progress
- 2026-10-10: task created as the handoff after T102 (F389 picked: the largest single measured share, 77% of
  mandelbrot, and a contained change in `crates/lower`). Not started.
