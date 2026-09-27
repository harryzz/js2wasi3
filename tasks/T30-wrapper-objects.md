# T30: wrapper objects (`new String/Number/Boolean`, `Object(prim)`)

Status: done | Depends on: T25 (built-in constructors as values), T28

## Goal
Primitive wrapper objects work like node (the wrapper part of F106). Today `new String(s)`, `new Number(n)`,
`new Boolean(b)` and `Object(prim)` trap (`wrapper_object_not_implemented`, `crates/backend/src/func.rs`), and some
built-in prototype methods they need are still sentinels (`property_value` → `BuiltinTrap`, e.g.
`Boolean.prototype.valueOf`). It is the largest test262 cause left: **826 tests** (built-ins/String/prototype 140,
language/expressions/compound-assignment 132, built-ins/Array/prototype 82, the arithmetic, shift and relational
operator directories ~12–15 each). Of those, `new String` appears in 371, `new Number` in 253, `new Boolean` in
237, `Object(…)` in 34.

## What node does (the behaviour to match)
- `new String("ab")`, `new Number(1)`, `new Boolean(false)`: objects (`typeof` "object", truthy even for
  `new Boolean(false)`), prototype `String.prototype` etc., holding the primitive ([[StringData]] /
  [[NumberData]] / [[BooleanData]]). `Object(p)` / `new Object(p)` for a string, number, boolean or symbol make the
  same (symbol: a Symbol wrapper). `String(x)`/`Number(x)`/`Boolean(x)` without `new` stay conversions.
- ToPrimitive of a wrapper goes through `valueOf`/`toString` (user-overridable): so `new Number(2) * 3` is 6,
  `new Boolean(true) ^ true` is 0, `new String("a") + 1` is "a1", `new String("1") == 1` is true.
- `String.prototype.*`, `Number.prototype.*`, `Boolean.prototype.*` accept a wrapper as `this`
  (thisStringValue / thisNumberValue / thisBooleanValue); `valueOf`/`toString` of the wrong type throw TypeErrors
  with node's messages. `String.prototype` methods take any `this` through ToString already.
- A String wrapper is exotic: own index properties `"0"`… (read-only, enumerable) and `length` (read-only), so
  `Object.keys(new String("ab"))` is `["0", "1"]` and `for-in` visits them.
