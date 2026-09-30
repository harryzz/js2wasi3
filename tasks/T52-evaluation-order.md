# T52: Evaluation order: member compound assignment, destructuring IteratorClose

Status: done (2026-09-30) | Depends on: T13 (members), T15 (destructuring, iterators),
T24 (compound/logical assignment)

## Goal
Two ordering gaps that silently give wrong results (no "unsupported"):
1. `base[prop] op= expr` / `base[prop]++`: a `null`/`undefined` base throws its TypeError before the key is converted
   (no `toString` call) and before the right side runs; the key is converted once (GetValue then PutValue share
   it). Plain `base[prop] = v` and reads throw for the base before converting an object key too.
2. Array destructuring (assignment and `for-of` heads): IteratorClose when the pattern finishes early or a target
   throws, not when the iterator itself threw; `next` read once.

## What we know (at handoff)
- test262 (subset): `compound-assignment` 33 fail (`S11.13.2_A7.*_T1/T2/T4`: null base vs key `toString`, key
  converted twice); `for-of/dstr` 25 fail + 7 unsupported, `assignment/dstr` 23 fail + 7 unsupported (`*-iter-rtrn-
  close`, `*-iter-thrw-close`, `*-trlg-iter-*`, `array-rest-iter-*`, "next is not a function").
- The runtime's get/set (`crates/backend/src/object.rs`) check the base first but built the TypeError's text with
  ToPropertyKey of the key (which runs an object's `toString`).
- Member assignment is `Lowerer::member_assign`/`member_update` (key evaluated into a local, converted by each
  `GetProp`/`SetProp`); destructuring assignment is in `crates/lower/src/patterns.rs`.

## Plan
1. Runtime: the nullish-base message names an object key without converting it (V8: `#<Object>`), a symbol as
   `Symbol(desc)`. Lowering: `key_once` (for compound/update on a computed, non-literal key: `null` base → the get
   throws; else the key is converted once).
2. Destructuring: IteratorClose per the spec (`iteratorRecord.[[Done]]` tracking), `next` read once.
3. Fixture (node is the oracle), gating check, test262 targeted `--update` on a-03, docs.

## Read first
- `crates/lower/src/lib.rs`: `member_assign`, `member_update`, `logical_assign`; `crates/lower/src/patterns.rs`.
- `crates/backend/src/object.rs`: `get`, `set`.

## Tests
Fixture via `scripts/diff.sh`, `cargo test`, `wasm-tools validate` + wasmtime. Gating: the runtime change touches
every program with a property get/set (expected); the lowering change only programs with `o[k] op= v`/`o[k]++`.
test262: targeted `--update` on a-03.

## Acceptance
- [x] The compound-assignment and destructuring IteratorClose tests pass or have named follow-ups; 0 regressions.
- [x] Fixture matches node.

## Notes / Result
- **What changed**: `crates/backend/src/object.rs`: `get`/`set` name the key of a `null`/`undefined` base without
  converting an object (`key_text`: `#<Class>`; a symbol `Symbol(desc)`). `crates/lower/src/patterns.rs`:
  `array_pattern` wraps the elements in the `finally` dispatch (IteratorClose on throws, returns and other jumps
  unless done), `array_elements` evaluates a member target's reference before the step (elements and rest); an
  undeclared identifier target is a `global_write`; an object pattern without named properties checks for
  `null`/`undefined`.
- **Not done, on purpose**: converting the key once in `o[k] op= v` (spec, test262 `S11.13.2_A7.*_T4`, 11 tests):
  node 24 converts it twice, and node is the oracle (F227).
- **test262**: full run on a-03: 107 more pass, 0 regressions; subset 27,626 of 28,591 run (96.6%).
- **Fixture**: `tests/fixtures/syntax/evaluation-order.js`; all 165 fixtures pass `diff.sh` (2 min locally).
- **Gating**: every program with a property get/set or an array pattern changes bytes (expected for these fixes).
- **Next smallest step**: F227's message details, or another follow-up.

## Checklist
- [x] runtime message, `key_once`
- [x] destructuring IteratorClose
- [x] fixture, test262 + baseline
- [x] docs (HISTORY.md, keywords.md, design.md, README), follow-ups

## Progress
- 2026-09-30: handoff written after T51 (`main` at `3adf980`; subset 27,519 of 28,591, 96.25%).
- 2026-09-30: part 1 done: `object.rs` `key_text` (an object key named `#<Class>` without conversion, a symbol
  `Symbol(desc)`; V8 leaves some objects out entirely: arrays, dates, regexps, objects with their own `toString`, not
  matched), `key_once` in `member_assign` (compound) and `member_update`. `language/expressions/compound-assignment`:
  388 of 388 run pass (the 33 fixed; whole directory 454 tests in 186 s locally).
- 2026-09-30: part 2: `array_pattern` uses the `finally` machinery (a throw, a generator `return()` at a `yield` in the
  pattern or another jump closes the iterator unless done); `array_elements` evaluates a member target's reference
  (object, key) before stepping, then the default, then stores (AssignmentElement's `lref` first; the rest element
  too, with [[Done]] set before `IterRest`). An undeclared identifier target is a `global_write` (ReferenceError at
  run time, not "unsupported: global"); an object pattern without named properties checks for `null`/`undefined`.
  a-03: `for-of/dstr` + `assignment/dstr` (937 tests, 43 s): 46 more pass, 0 regressions.
- 2026-09-30: node 24 converts the key twice in `o[k] op= v`/`o[k]++` (get, then set): the spec's once (`key_once`,
  test262 `S11.13.2_A7.*_T4`, 11 tests) disagrees with the oracle, so `key_once` is removed; the runtime's
  no-conversion message keeps the 22 T1/T2 tests passing. Fixture `tests/fixtures/syntax/evaluation-order.js` passes.
  Known message gap: a nested non-iterable pattern says "object is not iterable" (node: "undefined is not iterable
  (cannot read property Symbol(Symbol.iterator))").
- 2026-09-30: full subset run on a-03 (1,647 s): 107 more pass, 2 timeouts under load (pass alone, set back), 0
  regressions; subset 96.6%. Docs, follow-up F227; task done.
