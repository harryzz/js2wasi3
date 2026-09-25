# T05: Lower the numeric subset from AST to IR

Status: **done** (2026-09-25) | Depends on: T03, T04

## Goal
Lower real JS (via the frontend) into IR for number-only programs: `function` declarations, params, `let/const`,
arithmetic and comparisons, `if/while/for`, `return`, direct calls, `export function`.

## In scope
- For this task every value is `f64`. Anything else (strings, objects, closures, `this`, ...) gives an
  "unsupported: <construct> at file:line:col" diagnostic. Never miscompile silently.
- `js2wasi3 ir <file>` prints the IR.

## Out of scope
Dynamic values (T08), recursion into other modules, and optimisations.

## Acceptance
- `tests/fixtures/{add.js, fib.js, loops.js}` lower, and their IR snapshots match.
- A fixture that uses a string literal fails with a precise "unsupported" diagnostic.

## Progress
- 2026-09-25: started. T03 (frontend) and T04 (IR) are done. Checking the oxc 0.151 AST/scoping API for the lowering.
- 2026-09-25: API checked (oxc 0.151: `ExportDeclaration` = `export <decl>`, `ExportNamedDeclaration` = `export { a }`, `ExportFromDeclaration` = re-export;
  references resolve via `semantic.scoping().get_reference(id).symbol_id()`).
- 2026-09-25: decision: the lowering lives in a **new crate `crates/lower`** (frontend + ir). This keeps `crates/ir` free of oxc types
  (the promise in its crate docs). `docs/design.md` updated. Variables map by oxc `SymbolId` → `LocalId`/`GlobalId`, so shadowing is handled by semantic analysis, not by names.
- 2026-09-25: decision: semantics rules for this subset (no silent miscompile):
  comparisons/`!`/`&&`/`||` produce `Bool`, which may only be used as a condition (using it as a value is unsupported until T08);
  a function must end in a provable `return <expr>` or never return a value (then it's void);
  declarations without an initializer, postfix `++` used as a value, `**`, `>>>`, calls with the wrong arity, nested functions, strings, and non-literal top-level initializers are all
  unsupported with a diagnostic naming the construct and the task that will add it.
- 2026-09-25: `crates/lower/src/lib.rs` written (~750 lines) and compiling. Added `UnOp::Abs` to the IR (number truthiness is `abs(x) > 0`, one evaluation)
  and documented that `Convert F64→I32` means JS `ToInt32`. `frontend::statement_kind` is now public. Next: CLI `ir` subcommand, fixtures, snapshot tests.
- 2026-09-25: `js2wasi3 ir <file>` works and `build` now calls the lowering. Fixtures `fib.js` and `loops.js` (for+continue, labelled break,
  `while(true)`, do-while+continue, bitwise, ternary, `&&`/`||` as values, globals, a void helper) are added; their IR was checked by hand and saved as `*.ir` snapshots
  (`crates/lower/tests/snapshots.rs`, regenerate with `UPDATE_SNAPSHOTS=1`). `unsupported/string.js` → `string.js:2:13: error: unsupported: string (planned: T09)`. 2 tests pass.
  Next: unit tests for each rejection rule.
- 2026-09-25: 8 rejection-rule tests added (`crates/lower/tests/unsupported.rs`), all passing on the first run. clippy flagged `to_*` naming,
  so the helpers were renamed `number_of`/`cond_of`. Workspace: fmt + clippy clean, all tests pass. `js2wasi3 build fib.js` now stops at `backend::emit` (T06). Task closed.

## Checklist
- [x] Lower function declarations, params, `let/const/var`, arithmetic, comparisons, `if/while/do/for`, labels, `break/continue`, `return`, direct calls, `export function`
- [x] Every value is `f64`; everything else gets an "unsupported: <construct> (planned: <task>)" diagnostic at file:line:col
- [x] `js2wasi3 ir <file>` prints the IR
- [x] `add.js`, `fib.js`, `loops.js` lower, and their IR snapshots match
- [x] A string literal fails with a precise diagnostic

## Notes / Result
**Changed**
- New crate `crates/lower` (`lower(&Program) -> Result<ir::Module, Diagnostics>`), about 750 lines. The crate docs list the supported subset.
- `crates/ir`: `UnOp::Abs`. `Convert F64→I32` is documented as JS `ToInt32` (T06 must implement the modulo semantics, not a saturating truncation).
- `crates/frontend`: `statement_kind` is public.
- CLI: `js2wasi3 ir <file>`. `build` runs parse → lower → (backend stub).
- Fixtures: `tests/fixtures/{fib,loops}.js` + `{add,fib,loops}.ir` snapshots, and `tests/fixtures/unsupported/string.js`.
- `docs/design.md`: the crate table now has `lower`, and `ir` no longer depends on oxc.

**How the lowering works (for T06/T08)**
- Variables map by oxc `SymbolId`, so shadowing and block scopes come from semantic analysis. Top-level functions and globals are declared in pass 1, so call order doesn't matter.
- `Bool` exists only as a condition: comparisons, `!`, `true/false`, and `&&`/`||`/`?:` with a boolean operand produce `Bool`, and using one as a value is rejected (T08).
- Number `&&`/`||` evaluate the left side once via a `tmp` local (`cond.f64(abs(tmp := a) > 0, ...)`). `a ?? b` on numbers lowers to `a`.
- `for`/`do-while` get a `block` for `continue` only when a `continue` targets them. `while(true)` has no exit test.
- A function returns `f64` iff it contains `return <expr>`. Then every path must provably return (`return`, a block ending in one, or `if/else` where both do).

**Commands run**
- `cargo test -p js2wasi3-lower`: 10 passed (2 snapshots/fixtures + 8 rule tests). Workspace `cargo test`: all pass. `cargo clippy --all-targets`: clean.
- `js2wasi3 ir tests/fixtures/loops.js` matches `loops.ir`. `js2wasi3 ir tests/fixtures/unsupported/string.js` → `...string.js:2:13: error: unsupported: string (planned: T09)`, exit 1.

**Unsupported / deferred** (also in the Follow-ups table): F8–F11 below, plus everything the crate docs don't list (strings, objects, closures, ...), which are the planned tasks.

**Next smallest step:** T06, the backend (IR → core Wasm). Start with F1 (the IR validator), then emit `fib.js` and run it in wasmtime.
