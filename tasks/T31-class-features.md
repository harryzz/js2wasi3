# T31: class features (private accessors, computed instance keys, `static {}`, `o?.#x`)

Status: done | Depends on: T16 (classes), T28

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
- [x] Slice 1: private getters/setters
- [x] Slice 2: computed instance field keys
- [x] Slice 3: `static {}` blocks, `o?.#x`
- [x] Slice 4: test262 rerun, follow-ups, baseline

## Notes / Result
**What changed**
- IR: `PrivateAdd { kind: PrivateKind }` (field / method / accessor), `NewAccessor` (a getter/setter pair as a
  value), `ToPropertyKey`. Backend: `private_get`/`private_set` call an accessor's getter/setter (node's
  "'#x' was defined without a getter/setter"); `private_add_accessor` installs it with `ACCESSOR`; ToPropertyKey of
  an object now goes through ToPrimitive (hint string), so a `Symbol.toPrimitive` returning a symbol gives that
  symbol (all computed keys, not only class fields).
- Lowering (`classes.rs`): private getters/setters go into `#x.get`/`#x.set` locals and are paired after the class
  body (static: added to the class; instance: into the private cell the constructor installs from, before the
  fields). Computed field keys (instance and static) are evaluated in element order into a cell per key (the class
  object exists before its elements run, so the constructor captures the cell: `Cap::FieldKey`). `static {}`
  blocks are lowered inline like static field initializers (their range is in `ClassInfo::statics`, so `this`,
  `super` are the class); their captured `var`s get cells through `var_cells` (split out of `prologue`).
  `o?.#x`, `o?.a.#x`, `o?.#m()` in the optional-chain lowering. Brand messages: a static method/accessor says
  "Receiver must be class A", and writes to a method/accessor use the brand message (like node).
- Destructuring assignment targets take private members and `super.x` (`member_ref` + `member_set`).
- `scripts/test262.mjs` skips the `nonextensible-applies-to-private` feature (node 24 lacks it).

**Tests**: fixtures `tests/fixtures/classes/{private-accessors,computed-fields,static-blocks}.js` (pass with
`scripts/diff.sh`), `crates/lower/tests/unsupported.rs` (the three constructs now lower), `cargo test`, the
fixtures validate, wasmtime smoke run. Whole `scripts/diff.sh`: 106 pass, 1 fail (`async/for-await-order.js`,
fails on the T30 commit too: F157).

**test262**: the 977 non-passing tests under `language/{statements,expressions}/class` plus
`language/expressions/{optional-chaining,assignment/dstr,object}` and `language/statements/class/subclass` (2,618
tests, 855 s at -j 4, `--update`): 563 unsupported → pass, 2 → skip (node-missing feature), 2 → fail
(`static-init-arguments-*`: `arguments` in a parameter default, F158), no regressions. Subset: 21,475 of 24,820
run pass (86.5%, was 84.2%). The class tests left are `eval` (179, F151), a class or parameter used before its
declaration (25, F63), `Proxy`/`Function`/`Reflect` (F151, F152), `new.target`/`super()`/`arguments`/`this` in
arrows (F57, F103).

**Unsupported**: `accessor` fields (node 24 rejects them too).

**Next smallest step**: T32 (`defineProperty` on arrays) is proposed; F157 (the `for await` fixture) is a small
separate fix.

## Progress
- 2026-09-27: task written as a proposal after T30 (largest remaining test262 cause, ~570 tests). Not started.
- 2026-09-27: approved; started slice 1 (read classes.rs, props.rs).
- 2026-09-27: slice 1 implemented: `PrivateKind` on `PrivateAdd`, `NewAccessor`, `private_get`/`private_set` call accessors (node's 'defined without a getter/setter'); static brand message 'Receiver must be class A'; method writes use the brand message; destructuring targets take private members and `super.x` (`member_ref`). Fixture `classes/private-accessors.js` passes; class fixtures pass.
- 2026-09-27: slice 2 implemented: computed field keys (instance and static) evaluated in element order into a cell per key (the class object is made before its elements, so the constructor captures the cell: `Cap::FieldKey`), `Intrinsic::ToPropertyKey` once at class evaluation. Fixture `classes/computed-fields.js` passes; static computed keys now also run in order (they ran after the methods before).
- 2026-09-27: slice 3 implemented: `static {}` blocks lowered inline like static field initializers (their range is in `ClassInfo::statics`, so `this`/`super` are the class; captured `var`s get cells via the new `var_cells`, split out of `prologue`); `o?.#x`, `o?.a.#x`, `o?.#m()` in `chain_link`/`chain_call_link`. Fixture `classes/static-blocks.js` passes; all 11 class fixtures pass, `cargo test -p js2wasi3-lower` ok, the 3 new fixtures validate, wasmtime smoke run ok. Next: slice 4 (test262 rerun of class dirs).
- 2026-09-27: slice 4: test262 rerun (2,618 tests): 563 → pass, no regressions; ToPropertyKey of objects via ToPrimitive (2 computed-name tests); runner skips `nonextensible-applies-to-private`. Full diff.sh: only `async/for-await-order.js` fails, also on the T30 commit (F157). Task done.
