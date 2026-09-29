# T43: `arguments` in arrow functions (F203)

Status: done (2026-09-29) | Depends on: T25 (`arguments`), T38 (captures of `new.target`), T42 (eval code is an arrow)

## Goal
An arrow function's `arguments` is the enclosing non-arrow function's `arguments` object (the same object), as in
node. Today it's rejected ("`arguments` in an arrow function"), which also blocks direct `eval` code using
`arguments` inside a function (T42 splices it as an arrow). Programs without it compile to the same bytes.

## Design
Like `new.target` (T38): the closure analysis finds the nearest non-arrow function of each `arguments` reference;
that function uses `arguments` (`uses_arguments`, lowered as `f(...args)` with a `MakeArguments` object, T25) and
each arrow in between captures it by value (`Cap::Arguments`). At the module level it stays an undeclared name;
class constructors/field initializers stay rejected (as before).

## Read first
- `crates/lower/src/closures.rs` (`arguments`, `new.target` captures), `crates/lower/src/lib.rs` (the prologue's
  captures and `cx.arguments`, closure environments).

## Tests
Fixture `tests/fixtures/closures/arrow-arguments.js` (node is the oracle), `cargo test`, gating (old vs new
build of every fixture), test262: tests naming `arguments` that were unsupported.

## Checklist
- [x] analysis + lowering
- [x] fixture, gating, cargo test
- [x] test262 targeted run + baseline, docs, follow-ups

## Notes / Result
- `closures.rs`: an `arguments` reference walks the arrow chain to the nearest non-arrow function, which gets
  `uses_arguments`; each arrow in between captures `Cap::Arguments` (by value). `lib.rs`: the prologue binds it to
  `cx.arguments`, `env_for` passes the enclosing `cx.arguments`. Module level: still an undeclared name; class
  constructors/fields: still rejected.
- Fixed an older miscompile: in a function using `arguments` (lowered as `f(...args)`, T25), parameters were bound
  from `args[i]` before `var_cells`, which then gave captured ones fresh `undefined` cells; they're now in
  `param_syms`.
- Tests: fixture `closures/arrow-arguments.js` (node matches; includes direct eval code with `arguments`),
  `unsupported.rs` (the arrow case now lowers), `cargo test` ok; gating: every other fixture builds byte-identical.
  test262: the 659 subset tests naming `arguments` with `--update`: 13 → pass, 0 regressions; subset 26,666 of
  28,296 run (94.2%).
- Left: `arguments` in class constructors/fields and with a rest parameter (unchanged, F103).
- Next smallest step: none in this area; next task to be chosen.

## Progress
- 2026-09-29: task created (user chose F203 after T42).
- 2026-09-29: analysis: `arguments` walks the arrow chain to the nearest non-arrow function (like `new.target`),
  arrows capture `Cap::Arguments` (prologue: `cx.arguments`; `env_for`). Found and fixed an older miscompile: in a
  function using `arguments`, parameters are bound from `args[i]` before `var_cells`, which gave captured ones fresh
  `undefined` cells (`function f(a) { arguments; return (() => a)(); }` gave `undefined`); now in `param_syms`.
- 2026-09-29: fixture matches node; gating clean; `cargo test` ok (the `unsupported.rs` arrow case now lowers).
  test262 (659 tests naming `arguments`, `--update`): 13 → pass, 0 regressions (4 load timeouts pass at -j 2).
  Docs: keywords.md, design.md (T38 note), README, HISTORY.md; F203 ticked. Task done.
