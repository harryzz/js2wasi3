# T30: wrapper objects (`new String/Number/Boolean`, `Object(prim)`)

Status: todo (proposed) | Depends on: T25 (built-in constructors as values), T28

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
- [ ] Slice 1: representation + construction
- [ ] Slice 2: unwrapping, this-value checks, missing prototype methods
- [ ] Slice 3: String exotic, tags, JSON, console.log
- [ ] Slice 4: test262 rerun, follow-ups, baseline

## Notes / Result

## Progress
- 2026-09-27: task written as a proposal after T29 (largest remaining test262 cause, 826 tests). Not started.
