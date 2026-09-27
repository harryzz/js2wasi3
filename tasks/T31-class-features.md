# T31: class features (private accessors, computed instance keys, `static {}`, `o?.#x`)

Status: todo (proposed) | Depends on: T16 (classes), T28

## Goal
The class syntax still rejected by the lowering works like node (F58, except `accessor` fields, which node 24 doesn't
support). It is the largest test262 cause left after T30. A rerun of the non-passing class tests on 2026-09-27
(after T30, 1,387 tests with the `Array` failures, reasons in `out/test262/results.tsv` at that time) counted:

| reason (build rejected: `unsupported: …`) | tests | where |
|---|---|---|
| private getter or setter (`get #x()`, `set #x(v)`, static too) | 313 | `crates/lower/src/classes.rs` `class_method` |
| computed instance field key (`[k] = v` in a class body) | 229 | `class_value` |
| class `static` block (`static { … }`) | 17 | `class_value` |
| private field in an optional chain (`o?.#x`, `o?.a.#x`) | 12 | `classes.rs` ~l. 638, `lib.rs` ~l. 4432 |

About 570 tests, mostly `language/{statements,expressions}/class/elements` and `…/class/cpn-*`. Many of them also use
other features (`eval`: 175 more class tests, `Proxy`, `Reflect`, `Function`, subclassing built-ins: F106), so some
will move to another reason instead of passing; the rerun counts that.

## What node does (the behaviour to match)
- **Private accessors**: `get #x()` / `set #x(v)` (instance or `static`) are installed like private methods (brand:
  every instance gets the private name when the constructor runs, before the fields); `this.#x` calls the getter,
  `this.#x = v` the setter. A getter without a setter: `this.#x = v` throws TypeError "'#x' was defined without a
  setter"; a setter without a getter: reading throws "'#x' was defined without a getter" (node 24.21.0, checked).
  Wrong brand: the messages we already have for private fields/methods (a static one: "Receiver must be an
  instance of class A").
  `#x in o` is true for accessors too. Compound (`this.#x += 1`), `++`, destructuring targets (`[this.#x] = [1]`)
  go through get/set.
- **Computed instance field keys**: `class A { [k] = v }`: the key expression is evaluated **once, at class
  evaluation**, in order with the other computed keys (methods, static fields), ToPropertyKey'd (symbols allowed);
  each instance then gets `CreateDataPropertyOrThrow(this, key, value)` when its fields are defined. The
  initializer runs per instance with `this` = the instance. Anonymous functions get the key as their name.
- **`static { … }`**: runs once during class evaluation, in order with the static fields, with `this` = the class
  and `super.x` from the parent class; `var`s are local to the block; `await`/`arguments` are early errors (oxc).
  `return` isn't allowed.
- **`o?.#x`**: `undefined` if `o` is nullish, else a private read (same for `o?.a.#x`, `o?.#m()`).
- `accessor x = 1` (decorators proposal): node 24 rejects it (SyntaxError), so test262 tests using it are skipped
  by feature (`decorators`); keep rejecting it.

## Plan (slices)
1. **Private accessors**: an `$accessor` value with the `PRIVATE | ACCESSOR` attributes (T16's representation of
   accessors) added per instance like private methods (`Intrinsic::PrivateAdd` with a getter/setter pair: static
   ones on the class), `private_get`/`private_set` resolve accessors (call the getter/setter; node's messages when
   one is missing). A getter and setter pair of the same name share one entry. Fixture first
   (`tests/fixtures/classes/private-accessors.js`); `diff.sh`.
2. **Computed instance keys**: evaluate the keys into locals during class evaluation (in source order with the
   other computed keys), capture them for the constructor (the closure analysis keys instance fields by the class
   scope: add the key locals as captured values, like the private names), define with `DefineField` and the key.
   Function names from the key (`SetFunctionName`). Fixture.
3. **`static {}` blocks and `o?.#x`**: a static block is lowered as a static-field-like thunk run in order with
   `this` = the class (a function scope of its own, so its `var`s stay local); `o?.#x` in the optional-chain lowering
   (short-circuit, then `PrivateGet`). Fixtures.
4. **test262**: rerun the non-passing tests under `language/{statements,expressions}/class` (~1,000, ~5 min at
   -j 4) with `--update`; new causes → follow-ups; `tests/test262/HISTORY.md`.

## Read first
- F58 in `tasks/README.md`; T16's Notes / Result (classes, private names, accessors and attributes).
- `crates/lower/src/classes.rs` (`class_value`, `class_method`, `field`, private names), `crates/lower/src/closures.rs`
  (captures keyed by the class scope), the optional-chain lowering in `crates/lower/src/lib.rs`,
  `crates/backend/src/props.rs` (`private_add`, `private_get`/`private_set`, `$accessor`, `DefineAccessor`).

## Tests
One fixture per slice in `tests/fixtures/classes/` (`scripts/diff.sh`), `cargo test` (the `unsupported.rs` cases
for these constructs change: they now lower), `wasm-tools validate` + one wasmtime run. test262 as in slice 4.

## Checklist
- [ ] Slice 1: private getters/setters
- [ ] Slice 2: computed instance field keys
- [ ] Slice 3: `static {}` blocks, `o?.#x`
- [ ] Slice 4: test262 rerun, follow-ups, baseline

## Notes / Result

## Progress
- 2026-09-27: task written as a proposal after T30 (largest remaining test262 cause, ~570 tests). Not started.
