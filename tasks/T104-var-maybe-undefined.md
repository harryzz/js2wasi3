# T104: a `var` read before any write is `undefined` (F276)

Status: done (2026-10-11) | Depends on: T103 | Local

## Goal
A miscompile since T63 (F276): an uncaptured function-local `var` with an initializer that a read can reach before
the initializer ran reads Wasm's default instead of `undefined`: `0` when it's an `f64`, a cast-failure trap when
it's a string or another typed local:
```js
function f(c) { if (c) { var x = 1 } return x }        // f(false): node undefined, js2wasi3 0
function g(c) { if (c) { var s = 'a' + c } return s }  // g(false): node undefined, js2wasi3 traps
function h(n) { for (var i = 0; i < n; i++) { var last = i * 1.5 } return last }  // h(0)
```
T103's definite-assignment pass (`crates/lower/src/definite.rs`) already finds the other side: a `var` not proven
written before every read is exactly one that must start as `undefined` (and so is a `jsval`).

## Design
- `definite::analyze` also returns the candidates it couldn't prove (`Analysis::maybe_undefined`).
- `var_decl`: a plain local in `maybe_undefined` declared with an initializer (or a pattern) observes `undefined`
  for its slot and starts as `undefined` at function entry (`var_undefined`), like `var x;` does today.
- Precision matters more now (an unproven `var x = e` becomes a `jsval`): `do`-`while` without `break`/`continue`
  in its body evaluates its test from the body's end state.
- Check that no benchmark local changes type (IR before/after) beyond the ones a read before the write can reach.

## Read first
- `crates/lower/src/definite.rs`, `var_decl` in `crates/lower/src/lib.rs`, T103's notes, F398.

## Plan
- [x] `maybe_undefined`; `var_decl` (identifier and pattern declarators); command globals checked (top-level `var`).
- [x] `do`-`while` precision.
- [x] Fixture `tests/fixtures/syntax/var-maybe-undefined.js` (the cases above, patterns, `while`+`break`, `do`-`while`,
  a top-level `var` in a branch) = node.
- [x] IR before/after of the 18 benchmarks and the scriptc-comparison programs: list every local whose type changed.
- [x] `scripts/diff.sh` on the touched dirs; test262 subsets; full run on a-03 (ask); a-03 benchmarks if any local
  of a benchmark changed type.

## Acceptance
- The examples print what node prints; every touched fixture = node; test262 0 regressions; no benchmark slower.

## Notes / Result
**What changed** (`crates/lower` only): `definite::analyze` also returns the candidates it couldn't prove
(`Analysis::maybe_undefined`); `Lowerer::var_maybe_undefined` makes such a `var` with an initializer (an
identifier or any name of a pattern) observe `Any` and start as `undefined` at function entry (`var_undefined`), as
`var x;` already did. A `do`-`while` whose body has no `break`/`continue` tests from the body's end state.

**Result.** F276 fixed: `function f(c) { if (c) { var x = 1 } return x }` gives `undefined` for `f(false)` (was `0`);
a string-typed one no longer traps. No benchmark changes: the IR of the 32 programs (18 benchmarks + the scriptc
workloads, prelude included) has no local of another type, and differs in 3 `= undefined` lines.

**Tests.** `tests/fixtures/syntax/var-maybe-undefined.js` = node; `scripts/diff.sh` on `syntax/`, `exceptions/`,
`closures/`, `generators/`, `async/`, `classes/`, `objects/`, `loops.js`, `semantics.js`: all pass; `cargo test -p
js2wasi3-lower` ok; full test262 on a-03 (`~/t262-t104.log`, 1,297 s): no changes since the baseline (28,105).

**Not done.** F398's conservative spots (now they can also make a `var x = e` a `jsval`; no benchmark shows one;
`do`-`while` is done).

**Next smallest step.** None here. Speed: F372 (write guess in `set_named`) or F391 (unboxed number arrays).

## Progress
- 2026-10-11: task created (user: "Next up", after T103 suggested F276). Reproduced: `f(false)` = `0`, a
  string-typed one traps (cast failure). A top-level `var` in a branch: not checked yet (the program trapped first).
- 2026-10-11: implemented: `definite::analyze` returns `(definite, maybe_undefined)`; `Analysis::maybe_undefined`;
  `Lowerer::var_maybe_undefined` (observe `Any`, start as `undefined` via `var_undefined`) for `var` declarators
  with an initializer and for every name of a `var` pattern; `do`-`while` whose body has no `break`/`continue`
  tests from the body's end. Top-level `var` in a branch was right already (globals start as `undefined`).
  Fixture `tests/fixtures/syntax/var-maybe-undefined.js` (14 functions + a top-level case) = node.
  IR before/after (HEAD's compiler vs T104) of the 32 programs of `~/t104/a` (the 18 benchmarks + the scriptc
  workloads), prelude included: **no local changes type**; the whole IR differs in 3 lines: two `tmp = undefined`
  added in earley-boyer (a `var tmp` a read can reach first; it was a `jsval` already) and one `key = undefined`
  gone in splay (the `do`-`while` rule). So no a-03 benchmark run. `scripts/diff.sh` on `syntax/`, `exceptions/`,
  `closures/`, `generators/`, `async/`, `classes/`, `objects/`, `loops.js`, `semantics.js`: all pass;
  `cargo test -p js2wasi3-lower` ok.
- 2026-10-11: full test262 on a-03 started (user: full run instead of local subsets; `~/t262-t104.log`, `--update --causes 25 subset`).
- 2026-10-11: full test262 on a-03 (1,297 s, -j 36): no changes since the baseline; `baseline.json` copied back
  (identical). Docs updated. Done.
