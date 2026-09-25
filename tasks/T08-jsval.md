# T08: JS values on WasmGC

Status: done | Depends on: T07

## Goal
Implement D1's `jsval` (`(ref null eq)`) and the dynamic semantics needed by untyped code:
numbers (i31 fast path + boxed f64), `undefined`, `null`, booleans, `typeof`, `===`, `==` (primitives only),
JS `+ - * / %` and comparisons on numbers, ToBoolean, ToNumber (primitives).

## In scope
- The GC type section layout (a single `rec` group) and a list of runtime helpers emitted by the backend.
- Type inference stays trivial: `f64` where obviously numeric, otherwise `jsval`. Record the remaining work in Notes.
- The boundary converts `f64` ↔ `jsval`.

## Out of scope
Strings (T09), objects (T13).

## Acceptance
- Fixtures `values.js` (typeof table, equality table, truthiness) return the expected numbers through an f64 export.
- The core module validates and uses `i31ref` for small integers (visible in `wasm-tools print`).

## Progress
- 2026-09-25: started. Baseline green (T07 commit d740447). GC is on by default in wasmtime 49 (no flags).
- 2026-09-25: design.
  - **IR**: `Const::{Undefined, Null}` (type `jsval`); `Convert` gains F64↔JsVal (box / ToNumber) and Bool↔JsVal (select singleton / ToBoolean);
    `Binary{Eq,Ne}` on `jsval` = `===`/`!==`; new `BinOp::{LooseEq, LooseNe}` (jsval only) = `==`/`!=`;
    `Expr::TypeofIs { arg, tag }` → bool (the `typeof x === "..."` pattern; typeof as a string value waits for T09);
    `Expr::Seq { body, value }` (statements then a value: postfix `++` as a value, `void`, comma, assignment to a global as a value).
  - **Backend**: one `rec` group `$number (struct f64)`, `$oddball (struct i32 kind)` (0 undefined, 1 null, 2 false, 3 true);
    four immutable singleton globals before the user globals; helpers `box`, `to_number`, `to_boolean`, `is_number`, `strict_eq`,
    `loose_eq`, `typeof` (a code compared with the tag). Numbers: `i31ref` when integral, in [-2^30, 2^30) and not -0, otherwise `$number`.
    GC types, globals and helpers are only emitted when the module uses `jsval`, so number-only modules don't change.
  - **Type inference (lowering)**: optimistic `f64` for every variable, param and function result. Lowering runs, and whenever it would store
    a non-`f64` value into an `f64` slot (a non-number assignment, a declaration without init, a missing argument, `return;` or falling off the end
    next to value returns, the result of a void function used as a value), that slot is **demoted to `jsval`** and lowering runs again,
    until nothing changes. The lowering itself is the type rule, so inference and codegen can't disagree.
  - **Operators**: `- * / % < <= > >=`, unary `- +` and `+` go through ToNumber (exact for primitives; T09 adds string `+`/compare).
    `&&`/`||`/`?:` with operands of different types produce `jsval`; in a condition context they combine truthiness (no boxing).
  - **Boundary**: an export whose params/result aren't all `f64` gets an IR wrapper `name$export` (f64 params → box, jsval result → ToNumber).
  - F9 (`>>>`): `f64(x >>> y as i32)`, plus 2^32 if negative, via a temp. No new IR conversion needed.
- 2026-09-25: IR done (`Const::{Undefined,Null}`, `TypeofTag`, `TypeofIs`, `Seq`, `LooseEq/LooseNe`, jsval conversions; validator + printer).
  Backend done: `crates/backend/src/runtime.rs` (rec group, singletons, 8 helpers); const numbers are boxed at compile time
  (`i32.const; ref.i31`). Hand-built IR test `jsval_runtime_runs` passes 10/10 checks for 2.5 (`$number`) and 7 (i31). Next: lowering.
