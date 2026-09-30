# T51: Tagged templates and `String.raw` (F78)

Status: done (2026-09-30) | Depends on: T14 (calls through values), T21 (prelude),
T32 (array attributes, freeze), T35 (`CallList`)

## Goal
``tag`a${x}b` `` as in node 24.21.0: the tag is called with the template object and the substitutions (`this` from a
member callee: ``o.m`x` ``), the template object is a frozen array of the cooked strings (`undefined` for an invalid
escape, which is allowed only in tagged templates) with a frozen, non-enumerable `raw` array of the raw strings
(TRV: CR and CRLF become LF), the same object every time the same site is evaluated (per site, not per text), and
`String.raw`. Programs without a tagged template compile to the same bytes.

## What we know (at handoff)
- **Today**: `TaggedTemplateExpression` is "unsupported: expression" (`crates/lower/src/lib.rs`, the catch-all).
  Untagged templates lower in `Lowerer::template` (cooked strings, `Intrinsic::Concat`).
- **Model to follow**: `Lowerer::spread_call`: a member callee's object goes to a `this` local, the callee is read
  as a property, and the call is `Intrinsic::CallList(f, this, args array, callee text)`.
- **Freezing arrays** and a non-enumerable `raw` property work already (`Object.freeze`, `Object.defineProperty`).
- **test262** (subset, not passing at handoff): `language/expressions/tagged-template` 22 unsupported,
  `template-literal` 14 unsupported, `built-ins/String/raw` 26 unsupported + 4 fail; a few more use a tagged
  template in passing.

## Design questions (decide first, record in docs/design.md D5 or a new note)
1. **Lowering vs a source rewrite**: lower ``tag`…` `` like `spread_call` (callee → `f`/`this`, then
   `CallList(f, this, [templateObject, ...substitutions])`) rather than rewriting text as `frontend::using` does
   (spans stay exact, `Function.prototype.toString` shows the original text). Leaning lowering.
2. **Template object**: a prelude function `$$templateObject(site, cooked, raw)` (GetTemplateObject: arrays with
   read-only elements, `raw` non-enumerable, both frozen) caching by a per-site number in a prelude array; linked only
   when the source has a tagged template. The cooked/raw array literals are built on every evaluation (cheap); a
   check of the cache in the IR first is a later optimisation if a benchmark wants it.
3. `super.m`…`` / ``o.#m`…` `` / optional chains (a SyntaxError) as tags: `super` and private tags can stay
   "unsupported" if rare (check test262 counts).

## Plan
1. Lowering + `$$templateObject` + `String.raw` (`$$String_raw` in `builtins/string.js`); fixture
   `tests/fixtures/syntax/tagged-templates.js` (or the right fixture directory; node is the oracle): `this`, caching
   per site (a loop, two sites with the same text), invalid escapes, raw CR/CRLF, frozen, `String.raw` with
   substitutions, a tag returning a function (``f`a``b` ``), `new tag`x``.
2. test262: the three directories plus the subset tests with a tagged template (targeted `--update`, a-03).
3. Docs (design.md, keywords.md, README, HISTORY.md), F78 ticked, follow-ups.

## Read first
- `crates/lower/src/lib.rs`: `template`, `spread_call`, `spread_args`, how prelude functions are called with IR
  arguments (`direct_call_this`), `builtins::Prelude::new`'s `extra` names.
- `docs/design.md` D5 (prelude).
- F78 in `tasks/README.md`.

## Tests
Fixture via `scripts/diff.sh`, `cargo test`, `wasm-tools validate` + wasmtime. Gating: old vs new build of every
fixture byte-identical except those with a tagged template (none at handoff). test262: targeted `--update` on a-03.

## Acceptance
- [x] Tagged templates and `String.raw` behave like node's (fixture).
- [x] The test262 template directories pass or have named follow-ups; 0 regressions; baseline updated.
- [x] Programs without a tagged template compile to the same bytes.

## Notes / Result
- **What changed**: `Lowerer::tagged_template` (`crates/lower/src/lib.rs`): the tag and its `this` as in
  `spread_call`, then `CallList(tag, this, [templateObject, ...substitutions], callee text)`; the template object is
  a direct call of the prelude's `$$templateObject(site, cooked, raw)` (`builtins/string.js`: cached per site number
  in a prelude array; `raw` defined non-enumerable, both arrays frozen), rooted through `extra` only when the user's
  source has a tagged template. Cooked strings of invalid escapes are `undefined`, raw strings get TRV. `String.raw`
  is `$$String_raw`. `callee_text` prints a tagged call as V8 does (`tag(...)`).
- **Gating**: every fixture builds to the same bytes except the new `tests/fixtures/syntax/tagged-templates.js`.
- **test262** (a-03, 1,247 tests, 78 s): 68 more pass (all 30 `String/raw`, 21 of 22 `tagged-template`, all 14
  `template-literal`, 3 elsewhere), 0 regressions; `cache-eval-inner-function.js` fails (F226). Subset 27,519 of
  28,591 run (96.25%).
- **Unsupported**: `super.m`…`` and ``o.#m`…` `` tags (none in test262's subset); the template's arrays are built on
  every evaluation before the cache check (cheap; a cache check in the IR if a benchmark wants it).
- **Commands**: `scripts/diff.sh tests/fixtures/syntax/tagged-templates.js`, `cargo test`, scratchpad `cmp.sh`,
  `node scripts/test262.mjs --update --list <1,247 tests>` on a-03.
- **Next smallest step**: F226 needs run-time eval (F191); otherwise pick from the Follow-ups table.

## Checklist
- [x] design questions decided and recorded
- [x] lowering, template object, `String.raw`
- [x] fixture, gating check, test262 + baseline
- [x] docs (HISTORY.md, keywords.md, design.md, README), follow-ups

## Progress
- 2026-09-30: handoff written after T50. State: `main` at `b8d3393`, pushed; test262 subset 27,451 of 28,591 run
  pass (96.0%).
- 2026-09-30: decisions: Q1 lowering (`Lowerer::tagged_template`, like `spread_call`: member tag → `this` local, then
  `CallList(f, this, [templateObject, ...substitutions], callee text)`). Q2 `$$templateObject(site, cooked, raw)` in
  `builtins/string.js`, cached in a prelude array by the lowering's site number, linked through `extra` when the
  user's source has a tagged template (`builtins::TEMPLATE_OBJECT`); raw strings get TRV (CR/CRLF → LF), invalid
  escapes cook to `undefined`. `String.raw` is `$$String_raw`. `callee_text` prints a tagged call as V8 does
  (`tag(...)`). Fixture `tests/fixtures/syntax/tagged-templates.js` passes `diff.sh`.
- 2026-09-30: gating: only the new fixture changes. a-03 targeted run (1,247 tests, 1 min 44 s with the build): 68 more
  pass, 0 regressions, `cache-eval-inner-function.js` fails (eval compiled once: F226). Docs, follow-ups; task done.
