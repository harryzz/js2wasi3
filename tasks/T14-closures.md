# T14: Functions as values, closures and `this`

Status: todo | Depends on: T13

## Goal
Function objects (a GC struct holding a funcref and an environment), captured variables (boxing only the captured ones),
arrow functions, `this` in methods, `call/apply`, `arguments` (non-strict objects stay out), a call via `call_ref`, and TypeError when calling a non-function.

## Acceptance
- Fixtures `closures/*.js` (counters, loops capturing `let` per iteration, methods, arrows) match node.

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
