# T32: `Object.defineProperty` on arrays (accessor elements, element attributes, `length`)

Status: todo (proposed) | Depends on: T16 (attributes, accessors), T21 (array methods in the prelude)

## Goal
Arrays take any property descriptor on their indices and on `length`, like node (F145). Today an `$array` has no
attribute bytes for its elements (only whole-array `SEALED`/`FROZEN` flags), so `defineProperty(arr, "0", { get })`,
a non-writable/non-enumerable/non-configurable element, or `defineProperty(arr, "length", { writable: false })` trap
(`Object.defineProperty`, `array_define`, `array_set_length`). A rerun on 2026-09-27 (after T30) found **213**
test262 failures with these traps: 194 define an accessor element (mostly `built-ins/Array/prototype/{reduce,
reduceRight,indexOf,lastIndexOf,sort,map,forEach,filter,some,every}`: "a getter on index k runs / throws / changes
the array"), 12 define a data element with other attributes, 7 redefine `length` (non-writable, RangeError).

## What node does (the behaviour to match)
- `Object.defineProperty(a, "1", { get() { … }, configurable: true })`: `a[1]` calls the getter (with `a` as
  `this`); `a[1] = v` calls the setter or, without one, throws in strict mode ("Cannot set property 1 of [object
  Array] which has only a getter"). `1 in a`, `hasOwnProperty`, `Object.keys` and `for-in` see it (enumerable by its
  descriptor), `getOwnPropertyDescriptor` returns it. Array methods reach it through `Get`/`HasProperty` like any
  element (the prelude's generic algorithms already do; the native `push`/`pop`/`join`/`toString`, the iterator
  fast path and `console.log` must not skip it: node prints `[Getter]`, `[Setter]`, `[Getter/Setter]`).
- Data elements with `writable: false` (write throws "Cannot assign to read only property '0' of object '[object
  Array]'"), `enumerable: false` (hidden from `Object.keys`/`for-in`, but `console.log` still prints the value:
  `[ 1, [Getter], 9 ]` with keys `['0', '1']`, node 24.21.0), `configurable: false` (`delete` throws; `length`
  can't shrink past it).
- `length`: `defineProperty(a, "length", { value: n })` = ArraySetLength (RangeError "Invalid array length" for
  non-integers / ≥ 2^32; ToNumber twice, test262 checks the order); after `writable: false`, `push` and writes past
  the end throw "Cannot assign to read only property 'length' of object '[object Array]'" (checked).

## Plan (slices)
1. **Representation**: a nullable `eattrs: mut (ref null $bytes)` on `$array` (null: all elements default, the
   fast path everywhere); an accessor element's slot holds its `$accessor` with `ACCESSOR` in `eattrs`. A
   `LENGTH_READONLY` class flag (next to `SEALED`/`FROZEN`). `array::Helper::OwnGet`/`Define` and
   `props::Value`/`Set` honour them; `defineProperty`/`getOwnPropertyDescriptor` on indices and `length` stop
   trapping (ValidateAndApplyPropertyDescriptor as for objects). Fixture `tests/fixtures/builtins/array-define.js`.
2. **Everything that reads elements directly**: the native `push`/`pop`/`join`/`toString`, `$iter`'s array fast path,
   spread, `inspect` (`[Getter]` etc.), `Object.keys`/`for-in` (hidden elements), `delete` and length shrinking
   (non-configurable elements stop it), JSON: each takes the slow (generic `Get`) path when `eattrs` is non-null.
   Fixture.
3. **`length`**: ArraySetLength through `defineProperty` (RangeError, conversions in the spec's order), non-writable
   `length`. Fixture.
4. **test262**: rerun the 213 (plus `built-ins/Array` failures mentioning `defineProperty`, ~5 min) with `--update`;
   new causes → follow-ups; `tests/test262/HISTORY.md`.

## Read first
- F145, F108 in `tasks/README.md`; T16's Notes / Result (attributes, `$accessor`, sealed/frozen arrays), T15's
  (array layout).
- `crates/backend/src/array.rs` (layout, `OwnGet`/`Define`/`SetIndex`/`SetLength`, native methods),
  `crates/backend/src/props.rs` (`define_property`, `Value`, `Set`, descriptors), `crates/backend/src/iter.rs` (array
  fast path), `crates/backend/src/inspect.rs` (`format_array`), `crates/backend/src/object.rs` (keys, delete).

## Tests
One fixture per slice in `tests/fixtures/builtins/` (`scripts/diff.sh`), plus the existing array fixtures (the fast
paths change), `cargo test`, `wasm-tools validate` + one wasmtime run. test262 as in slice 4.

## Checklist
- [ ] Slice 1: element attributes and accessors, `defineProperty`/descriptors on indices
- [ ] Slice 2: native methods, iteration, printing, keys, delete on such arrays
- [ ] Slice 3: `length` (ArraySetLength, non-writable)
- [ ] Slice 4: test262 rerun, follow-ups, baseline

## Notes / Result

## Progress
- 2026-09-27: task written as a proposal after T30 (213 test262 failures). Not started.
