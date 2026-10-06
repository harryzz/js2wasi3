# T88: EarleyBoyer's wrong result (F333)

Status: done (2026-10-06) | Depends on: — | Local

## Goal
Octane EarleyBoyer (`tests/bench/patched/earley-boyer.js` with the Octane harness, `scripts/bench.sh`) computes a
wrong result under js2wasi3 (`TypeError: Cannot read properties of undefined (reading 'cdr')`; node: done). Find the
miscompile, fix it, add a fixture.

## Notes / Result
- Cause: the lowering set every `var`-declared local to `undefined` at function entry, parameters too, so
  `function(x) { var x; … }` (scheme2js output) lost its argument. Fix in `crates/lower/src/lib.rs` (parameters
  skipped; `var` initializations before the prologue, for destructured/rest/default parameters). Fixture
  `closures/var-param-redeclare.js`. EarleyBoyer computes node's result.
- Full test262 on a-03 (with T89): 0 regressions (HISTORY.md 2026-10-06).
- Profile of the now-correct EarleyBoyer: F368, F369 (→ T89), F350 (→ T89), F359. F333 ticked.
- Next: T89 (done), then F368.

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
- 2026-10-06: profile of the now-correct EarleyBoyer (laptop, perf, `fixed 0.05`, 14.5 s vs node 0.45 s): property
  access ~55%: adds (`this.car = …` in constructors) search the object and the whole prototype chain each time
  (`ordinary_set_named` → `find_own_named` 12.5%, `append_named` 2.8%; F368); cached reads still cost a helper call
  (`get_named` self 18.7%; F359); `instanceof`/`new` read `F.prototype` through the generic `get` (~4–5%; F369);
  wasmtime libcalls ~12% (F350). GC < 1%. User: F369 + F350 next (T89); the full test262 run covers T88 and T89.
- 2026-10-06: full test262 on a-03 (`~/t262-t89.log`): 28,099 pass, 0 regressions; replay `t79/out-t89`: 22.07 s. Done.
