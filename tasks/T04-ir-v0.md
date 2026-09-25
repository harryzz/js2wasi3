# T04: IR v0 design

Status: todo | Depends on: T02 (can run in parallel with T03)

## Goal
Define the IR data structures that every later task extends, plus a text printer for tests. No lowering yet.

## Read first
`docs/design.md` D1–D3. For comparison: Porffor's `compiler/ir.js` (a flat structured tree, a C-shaped IR), at
`git clone https://github.com/CanadaHonk/porffor` @ `de4eb588`. Read it for ideas only; don't copy it.

## In scope
- `Module { functions, globals, data?, exports }`, `Function { params, locals, body, flags(async, generator) }`.
- Value types: `f64`, `i32`, `bool`, `jsval` (dynamic), plus GC ref kinds added later.
- Structured statements: block, if, loop, break/continue with labels, return, expression statement.
- Expressions: constants, locals/globals, unary/binary ops as **enums** (not strings), direct calls.
- Placeholders for later tasks: `Try`, `Throw`, `Suspend` (await/yield). They can be declared but unimplemented.
- A stable text printer (`Display`) used for snapshot tests.

## Out of scope
AST lowering (T05), Wasm emission (T06).

## Acceptance
- `cargo test -p js2wasi3-ir`: build a small `add(a,b)` function by hand and match its printed text.
- A doc comment at the top of `crates/ir/src/lib.rs` explains every node kind in one line.

## Notes / Result
