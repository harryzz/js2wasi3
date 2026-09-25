# T03: Frontend: oxc parse + semantic

Status: todo | Depends on: T02

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

## Notes / Result
