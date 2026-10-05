# T88: EarleyBoyer's wrong result (F333)

Status: in progress (2026-10-05) | Depends on: — | Local

## Goal
Octane EarleyBoyer (`tests/bench/patched/earley-boyer.js` with the Octane harness, `scripts/bench.sh`) computes a
wrong result under js2wasi3 (`TypeError: Cannot read properties of undefined (reading 'cdr')`; node: done). Find the
miscompile, fix it, add a fixture.

## Progress
- 2026-10-05: reproduced (prelude + base + patched earley-boyer + driver, `fixed 0.05`). Both parts fail (Earley:
  'cdr', Boyer: 'car'). Bisected Boyer by stages against node: setup's rule table, translation, substitution and
  rewriting (with the count) equal; `tautologyp_nboyer` fails. Cause: scheme2js redeclares parameters
  (`function(sc_x_11, true_lst, false_lst) { …; var true_lst; var false_lst; …}`), and the lowering set every
  `var`-declared local to `undefined` at function entry, parameters too: `var a;` wiped argument `a`.
- 2026-10-05: fix (`crates/lower/src/lib.rs`): a parameter local isn't in `var_undefined`, and `finish` emits the
  `var` initializations before the prologue, so bindings the prologue fills from parameters (destructuring, rest,
  defaults) keep their values too. Repro (plain, captured, destructured, rest, default params; `var x;` in a loop;
  top level) = node: fixture `closures/var-param-redeclare.js`. EarleyBoyer (`fixed 0.05`): done, as node (14.5 s
  vs node 0.45 s: speed is separate).
- 2026-10-05: tests: `scripts/diff.sh` closures/, classes/, objects/, generators/, async/, commonjs/, modules/: 82
  pass; test262 `language/statements/{variable,function}`, `language/expressions/{function,arrow-function}`:
  0 regressions. Full run on a-03 next.
