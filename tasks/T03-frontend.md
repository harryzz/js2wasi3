# T03: Frontend: oxc parse + semantic

Status: **done** (2026-09-25) | Depends on: T02

## Goal
`frontend::parse(path, source)` returns a checked ES module (AST + scope/symbol info) or
diagnostics with file:line:col, using oxc 0.151.

## Read first
`docs/design.md` (pipeline table), `crates/frontend/src/lib.rs`, oxc_parser/oxc_semantic docs for 0.151.

## In scope
- Parse as an ES module (`SourceType::mjs`; `.ts` stays for later). Run `SemanticBuilder` and report its errors.
- Decide how the AST's lifetime works (oxc arena allocator): keep the `Allocator` inside `Program`, or have callers own it. Document the choice in the crate docs.
- A CLI subcommand `js2wasi3 parse <file>` prints the top-level statements and exported names, or the diagnostics.

## Out of scope
Imports of other files (single-file only for now), TypeScript, lowering.

## Acceptance
- `cargo test -p js2wasi3-frontend`: at least 1 test for valid `add.js` (export `add` found) and 1 syntax error with the right line and column.
- `js2wasi3 parse tests/fixtures/add.js` lists `export add`.

## Checklist
- [x] Parse as ES module (`SourceType::mjs`) + `SemanticBuilder` with syntax-error checks
- [x] Lifetime decision documented in the crate docs (the caller owns the `Allocator`)
- [x] `js2wasi3 parse <file>` prints top-level statements and exports, or diagnostics
- [x] Tests: `add.js` export found; syntax error at the right line:col

## Notes / Result
**Changed**
- `crates/frontend/src/lib.rs`: `parse(&Allocator, path, source) -> Result<Program<'a>, Diagnostics>`.
  `Program` holds `ast`, `semantic` and `exports: Vec<Export {name, local, from, span}>` (from oxc's `ModuleRecord`, in source order).
  `Diagnostic` shows as `path:line:col: error: msg`. Columns are 1-based and count chars.
- `crates/js2wasi3/src/main.rs`: new `parse` subcommand. `build` uses the new API and now stops at `backend::emit` (T06).
- New dependencies: `oxc_*` 0.151.0 including `oxc_syntax` (added to the workspace pins).

**Commands run**
- `cargo test -p js2wasi3-frontend`: 6 passed (add.js exports, syntax error 2:13, semantic redeclaration 2:5,
  export forms incl. re-exports/star/default fn, default expression has no local, line:col counting chars).
- `js2wasi3 parse tests/fixtures/add.js` → `1:1 ExportDeclaration`, `1:8 export add (local add)`.
- `js2wasi3 parse bad.js` → `bad.js:2:13: error: Unexpected token`, exit 1.
- `cargo clippy --all-targets`: clean. `cargo fmt`: applied.

**oxc 0.151 facts worth knowing**
- `ParserReturn.fatal_error` (not `panicked`). An `export` with a declaration is `Statement::ExportDeclaration`
  (there is also `ExportFromDeclaration`). Don't rely on the `ExportNamedDeclaration` name.
- Multi-label diagnostics (e.g. a redeclaration) have no primary label, and the offending site is the last label. `convert` uses primary, else last.
- `export default <expr>` has `local: None`, and `export default function f` has `local: Some("f")`.

**Unsupported / deferred**
- TypeScript: always parsed as `.mjs`, so TS syntax becomes a syntax error for now.
- Imports of other files: re-exports are recorded (`from`) but not resolved. Lowering must reject them (T05).
- Secondary labels (e.g. "declared here") aren't shown yet. Add `notes` to `Diagnostic` if needed.

**Next smallest step:** T04 (IR v0), which doesn't depend on T03. Then T05 lowers `Program` into it.
