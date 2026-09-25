# T14: Functions as values, closures and `this`

Status: done | Depends on: T13

## Goal
Function objects (a GC struct holding a funcref and an environment), captured variables (boxing only the captured ones),
arrow functions, `this` in methods, `call/apply`, `arguments` (non-strict objects stay out), a call via `call_ref`, and TypeError when calling a non-function.

## Acceptance
- [x] Fixtures `closures/*.js` (counters, loops capturing `let` per iteration, methods, arrows) match node.

## Handoff (written 2026-09-25 at the end of the T12–T13 session)

**State:** `main` at `c3f4dd2` (T13), clean tree. T01–T13 done. Exceptions (T12) and objects (T13) work; functions
are still only top-level declarations called directly by name.

**Run first (baseline, ~1 min):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'      # all ok, 56 tests in total
scripts/diff.sh                              # 21 passed, 0 failed, 8 skipped
```
New fixtures go in `tests/fixtures/closures/` (the default `diff.sh` run includes subdirectories except `unsupported/`).
Check them with `scripts/diff.sh tests/fixtures/closures`. A program that ends with an uncaught exception needs an
empty `x.uncaught` next to it (`docs/design.md`, "Testing").

**Where things are today:**
- Lowering (`crates/lower/src/lib.rs`) rejects, all with "planned: T14": nested `function` declarations (`stmt()`),
  function/arrow expressions (`expr()`), a function name used as a value (`resolve()`), and any call whose callee isn't a
  top-level function name (`call()`: `o.m()`, `f()()`, `o?.m()`, F41). Methods in object literals are rejected in
  `literal_property()`. `call()` also rejects more arguments than parameters (F16).
- Functions are typed: each has `f64`/`jsval` params and result from the inference (`Slot::Var`, `Slot::Result`, the
  `Facts` fixpoint in `lower()`). A direct call converts its arguments to the callee's parameter types (`store`).
- Commands: top-level variables are **globals**, so a function using one needs no capture. Only locals of a function
  (and of `main` for blocks, F28) would need capturing. TDZ checks (`Var.guard`, `tdz_flags`) exist for top-level
  `let`/`const`: keep them working when those are read from closures.
- Semantic analysis now builds AST nodes (`with_build_nodes(true)` in the frontend), so
  `scoping.get_resolved_references(sym)` → `semantic.nodes().get_node(id)` gives every use of a symbol: capture analysis
  can find the symbols used from a function other than the one declaring them.
- IR (`crates/ir/src/lib.rs`): `Expr::Call { func: FuncId, args }` only; there is no function-reference expression and no
  indirect call. `FnFlags` exists (async/generator). `ir::may_throw` treats unknown callees as not throwing: an indirect
  call must count as "may throw".
- Backend: runtime types are one `rec` group, all `is_final: true` (`runtime::types`): `$number, $oddball, $string,
  $i64s, $object, $keys, $vals, $bytes, $i32s` (TYPES = 9). `typeof` has a `Function` code but nothing returns it yet.
  Objects: `crates/backend/src/object.rs` (layout in its module docs).
- Built-in methods are sentinels (`G_BUILTIN`) that trap when read (`builtin_not_implemented`); ToPrimitive traps when
  a user replaced `toString`/`valueOf` (`user_to_primitive_not_implemented`, F37). With callable values these can call.

**Decisions to make first (log them in Progress):**
1. **Function object layout.** Functions are objects (properties, `prototype`, `instanceof Object`). Either make
   `$function` a struct subtype of `$object` (then `$object` must not be final, and `ref.test $object` also matches
   functions: check every helper that tests for objects) or a separate type that the object helpers accept too.
2. **Calling convention for indirect calls.** Direct calls keep the typed signatures. An indirect `call_ref` needs one
   uniform type, e.g. `(env, this: jsval, args...: jsval) -> jsval` with a fixed arity or an args array (arrays are
   T15). Suggestion: each function that becomes a value gets a small adapter with the uniform type that converts to
   the typed function; missing arguments are `undefined`, extra ones are dropped (F16).
3. **Captured variables.** Box only captured locals (a `(struct (mut T))` "cell" per variable, or one environment
   struct per scope); a `let` in a loop body needs a fresh cell per iteration (acceptance fixture). Decide how the
   inference (`f64` vs `jsval` slots) and the cells fit together.
4. **`this`.** `o.m()` passes `o`; a plain call passes `undefined` (strict mode); arrows take the enclosing `this`.
   Scope says `call`/`apply`, which need `Function.prototype` (a new `Builtin`?) and, for `apply`, arrays (T15):
   decide what to defer.

**Pitfalls seen so far:**
- `ref.func` needs the function declared in an element section (a declarative `elem` segment), or validation fails.
- Wasm function indices are offset by the 15 host imports in commands (`func_base`); new core functions and adapters
  must go through `func_base` / `helper_index`, not raw IR ids.
- `try_table` blocks take no values from outside: push call arguments inside it (T12 bug in the export adapter).
- An `if let (a, b) = (x, y.pop())` evaluates both sides (T13 bug): don't pop in a tuple.
- `wasm-tools validate --features all` on `js2wasi3 build --core` output points to the failing function; `wasm-tools
  print` + the name section shows its body.
- Remember `CLAUDE.md`: set `in progress`, append to `## Progress` after every step, update Follow-ups at the end
  (F16, F28, F37, F41 are the ones to pick up here).

