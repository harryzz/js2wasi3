# T32: `Object.defineProperty` on arrays (accessor elements, element attributes, `length`)

Status: done | Depends on: T16 (attributes, accessors), T21 (array methods in the prelude)

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
- [x] Slice 1: element attributes and accessors, `defineProperty`/descriptors on indices
- [x] Slice 2: native methods, iteration, printing, keys, delete on such arrays
- [x] Slice 3: `length` (ArraySetLength, non-writable)
- [x] Slice 4: test262 rerun, follow-ups, baseline

## Notes / Result
**What changed**
- `$array` has `eattrs: mut (ref null $bytes)` (`array::F_EATTRS`): each element's `HIDDEN`/`READONLY`/`FIXED`/
  `ACCESSOR` bits, null while all are default; an accessor element's slot holds its `$accessor` (like the
  dictionary part). New `class` flag `object::LENGTH_READONLY` (0x800). Array helpers: `Attrs` (with `SEALED`/`FROZEN`
  folded in), `SetAttrs` (makes/grows `eattrs` only for non-default bits), `Resize` (shrinking stops at a
  non-configurable element: keeps up to it, then node's "Cannot delete property 'k' of [object Array]").
- `Object.defineProperty` (`props.rs`): the ValidateAndApplyPropertyDescriptor code is generated for both storages
  (`validate`/`apply` over a dictionary entry or an array element), so indices follow the object rules; a new
  element gets absent attributes false (holes before it). `length` is ArraySetLength: ToUint32 and ToNumber
  (two conversions) and the RangeError before the other checks; never configurable/enumerable/accessor;
  `writable: false` is applied even when a shrink fails. `getOwnPropertyDescriptor` reports real element and
  `length` attributes.
- Writes: `set_index` (every `a[i] = v`, `push`) calls an accessor element's setter (node's "only a getter"
  TypeError), rejects read-only elements and growth past a read-only `length`; `a.length = v` converts twice too;
  `push()`/`pop()` throw on a read-only/frozen `length` even when nothing changes.
- Reads resolve accessor elements with `props::Value`: the `get` fast path, native `join`/`pop`, the iterator's
  array fast path (so spread, `for-of`, destructuring); `console.log` prints `[Getter]`/`[Setter]`/
  `[Getter/Setter]` and, like node's `formatSpecialArray`, shows non-enumerable elements after the first hole as
  holes. `delete`/`pop` honour non-configurable elements; `Object.keys`/`for-in` skip non-enumerable elements
  (`IndexKeys` takes `all`).
- F157: `tests/fixtures/async/for-await-order.js` split (the error cases are `for-await-errors.js`): it fails only
  under wasmtime's copying collector at certain heap states (see F157).

**Tests**: fixtures `tests/fixtures/builtins/array-define.js`, `array-length.js` (= node); `diff.sh` arrays,
builtins, objects, classes (53 pass); whole `diff.sh`: 109 pass, 1 fail under load (`async/timers-more.js`, a
timer fixture; passes 3/3 alone); `cargo test`; the fixtures validate; wasmtime smoke run.

**test262**: all of `built-ins/Array`, `built-ins/JSON`, `language/statements/for-of`, `language/expressions/array`
(4,050 tests, 1,299 s at -j 4, `--update`): 237 fail → pass, no regressions; then 4 more (`push`/`pop` setting a
read-only length). Subset: 21,716 of 24,820 run pass (87.5%, was 86.5%); `built-ins/Array` 83.5% (was 75.6%).
Left in `built-ins/Array`: `push`/`pop`/`join` on non-arrays and species (F108), holes not looking up the
prototype in the native paths (F159), indices ≥ 2^26 (no sparse arrays, F160), `Proxy`/`Reflect`/`Function`/
`globalThis` (F151, F152).

**Unsupported**: CreateDataProperty on an existing accessor or read-only element (`object::Helper::Define` shares
`set_index`, so it calls the setter / throws instead of redefining; F161).

**Next smallest step**: no task is proposed; candidates: F159 (holes → prototype chain in the native paths),
F108 (generic `push`/`pop`/`join`).

## Progress
- 2026-09-27: task written as a proposal after T30 (213 test262 failures). Not started.
- 2026-09-27: approved; started slice 1 (reading array.rs, props.rs).
- 2026-09-27: slice 1 in progress: `$array` got `eattrs` (F_EATTRS=9, nullable $bytes), `LENGTH_READONLY` class flag 0x800; array helpers Attrs/SetAttrs/Resize; set_index calls accessor setters, rejects read-only elements and growth past a read-only length; set_length converts twice (ArraySetLength) and Resize stops at non-configurable elements.
- 2026-09-27: slices 1–3 implemented: defineProperty on indices shares ValidateAndApply with objects (storage-parametrized `validate`/`apply` in props.rs), `length` via ArraySetLength (2 conversions, RangeError first, LENGTH_READONLY, Resize), getOwnPropertyDescriptor from real element attrs; readers resolve accessor elements (get fast path, join, pop, iterator, inspect [Getter]); delete/pop honour FIXED; IndexKeys(all) hides non-enumerable; inspect treats hidden elements as holes after the first hole (node's formatSpecialArray). Fixtures builtins/array-define.js, array-length.js pass.
- 2026-09-27: slice 4: test262 (4,050 tests) 237 fail → pass, 0 regressions; push()/pop() on read-only length +4. diff.sh whole: only a timer fixture under load. Task done.
