# T16: Classes

Status: done | Depends on: T15

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
**Done 2026-09-26.** Classes and property attributes.

What changed:
- IR: `FnKind` on `Closure` (Plain / Ordinary / Class { construct }), `Expr::New`, intrinsics ConstructList,
  CreateThis, ClassHeritage, ClassSetup, InstanceOfValue, DefineMethod, DefineField, DefineAccessor, SuperGet,
  SuperSet, Hole, IsHole, CtorReturn, DefineProperty, GetOwnPropertyDescriptor, Integrity, TestIntegrity,
  ErrorCtor, PrivateGet/Set/Has/Add.
- Backend: rec group of 17 types (`$accessor`; `$function` + `construct`, `kind`; `class` field mutable);
  `crates/backend/src/props.rs` (accessors, OrdinarySet, attributes, defineProperty/descriptors, integrity,
  super get/set, private members); `func.rs` (construct, ordinary construct, class heritage/setup,
  instanceof, constructor results, native error constructors as values); object.rs (get/set/delete honor
  accessors and attributes; ToPrimitive with hint and user methods; real `Error.prototype.toString`);
  arrays honor sealed/frozen; `inspect.rs` prints classes, instances, null prototypes, accessors and functions'
  own properties like node; uncaught errors use node's `improveStack` naming.
- Lowering: `crates/lower/src/classes.rs`; closure analysis treats a class as a function (constructor +
  instance fields), `Cap::Home`, `Cap::Private`, `Cap::PrivMethod`; `lower_body` split into `prologue`/`finish`.
- Tests: fixtures `tests/fixtures/classes/{basics,fields,errors,errors2,properties,inspect,private,scopes}.js`;
  `classes` in `crates/lower/tests/unsupported.rs` (older T16 rejections updated); trap test cases in
  `crates/js2wasi3/tests/build.rs` moved to still-unimplemented features.

Commands run: `cargo test` (60 passed), `scripts/diff.sh` (40 passed, 0 failed, 9 skipped), `cargo clippy
--all-targets` (clean), `cargo fmt`, `wasm-tools validate` + `wasmtime run -Sp3` on `classes/private.js`.

Unsupported / deferred (Follow-ups F57–F63; F36, F44, F48 done; F37, F47, F52 partly): `new.target` outside class
constructors, `super()` in arrows, private accessors, static blocks, `accessor` fields, computed instance field
keys, computed keys with function values, array element attributes, circular `console.log`, `may_throw` with user
ToPrimitive, a class used before its declaration in the same function.

Next smallest step: T17 (promises + microtask queue).