- `Object.prototype.toString.call(w)`: `[object String]` / `[object Number]` / `[object Boolean]`.
- `JSON.stringify` unwraps them (F93's leftover); `console.log` prints `[String: 'ab']`, `[Number: 1]`,
  `[Boolean: false]` (with extra own properties like node's `util.inspect`).
- Strict-mode primitive `this` (`fn.call(true)`) stays a primitive: no change.

## Plan (slices)
1. **Representation + construction**: a wrapper is an ordinary `$object` (prototype, properties) with the primitive
   in an internal slot (e.g. a WasmGC subtype of `$object` with a `prim` field, so existing object paths keep
   working and `ref.test` tells wrappers apart). `new String/Number/Boolean` and `Object(prim)` make them (replace
   `WrapperTrap`); `typeof`, truthiness, identity equality, `instanceof`. Fixture first; `diff.sh`.
2. **Unwrapping**: ToPrimitive (`valueOf`/`toString` lookup: probably already generic for objects; check), the
   prototype methods' this-value checks (`$$thisStringValue` etc. in the prelude or native), the missing
   `Boolean.prototype.valueOf`/`toString` and any other sentinel the tests reach.
3. **String exotic + printing**: own indices and `length` (get, `Object.keys`, `for-in`,
   `getOwnPropertyNames`, `hasOwnProperty`, writes to them fail in strict mode), `Object.prototype.toString` tags,
   `JSON.stringify`, `console.log`.
4. **test262**: rerun the 826 tests (~7 min at -j 4); new causes → follow-ups; baseline + `tests/test262/HISTORY.md`.

## Read first
- F106 and F88 in `tasks/README.md`; T25's Notes / Result (slice 4: constructors as values).
- `crates/backend/src/func.rs` (`builtin_ctor`, `WrapperTrap`), `crates/backend/src/object.rs` (`$object` layout,
  `Proto`), `crates/backend/src/props.rs` (`value`, `BuiltinTrap`), ToPrimitive in the runtime,
  `crates/lower/builtins/object.js` (`$$Object`, `$$Boolean`), `string.js` (`$$String`), `crates/backend/src/inspect.rs`.

## Tests
One fixture per slice in `tests/fixtures/builtins/` (`scripts/diff.sh`), `cargo test`, `wasm-tools validate` + one
wasmtime run; `tests/fixtures/exceptions/trap.js` (a wrapper trap that must not be caught) needs a new trap source or
removal once wrappers work. test262 as in slice 4.

## Checklist
- [x] Slice 1: representation + construction
- [x] Slice 2: unwrapping, this-value checks, missing prototype methods
- [x] Slice 3: String exotic, tags, JSON, console.log
- [x] Slice 4: test262 rerun, follow-ups, baseline

## Notes / Result
**What changed**
- `crates/backend/src/wrapper.rs` (new): `$wrapper` (`T_WRAPPER` = 27, a final subtype of `$object` + `prim: jsval`,
  class `CLASS_WRAPPER` = 8). Helpers: `New(prim, proto)` (a String wrapper gets its indices as real own properties,
  enumerable/read-only/non-configurable, and a hidden read-only `length`: so `Object.keys`, `for-in`,
  `hasOwnProperty`, `getOwnPropertyNames` and strict-mode write errors needed no new code), `ToObject`, `Unwrap`,
  `Tag` (`String`/`Number`/`Boolean`/`Symbol`), `Construct(b)`: the construct entry of the `String`/`Number`/`Boolean`
  values (the prelude conversion, then wrapped; prototype from `new.target`; `new String(symbol)` throws).
- IR/lowering: intrinsics `__intrinsic_to_object(v)` / `__intrinsic_unwrap(v)` (`Intrinsic::ToObject`/`Unwrap`).
- Prelude: `$$Object` wraps primitives, `Object.assign` uses ToObject, `$$thisObject` (array.js) is ToObject,
  `$$thisNumber`/`$$thisSymbol`/String `valueOf`/`toString` unwrap, `Boolean.prototype.valueOf`/`toString` added
  (`$$thisBoolean`), `JSON.stringify` unwraps values, replacer-array items and `space` (`$$jsonUnwrap`).
- Runtime: ToPrimitive gives a wrapper's primitive when it reaches the built-in (sentinel) `valueOf`/`toString` of
  the wrapper's own kind of prototype (compared with the `G_PROTOS` global, so nothing is linked for programs
  without wrappers); a Symbol wrapper converts to its symbol. `Object.prototype.toString` (native path and the
  prelude's through `builtin_tag`) and read-only error texts (`props::desc`) say `[object String]` etc.
- `console.log`: node's boxed form (`inspect::format_object`): `[String: 'ab']`, `[Number: -0]`,
  `[Symbol: Symbol(q)]`, `[Number (Foo): 5]`, `[Number (null prototype): 7]`, `[String: 'ab'] { extra: 1 }`.
- `func::Helper::WrapperTrap` is now `StubTrap` (`constructor_stub_not_implemented`, constructor stubs only);
  `exceptions/trap.js` and the build.rs streaming test trap on `"".anchor` instead.

**Tests**: fixtures `tests/fixtures/builtins/wrappers.js`, `wrappers-exotic.js` (= node); `diff.sh` builtins +
exceptions: 37 pass; `cargo test` ok; `wasm-tools validate` + wasmtime run ok.

**test262**: 765 non-passing tests (a wrapper-trap reason or wrappers in the source): 745 pass, 0 regressions;
total 20,912 of 24,822 run (84.2%, was 81.2%). Left: F108 (native `join`/`pop`/`push` on non-arrays,
`isConcatSpreadable`), T27 (`match`/`search`).

**Unsupported / left**: F156 (see `tasks/README.md`).

**Next smallest step**: by test262 count: classes (F58, ~580: private accessors, computed instance keys),
`defineProperty` on arrays (F145, 202).

## Progress
- 2026-09-27: task written as a proposal after T29 (largest remaining test262 cause, 826 tests). Not started.
- 2026-09-27: started (approved). Status in progress.
- 2026-09-27: design: `$wrapper` = final subtype of `$object` + `prim: jsval`; construct entry of String/Number/Boolean value = native helper that runs the prelude conversion then wraps (proto from new.target like ErrorConstruct); `Object(prim)` via a prelude intrinsic `to_object`; String wrapper gets real own index props (enumerable, read-only, fixed) + hidden `length`, so keys/for-in/writes need nothing new; ToPrimitive: the sentinel valueOf/toString of the matching prototype unwraps; prelude this-value checks unwrap via an intrinsic.
- 2026-09-27: slice 1 (+ most of 2) implemented: `crates/backend/src/wrapper.rs` (`$wrapper` T_WRAPPER=27, New/ToObject/Unwrap/Construct), intrinsics `to_object`/`unwrap`, `$$Object` wraps, `Object.assign` ToObject, `$$this{Number,Symbol,Boolean}` + String valueOf/toString unwrap, `Boolean.prototype.valueOf/toString` in object.js, ToPrimitive unwraps via sentinel valueOf/toString of the wrapper's prototype and Symbol wrappers. `tests/fixtures/builtins/wrappers.js` PASS (diff.sh), validate + wasmtime OK.
- 2026-09-27: slice 3: `wrapper::Helper::Tag`; `[object String]` etc. in ToPrimitive's Object.prototype.toString, `builtin_tag` (prelude toString) and read-only error descriptions (`props::desc`); JSON.stringify unwraps values, replacer-array items and `space` (`$$jsonUnwrap`); console.log prints node's boxed form (`[String: 'ab']`, `[Number (Foo): 5]`, `[Number (null prototype): 7]`, ` { extra: 1 }`) in `inspect::format_object`. Own indices/length/keys/for-in/strict writes already worked from slice 1's real properties. `tests/fixtures/builtins/wrappers-exotic.js` + `wrappers.js` PASS. `exceptions/trap.js` + build.rs stream test now trap on `"".anchor` (builtin_not_implemented); stub trap renamed StubTrap/constructor_stub_not_implemented.
- 2026-09-27: `$$thisObject` (array.js) now ToObject. Slice 4: running test262 on 765 non-passing tests (wrapper-trap reasons + sources using wrappers), -j 4, no --update yet (log: scratchpad t30-run.log).
- 2026-09-27: `new String(symbol)` throws a TypeError. test262 rerun with --update: 745 of 765 pass, 0 regressions; total 84.2%. HISTORY, notes, README updated. T30 done.