## Notes / Result
**Done 2026-09-25.** Functions are values: nested declarations, function expressions, arrows and object-literal
methods; closures with captured variables; `this`; calls through values; `Function.prototype.call`.

What changed:
- IR (`crates/ir`): `Expr::Closure(Closure)`, `Expr::CallRef { callee, this, args, what }`, intrinsics
  `NewCell(ty)/NewCellUninit/CellGet(ty)/CellSet(ty)/CellIsUninit/EnvGet(i)/ArgGet(i)`, `Builtin::Function`.
  `may_throw`: a `CallRef` may throw.
- Backend: `$object` is non-final; new rec-group types `$code`, `$function` (subtype of `$object`), `$cell_f64`,
  `$cell` (TYPES = 13). `crates/backend/src/func.rs`: `$new_function` (own `length`, `name`, and `prototype` for
  `function`s), `$call_value` (TypeError "`x` is not a function" with node's text), `$arg_get`,
  `Function.prototype.call` (real function; `apply`/`bind`/`toString` stay sentinels). `typeof` → `"function"`,
  `console.log(f)` → `[Function: f]` / `[Function (anonymous)]`. Shared (top-level) function objects: one lazy global
  each. Declarative element segment for `ref.func`.
- Lowering: `crates/lower/src/closures.rs` (analysis) and, in `lib.rs`, `declare_nested`, `lower_body`, `closure`,
  `func_value`, `adapter`, `hoist`, `direct_call`, `value_args`, `chain_call`, `declare_store`, `nested_vars` (F28).
  Design decisions: see the Progress entry of 2026-09-25 (layout, calling convention, cells, `this`).
- Tests: fixtures `tests/fixtures/closures/{counter,loops,methods,values,scopes,tdz,uncaught,lib}.js`;
  `closures` in `crates/lower/tests/unsupported.rs` (other tests there updated: T14 constructs now lower);
  `closures_in_a_library` in `crates/js2wasi3/tests/build.rs`.

Commands run: `cargo test` (58 passed), `scripts/diff.sh` (28 passed, 0 failed, 9 skipped),
`wasm-tools validate` + `wasmtime run -Sp3` on `closures/methods.js`, `cargo clippy --all-targets` (clean).

Unsupported / deferred (new Follow-ups F42–F48 in `tasks/README.md`; F16, F28, F41 done, F37 partly): `arguments` (T15), `apply`/`bind`/
`Function.prototype.toString` (sentinels, trap when read), `new` of user functions and `instanceof` with them (T16),
user `toString`/`valueOf` in ToPrimitive (still trap, F37), assigning to a function declaration's binding, a
function with a computed key in a literal, a closure in a `for (let …)` head capturing a variable of that head
before it's declared. Functions' `name`/`length` are writable (read-only attributes: F36). `Function.prototype` is a
plain object (`typeof` gives `"object"`).

Next smallest step: T15 (arrays and iteration); `arguments` and `Function.prototype.apply` come with arrays.

## Progress
- 2026-09-25: started. Tools ok; baseline `cargo test` 56 ok, `scripts/diff.sh` 21 passed / 8 skipped.
- 2026-09-25: design decisions (read IR, lowering, backend runtime/object first):
  1. **Layout**: `$object` becomes non-final; `$function` is a final subtype of it with two extra fields
     `code: (ref $code)` and `env: (ref null $vals)`, so every object helper accepts functions unchanged. `typeof`
     tests `$function` first. `$code = (func (param eqref eqref eqref) (result eqref))` (self, this, args) is in the
     runtime `rec` group (TYPES 9 → 11). Own properties `length`, `name` (non-enumerable) and, for non-arrow
     non-method functions, `prototype` (`{constructor: f}`) are defined when the object is made.
  2. **Calls**: direct calls stay typed (`Expr::Call`). A function used as a value gets an IR adapter
     `f$call(self, this, args) -> jsval` (the `$code` type) calling the typed function; its params are observed `Any`.
     An indirect call is `Expr::CallRef { callee, this, args, what }`: args go in a `$vals` array, a runtime helper
     checks `ref.test $function` (else TypeError "<what> is not a function", node's text) and does `call_ref`.
     Missing args are `undefined`, extra ones are evaluated and dropped (F16).
  3. **Captures**: flat closures with one mutable cell per captured variable (`$cell_f64` / `$cell` structs, cell
     type from the inference), carried as `jsval` in the IR (`Intrinsic::NewCell/CellGet/CellSet`). The env is a
     `$vals` array of cells (or plain values for `this` and a function expression's own name, which never change).
     A nested function's typed signature is `(self?, this?, params…)`: `self` is the function object (its env).
     Cells: params at entry, `var` at function entry (one cell per function), `let`/`const` at the declaration
     (so a loop body gets a fresh cell each iteration); `for (let …)` copies the cell before each update. A
     `let`/`const` captured by a closure created before its declaration ends gets an uninitialized cell at block
     start and TDZ checks in closures (the variable becomes `jsval`).
  4. **`this`**: a function using `this` gets a `this` param; direct calls pass `undefined`, `o.m()` passes `o`, arrows
     capture it by value; module top level `this` is `undefined`. `Function.prototype.call` is a real function
     (runtime code); `apply`/`bind`/`arguments` need arrays → deferred to T15.
- 2026-09-25: IR: `Expr::Closure(Box<Closure>)`, `Expr::CallRef`, intrinsics `NewCell/NewCellUninit/CellGet/CellSet/
  CellIsUninit/EnvGet/ArgGet`, `Builtin::Function` (validator, printer, `may_throw`: `CallRef` always may throw).
  Backend: rec group now 13 types (`$code`, `$function` ⊂ `$object` non-final, `$cell_f64`, `$cell`); new
  `crates/backend/src/func.rs` (`new_function`, `call_value`, `arg_get`, `Function.prototype.call`); `$code`-shaped
  functions use `T_CODE`; shared closures get one lazy global each; `ref.func` targets go in a declarative elem
  segment; `typeof` → "function"; `console.log(f)` → `[Function: f]`. Existing tests unchanged: cargo test ok, diff 21/8.
  Next: the lowering (capture analysis, closures, calls).
- 2026-09-25: lowering part 1: `crates/lower/src/closures.rs` (analysis: function tree, `this` users, captures per
  function, cell variables, TDZ-risk `let`s, self names, function declarations used as values). `declare_nested`
  declares every non-top-level function up front in source order (stable ids across rounds); `lower_body` is shared
  by top-level and nested functions (env unpack, param/`var` cells); `closure`, `func_value`, `adapter`, `hoist`.
  Builds; not wired into expressions/calls yet.
- 2026-09-25: calls: `direct_call` (typed; `self`/`this` hidden args; extra args evaluated then dropped, F16),
  `CallRef` for values and `o.m()` (this = `o`), node's "x is not a function" text (`callee_text`). Fixed a
  nullable `local.tee` in `new_function`. First fixture `closures/counter.js` passes (counters, shared captures,
  nested declarations, arrows, `console.log` of functions).
- 2026-09-25: optional calls in chains (`chain_call`), object-literal methods (named after the key; computed keys
  rejected), `for (let …)` per-iteration cell copies, exports with a `this` param (wrapper passes `undefined`), F22
  flags also for closures made in top-level code, F28 (`nested_vars`: nested top-level `var`s are globals).
  Updated `crates/lower/tests/unsupported.rs` (T14 constructs now lower; new `closures` test with T14's rejections).
- 2026-09-25: fixtures `closures/{counter,loops,methods,values,scopes,tdz,uncaught}.js` all match node (diff: 28
  passed / 8 skipped overall). A declaration used only as a callee no longer gets `Any` params when its object is
  made for its environment (recursive `go` stays `f64`). Library test `closures_in_a_library` (build.rs) with
  `closures/lib.js`.
- 2026-09-25: task done; user approved the commit ("T14: closures, function values, `this`"), no attribution lines,
  not pushed. Next: T15 in a new session.