## Progress
- 2026-09-25: started. Tools ok; baseline `cargo test` 59 ok, `scripts/diff.sh` 32 passed / 9 skipped.
- 2026-09-25: design decisions (after reading closures.rs, lib.rs, func.rs, object.rs, runtime.rs):
  1. **Construct convention.** `$function` gets a third field `construct: (ref null $code)`, called as
     `(self, new_target, args) -> jsval`; null means "not a constructor" (arrows, methods, accessors). Ordinary
     `function`s get a runtime `$code` (`$construct_ordinary`: object with proto `new_target.prototype`, or
     `Object.prototype` if that isn't an object; call `code` with it as `this`; an object result wins).
     A class's `code` throws "Class constructor X cannot be invoked without 'new'"; its `construct` is a
     generated adapter calling the typed constructor. `new F(…)` is a new `Expr::New { callee, new_target,
     args, what }` (a `$vals` like `CallRef`) → `$construct` ("x is not a constructor" otherwise).
     `new.target` only in class constructors (a hidden param); elsewhere rejected for now.
  2. **Class layout.** A class is a `$function` (typeof "function", printed `[class X]`/`[class X extends Y]`
     via a kind field in `$function`), `prototype` non-writable/non-enumerable/non-configurable, methods and
     accessors non-enumerable on it, static members on the function. `extends`: the heritage value is checked
     at run time (node's messages) and sets both chains; `extends null` works too. The class *is* a function in
     the closure analysis: the constructor's `Function` scope and the instance-field initializers are mapped to
     the class scope (one pseudo-function: `self` = the class, `this` = the instance), so field initializers
     capture like any code in the constructor. Instance fields are defined (not set) at the start of a base
     constructor / right after each `super(…)` returns. Implicit constructors are synthesized (derived: forwards
     all arguments). Static fields: evaluated where the class is defined, `this` = the class.
     `instanceof` with any value: a generic `InstanceOfValue` (node's TypeErrors); `InstanceOf(Builtin)` stays.
  3. **`super`.** `super(…)` = `$construct(Object.getPrototypeOf(self), args, new_target)`, then bind `this`
     (a local that is Wasm null until then; reads check it: node's ReferenceErrors) and run the field
     initializers. `super.x` / `super.m()` read from the home object's prototype with the current `this` as
     receiver: the home object is `self.prototype` in the constructor/field initializers, and a new capture
     `Cap::Home` (by value, like `this`) for methods and arrows inside them. `super()` in an arrow and
     `super.x = v` are rejected for now.
  4. **Accessors and attributes (F36).** Attribute bits per entry: 1 not enumerable, 2 read-only, 4 not
     configurable, 8 accessor (the value slot holds a new `$accessor { get, set }` struct, never a JS value).
     Non-extensible objects: a bit in the (now mutable) `class` field; frozen arrays: another bit checked by
     the element store. Get/Set/Delete follow OrdinaryGet/OrdinarySet/delete with node's strict-mode messages
     (`#<Ctor>` from the `constructor` found on the chain). `Object.defineProperty`, `getOwnPropertyDescriptor`,
     `freeze`/`seal`/`preventExtensions`/`isFrozen`/`isSealed`/`isExtensible`. Built-in function properties
     get their real attributes (`name`/`length` read-only, F47 in part).
  5. **User `toString`/`valueOf` (F37)**: ToPrimitive calls them (hint string/number/default) and returns any
     primitive; "Cannot convert object to primitive value" otherwise.
  6. **Private names (`#x`)**: not in the first pass; kept rejected, done at the end if time allows
     (plan: one private `$symbol` per name per class evaluation, captured like `Cap::Home`).
- 2026-09-25: runtime part done (not used by the lowering yet): `$accessor` type (rec group 17 types), `$function`
  gets `construct` + `kind` fields, `class` field mutable; new `crates/backend/src/props.rs` (Value, OrdinarySet,
  constructor name / `#<Name>`, Put, DefineAccessor, defineProperty, getOwnPropertyDescriptor, preventExtensions/
  seal/freeze + tests, super_get); `func.rs`: construct / construct_same / construct_list / construct_ordinary /
  create_this / class_heritage / class_setup / instanceof_value / ctor_return; object.rs: get calls getters, set is
  OrdinarySet, delete checks non-configurable, object spread calls getters, ToPrimitive takes a hint and calls user
  `valueOf`/`toString` (`==`, `+`, ToString, ToNumber updated); arrays check sealed/frozen/non-extensible.
  IR: `FnKind` on closures, `Expr::New`, intrinsics ConstructList/CreateThis/ClassHeritage/ClassSetup/InstanceOfValue/
  DefineMethod/DefineField/DefineAccessor/SuperGet/Hole/IsHole/CtorReturn/DefineProperty/GetOwnPropertyDescriptor/
  Integrity/TestIntegrity. `cargo test` ok (the trap test's user-`toString` case now uses `Function.prototype.toString`),
  diff 32/0/9. Next: lowering.
- 2026-09-25: lowering: `crates/lower/src/classes.rs` (class_value, constructor lowering, construct/call adapters,
  super(), super.x, fields, `new`, `this` checks); closure analysis: classes as pseudo-functions (`fn_at`,
  `ctor_class`, field ranges), `Cap::Home`, `new.target`, rejections collected in `Analysis::unsupported`;
  `lower_body` split into `prologue` + `finish` (shared with class constructors); class declarations are
  let-like (TDZ cells, command globals); `instanceof` with values; `Object.defineProperty`/…/`isFrozen`;
  getters/setters in literals. First fixture `classes/basics.js` passes (super.x also needs `this`: fixed in
  the analysis).
- 2026-09-25: fixtures `classes/{basics,fields,errors,properties,inspect}.js` match node. `console.log`: classes
  (`[class A extends B]`), instances (`A { x: 1 }`, node's constructor-name rule incl. the instanceof check),
  null prototypes, accessors (`[Getter/Setter]`), functions/classes with own properties (F48); empty objects/arrays
  aren't abbreviated past depth 2 (node). `callee_text` of function/class literals is `(intermediate value)`.
  Tests: `classes` in `crates/lower/tests/unsupported.rs`; old T16 rejections updated (`new Map` / `instanceof Map`
  now fail on the global `Map`); the trap test's inspect case uses a symbol key. cargo test 60 ok, diff 37/0/9.
  Next: `extends Error` (native error constructors as values), then private names.
- 2026-09-25: native error constructors are values (`Intrinsic::ErrorCtor`, lazy globals `G_ERROR_CTORS`, call +
  construct codes honoring `new.target`; each error prototype's `constructor` is set), so `class X extends Error`
  works; `Error.prototype.toString` is a real function; uncaught errors are described like node's `improveStack`
  (`NotFound [ValidationError]: msg`; string helpers `StrAt`/`StrSlice`). Fixture `classes/errors2.js` (+ `.uncaught`).
  cargo test 60 ok, diff 38/0/9. Next: private names (`#x`, `#m()`, `#x in o`, static privates).
- 2026-09-26: private names (fields, methods, statics, `#x in o`, brand errors like node), `super.x = v`,
  `super` in literal methods; fixtures `classes/{private,scopes}.js`. Docs, Notes / Result, README status and
  follow-ups F57–F63. cargo test 60 ok, diff 40/0/9, clippy clean. Task done.
- 2026-09-26: user approved the commit ("T16: classes", 48c863a), no attribution lines, not pushed. Next: T17.
