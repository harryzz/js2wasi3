# T16: Classes

Status: todo | Depends on: T15

## Goal
`class` with constructor, methods, static members, fields, `extends`, `super()` and `super.m()`, `new.target`, `instanceof`.
Also F36: getters/setters (in classes and object literals, where the lowering rejects them today), `Object.defineProperty`,
read-only/non-configurable attributes, `Object.freeze`/`seal` (accessor entries in `crates/backend/src/object.rs`).

## Acceptance
- Fixtures `classes/*.js` match node.

## Handoff (written 2026-09-25 at the end of the T14–T15 session)

**State:** `main` at `031e632` (T15), clean tree. T01–T15 done: functions are values (T14: closures, cells, `this`,
`call`/`apply`), arrays/symbols/iteration/destructuring (T15). Classes are still rejected (a `class` statement falls
to "statement `…` (planned: later)"; `new X()` of anything but a native error is "`new` (only the native error
constructors so far)" at `crates/lower/src/lib.rs` ~2507; getters/setters in literals are rejected in
`literal_property`, "getter or setter (planned: T16)").

**Run first (baseline, ~1 min):**
```sh
scripts/check-tools.sh
cargo test -q 2>&1 | grep 'test result'      # all ok, 59 tests in total
scripts/diff.sh                              # 32 passed, 0 failed, 9 skipped
```
New fixtures go in `tests/fixtures/classes/`; check them with `scripts/diff.sh tests/fixtures/classes`. A program
ending with an uncaught exception needs an empty `x.uncaught` next to it. `console.log` of a class instance traps
today (`inspect.rs` only prints plain objects: prototype must be `Object.prototype`) — node prints `ClassName { … }`;
decide whether to add that (small: the constructor's `name` from the prototype's `constructor`) or avoid it in fixtures.

**Read first:** `docs/design.md` (D1 notes for T13–T15), T14's and T15's "Notes / Result" and Progress (the design
decisions are logged there), then:
- `crates/lower/src/closures.rs` — capture analysis (function tree, `this` users, captures, TDZ cells, self names).
  Class methods / constructors are `Function` nodes whose parent is a `MethodDefinition`; `declare_nested` skips them
  today (search "Class members and accessors"), and `analyze` does not treat `super`/`new.target`.
- `crates/lower/src/lib.rs` — `declare_nested`, `lower_body` (hidden params: `Hidden { self_, this }`), `closure`,
  `adapter` (the `$code` entry `(self, this, args) -> jsval`), `direct_call`, `call`/`spread_call`, `hoist`.
- `crates/lower/src/patterns.rs` — destructuring, `for-of` (reuses the `Finally` machinery).
- `crates/backend/src/func.rs` — `$function` ⊂ `$object`: `code`, `env`; `new_function` gives own `length`, `name` and,
  when `ctor` is 1, a `prototype` object whose `constructor` is the function (`Closure.constructor` in the IR).
- `crates/backend/src/object.rs` — dictionary objects; every own read goes through `OwnGet`, writes through `Define`
  (`attrs` byte per entry, only bit 0 = not enumerable today). This is where accessors (F36) go: e.g. an attribute bit
  "accessor" whose value slot holds a getter/setter pair object; `lookup`/`get`/`set` must call them with `this` =
  the receiver (use `func::Helper::CallValue`). Arrays (`array.rs`) handle indices/`length` before the dictionary.

**Decisions to make first (log them in Progress):**
1. **`new` / construct calling convention.** Today a call passes `this`. `new F(...)` needs: create the object with
   prototype `F.prototype` (read at run time), call with `this` = it, use the result if it's an object. For classes:
   derived constructors create `this` via `super(...)` (TDZ on `this` before it), `new.target` must be passed. Options:
   a hidden `new_target` param (only for functions that use it / derived constructors), or a separate construct entry
   in `$function`. Calling a class without `new` must throw "Class constructor X cannot be invoked without 'new'".
2. **Class layout.** A class is a `$function` (typeof "function") with `prototype` holding the methods (non-enumerable),
   static members on the function itself, `extends`: prototype chain of both the prototypes and the constructors.
   Instance fields: initialized at the start of the constructor (base) or after `super()` returns (derived).
   `instanceof` with a user function (F44): walk the chain against `F.prototype` (a generic `InstanceOf` intrinsic taking
   the right-hand value, not a `Builtin`).
3. **`super.m()` / `super.x`.** Needs the home object: capture the class prototype (or its parent) like `this` is
   captured by value in T14 (a `Cap::Value`-like environment entry), then `GetProp(parent, key)` called with the
   current `this`.
4. **Accessors and attributes (F36)** — see above; decide read-only semantics (strict-mode TypeError messages from node:
   "Cannot assign to read only property 'x' of object '#<Object>'").
5. **Private names (`#x`)**: in scope for T16? If not, keep rejecting with a clear planned task.

**Pitfalls seen so far:**
- oxc: arrow bodies are `ArrowFunctionBody` (use `FnAst::statements()/expression()`), `FormalParameters.rest` is
  `FormalParameterRest { rest: BindingRestElement }`; `AssignmentTargetMaybeDefault::as_assignment_target()` returns
  `Option`.
- `cargo fmt` reflows code: Python patch scripts that match multi-line text break after it — re-read (or use Edit).
- Every new runtime type goes in the one `rec` group (`runtime::types`, `TYPES`), subtypes need a non-final parent.
  `ref.func` targets must be declared (`Cx::declare_func`). A struct result stored with `local.tee` into a nullable
  local needs `ref.as_non_null` before passing it on.
- Symbols are `$string`s: use `runtime::is_string` (not `ref.test $string`) when a test must exclude symbols.
- The fixpoint lowering re-runs rounds: FuncIds of JS functions must be declared up front in source order
  (`declare_nested`); adapters may be created lazily.
- Remember `CLAUDE.md`: set `in progress`, append to `## Progress` after every step, update Follow-ups at the end
  (F36, F44 and F52 are the ones to pick up here; F37's user `valueOf`/`toString` is listed for T16 too).

## Notes / Result
