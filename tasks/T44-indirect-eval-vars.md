# T44: `var` and function declarations in indirect eval code (F205)

Status: done (2026-09-29) | Depends on: T42 (global object, indirect eval of literals)

## Goal
Indirect eval code (`(0, eval)("…")`, `globalThis.eval("…")`, `eval?.("…")`) is global code: its top-level `var`
and function declarations are global-object properties (EvalDeclarationInstantiation with the global
environment), as in node. T42 rejected them at build time.

## Design
In `frontend::eval` (global mode), rewrite the code before splicing:
- every `var` declaration (also in blocks and loops, not in nested functions) becomes assignments
  (`var x = 1, y;` → `void (x = (1));`), so its names are undeclared in the spliced code and go through the global
  object (T42); `for (var k in o)` targets stay rejected (undeclared `for-in` targets are, F154's note);
- top-level function declarations are removed and become named function expressions in a prologue;
- the prologue does the spec's checks first (CanDeclareGlobalFunction/Var: node's TypeErrors "Identifier 'x' has
  already been declared", "Cannot add property x, object is not extensible"), then CreateGlobalFunctionBinding
  (define, or Set for a non-configurable one) and CreateGlobalVarBinding (define `undefined` if absent), all
  configurable (eval code bindings are deletable);
- completion values are unchanged (declarations stay empty).
Function declarations nested in blocks (sloppy Annex B) stay rejected.

## Tests
Fixture `tests/fixtures/globals/indirect-eval-vars.js`; `cargo test`; gating; test262 `language/eval-code/indirect`
and subset tests naming `eval`.

## Checklist
- [x] rewrite + prologue
- [x] fixture, gating, cargo test
- [x] test262 targeted run + baseline, docs, follow-ups

## Notes / Result
- `crates/frontend/src/eval.rs`: `Decls` (non-strict indirect eval code only: a `"use strict"` directive keeps the
  code's own variable environment) rewrites `var` declarations into assignments and moves top-level function
  declarations into a prologue that runs EvalDeclarationInstantiation's global steps with node's TypeError texts;
  the eval edits are now replacements `(start, end, rank, text)` (insertions have `end == start`).
- Tests: fixture `globals/indirect-eval-vars.js` (hoisting, blocks and loops, several declarators, completion
  values, existing and non-configurable globals, deletable, other indirect forms, strict code, a non-extensible
  global object) matches node; `cargo test` ok; gating: every other fixture byte-identical.
- test262 (`--update`): `language/eval-code/indirect` 32 of 59 run (was 21), 0 regressions; subset 26,677 of 28,296
  run (94.3%). What's left there: script-only tests (top-level `this`, script `var`s named from eval code: F206),
  `eval` through a variable (F191), block-level functions (Annex B, F204).
- Still rejected: `for (var … in/of …)` (undeclared `for-in` targets), destructuring `var`s, function declarations
  in blocks.
- Next smallest step: none here; the next task is to be chosen.

## Progress
- 2026-09-29: task created (user chose F205 after T43). node checked: `var NaN` is fine, `function NaN(){}` throws
  "Identifier 'NaN' has already been declared"; a non-extensible global object: "Cannot add property x, object is
  not extensible".
- 2026-09-29: `eval.rs`: `Decls` (global mode) rewrites `var` declarations to `void (x = (init), …);` (also
  `for (var …;;)` inits; `for (var … in/of …)`, destructuring `var`s and block-level functions rejected), removes
  top-level function declarations into a prologue (`((g, fs, vs) => { checks; defines })(globalThis, […], […])`,
  node's TypeError texts); edits are now replacements `(start, end, rank, text)`. Fixture
  `globals/indirect-eval-vars.js` matches node on the first build.
- 2026-09-29: strict indirect eval code (a `"use strict"` directive) keeps its declarations (test262
  `var-env-func-strict.js`); gating clean, `cargo test` ok; test262 `--update` run: indirect 32/59, 0 regressions,
  subset 94.3%. Docs: keywords.md, design.md D6, HISTORY.md; F205 ticked. Task done.
