# T04: IR v0 design

Status: **done** (2026-09-25) | Depends on: T02

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

## Checklist
- [x] `Module`/`Function`/`Global`/`Export`, `FnFlags { is_async, is_generator }`
- [x] Types `F64`, `I32`, `Bool`, `JsVal` (`#[non_exhaustive]` so GC ref kinds can be added)
- [x] Structured statements with labelled `Block`/`Loop`, `Break`/`Continue`, `If`, `Return`, `Expr`, `Set`, `SetGlobal`
- [x] Expressions: consts, locals/globals, `Tee`, `Unary`/`Binary` (enum ops), `Convert`, `Cond`, `Call`
- [x] Placeholders `Try`, `Throw`, `Suspend`
- [x] Stable text printer + snapshot tests. One-line doc for every node kind in `lib.rs`

## Notes / Result
**Changed**
- `crates/ir/src/lib.rs`: IR types, the node-kind reference in the crate docs, and helpers (`Function::new/param/add_local/local`,
  `Module::add_function/function`, `Expr::{f64,i32,bool,local,unary,binary}`, `Expr::ty`).
- `crates/ir/src/print.rs`: `Display for Module`. Locals print as `name.N`, functions as `name#N`, globals as `@name`, labels as `LN`, and constants as `1.0` / `1_i32` / `true`.
- `crates/ir/src/tests.rs`: `add_prints` (the required hand-built `add(a,b)`), `all_kinds_print` (every node kind,
  written as the lowering of a `for` loop with `continue`), `expr_types`.
- `crates/js2wasi3/src/main.rs`: uses `Module::default()`.

**Design decisions (for T05/T06/T18)**
- A structured tree, not a CFG: it maps 1:1 to Wasm `block`/`loop`/`if`/`br`, so the backend needs no relooper.
- `Loop` doesn't repeat implicitly: the body must end with `Continue`, like Wasm `loop`. JS `for` lowers to
  `loop L0 { if !test break L0; block L1 { body } update; continue L0 }`, and JS `continue` becomes `break L1`.
- Ops are machine-typed (`f64.add`, `i32.shr_u`, ...). JS-semantics operators on `jsval` become new nodes in T08.
- Locals are numbered and flat per function, params first, so the T18 liveness pass is cheap.
- `finally` must be lowered before it reaches the IR, so `Try` has only a catch.

**Commands run**
- `cargo test -p js2wasi3-ir`: 3 passed. `cargo test` (workspace): all pass. `cargo clippy --all-targets`: clean.

**Deferred (follow-ups)**
- An IR validator (label scoping, `Continue` only to a `Loop`, id ranges, type agreement) would help T05/T06 catch bugs early.
  Suggest adding it at the start of T06.
- Statement-level source positions: only `Function.pos` exists. Add them when T05 needs per-statement diagnostics.
- Static data segments (strings) are left for T09.

**Next smallest step:** T05, lowering the numeric subset from `frontend::Program` to this IR.