- 2026-09-25: lowering done: demotion fixpoint (`Slot::{Var, Result}`), `Val` of type f64/bool/jsval with `number_of`/`jsval_of`/`cond_of`,
  `cond_expr` for condition contexts, `typeof x === "..."` (folded when the type is static), `==`/`===` via `equality()`,
  `??` on jsval, `void`, comma, postfix values, jsval globals (`let g;`, `true`, `null` initializers), missing args, F9 `>>>`/`>>>=`,
  export wrappers `<name>$export`. Rewrote `crates/lower/tests/unsupported.rs` for the T08 rules (`primitive_values_lower` lists what now works).
- 2026-09-25: `tests/fixtures/values.js` + `values.out` (node). All 7 exports match node on the first run. Types come out as intended
  (loop counters and accumulators stay f64; `val` returns jsval; `second(a: f64, b: jsval)` from a missing argument).
  `wasm-tools print`: rec group, 4 singleton globals, `ref.i31`. Spot-check of 12 small programs vs node: 11 identical, the 12th a deliberate
  `**` rejection, which showed a cascading "used before its declaration" error. Fixed: a variable is declared even when its initializer fails.
- 2026-09-25: found that wasmtime 49 `--invoke` can't call exports whose WIT name is a keyword (`bool`, `use`, ...), even written `%bool` → F15.
- 2026-09-25: IR validator test for the new nodes; `cargo fmt`, `cargo clippy --all-targets` clean, full `cargo test` ok.

## Notes / Result

**Done.** JS primitive values (numbers, booleans, `undefined`, `null`) on WasmGC, with a trivial optimistic type inference.

What changed:
- `crates/ir`: `Const::{Undefined, Null}`, `TypeofTag`, `Expr::{TypeofIs, Seq}`, `BinOp::{LooseEq, LooseNe}`, jsval conversions
  (F64↔JsVal, Bool↔JsVal); validator and printer updated.
- `crates/backend/src/runtime.rs` (new): `rec` group `$number (struct f64)` / `$oddball (struct i32)`, singleton globals 0–3, helpers
  `box_number`, `to_number`, `to_boolean`, `is_number`, `is_nullish`, `strict_eq`, `loose_eq`, `typeof`. Emitted only if the module uses jsval;
  number-only modules are unchanged. Constant numbers are boxed at compile time (`i32.const n; ref.i31`).
- `crates/lower`: optimistic inference (see the crate docs), `cond_expr`, all F10 implicit-`undefined` cases, F9 `>>>`, export wrappers.
- Tests: `backend/tests/emit.rs` `jsval_runtime_runs` (hand-built IR, 10 checks × i31/boxed) and `values_use_i31ref`;
  `js2wasi3/tests/build.rs` `values_match_node` (7 exports vs `values.out`); `lower/tests/unsupported.rs` updated; IR validator test.

Commands run: `cargo test -q` (all ok), `cargo clippy -q --all-targets` (clean), `wasm-tools validate values.wasm`,
`wasmtime run --invoke 'misc(3)' values.wasm` → 16383 (= node).

Type inference: what it does and what's left.
- A slot (variable, param, result) is `f64` iff every value stored into it is an `f64` expression; otherwise `jsval`. Arithmetic results
  are `f64` even on jsval operands (ToNumber), so `let x = a - b` stays `f64` even when `a` is jsval.
- Not done: `bool`-typed slots (a boolean stored in a variable is boxed as jsval) → F14; int32 range analysis (avoid f64 round trips) → F8/F14.

Unsupported / limits:
- `typeof x` as a value (it's a string) → T09 (F17). `+` and `<` on strings → T09 (the ToNumber lowering is only right for non-strings).
- Calls with extra arguments → F16. `&&=`, `||=`, `??=` → F18. `**` → T21.
- The WIT boundary is still `f64` only: jsval params/results go through ToNumber/box in the `$export` wrapper (F13).

Next smallest step: T09. Add a `$string` type (`array i16`) to the rec group, extend `to_number`/`to_boolean`/`strict_eq`/`loose_eq`/`typeof`
with a string case, and add dynamic `+` (a new IR op or a helper call on jsval) with string concatenation.
