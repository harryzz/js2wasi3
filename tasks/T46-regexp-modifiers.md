# T46: RegExp modifier groups `(?ims-ims:…)`

Status: done (2026-09-29) | Depends on: T27 (RegExp engine), T39

## Goal
The ES2025 pattern modifiers `(?i:…)`, `(?-i:…)`, `(?ims-ims:…)` as in node 24 (V8): the `i`, `m`, `s` flags on or
off inside a group; node's early errors ("Repeated flag in flag group", "Multiple dashes in flag group", "Invalid
flag group"). test262: ~70 failing tests (feature `regexp-modifiers`, in `built-ins/RegExp`). Programs without a
modifier group compile to the same bytes.

## Design
- Parser: `(?` + `i`/`m`/`s`/`-` → `$$reModifiers`: AST node `[13, add, remove, body]`; `$$reIcase` follows `i`
  while the body parses (a `v` class folds then).
- Emitter: `MODS` (opcode 19) with the inner `i`/`m`/`s` bits before the body and the outer ones after it
  (`$$reModBits` tracks them; `$$reIcase` too, so characters and classes are stored folded as the VM expects).
- VM: `icase`/`multiline`/`dotAll` are `let`s; `MODS` sets them and pushes an undo entry `[-5, old, 0]` that
  backtracking restores. Groups are balanced, so a lookaround's body ends with the flags it started with (its
  barrier handling needs nothing). The search fast path never starts at a `MODS` (a pattern starting with one isn't
  skipped).
- Gating: all of it sits in `if (__intrinsic_regexp_mods()) { … }` blocks, kept only when the user's source contains
  `(?i`, `(?m`, `(?s` or `(?-` (`Prelude::regexp_mods`; the scan skips dropped blocks' references, like T40's
  `__intrinsic_bigint_mode`). A pattern built at run time without such text in the source: "Invalid group".

## Checklist
- [x] parser, emitter, VM, gating
- [x] fixture, gating check, cargo test
- [x] test262 + baseline, docs, follow-ups

## Notes / Result
- `crates/lower/builtins/regexp.js`: `$$reModifiers` (parser, node's early errors), node 13 in `$$reEmit`
  (`MODS` 19 around the body, `$$reModBits`/`$$reIcase` tracked), the VM's `MODS` and the `-5` undo entry;
  `crates/lower/src/builtins.rs` (`Prelude::regexp_mods`, dropped-block references skipped by the scan),
  `crates/lower/src/lib.rs` (`__intrinsic_regexp_mods()` like `__intrinsic_bigint_mode()`).
- Tests: fixture `builtins/regexp-modifiers.js` (node matches); `cargo test` ok; gating: every other fixture
  byte-identical (the RegExp ones included). test262: all 122 tests with modifier groups pass (67 were failing);
  subset 27,053 of 28,512 run (94.9%), 0 regressions.
- Left (F215): patterns built at run time without modifier text in the source; V8's `\u212a` case under `(?i:)`.
- Next smallest step: none here.

## Progress
- 2026-09-29: task created (user chose RegExp modifiers). Implemented as designed; fixture
  `builtins/regexp-modifiers.js` matches node except `/(?i:\u212a)/u.test("k")` (node: false, though
  `/\u212a/iu.test("k")` and `/(?i:K)/u.test("\u212a")` are true: a V8 inconsistency; js2wasi3 follows the spec's
  folding, true). Case dropped from the fixture.
- 2026-09-29: gating clean (only the new fixture differs), `cargo test` ok; test262 `--update` on the 122 tests
  with modifier groups: all pass (6 load timeouts pass at -j 2). Docs: design.md (T27 note), keywords.md,
  HISTORY.md; follow-up F215. Task done.
