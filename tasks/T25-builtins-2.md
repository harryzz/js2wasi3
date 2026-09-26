# T25: Built-ins, second batch

Status: todo | Depends on: T21

## Goal
Close the built-in gaps T21 left open, on T21's prelude pipeline (D5: JS in `crates/lower/builtins/*.js`,
`__intrinsic_*` for what JS can't express, only what's reachable is compiled). Split into slices, one session each
(log each slice in Progress, like T21):

1. **`JSON.parse`** (F93): `JSON.parse(text, reviver)` in `builtins/json.js`; node's SyntaxError messages
   (`Unexpected token … in JSON at position n` / node 20's wording: check with node); the reviver walk
   (InternalizeJSONProperty, deletes on `undefined`); `__proto__` as an own key (the `define` intrinsic).
   String → number for number literals goes through `$$StringToNumber`.
2. **`Map` / `Set`** (and `WeakMap`/`WeakSet` if cheap): a new runtime type in the backend (a hash table keyed
   by SameValueZero: numbers by value, `-0` = `+0`, strings by content, objects by identity; insertion order kept
   across deletes, iteration that sees entries added during it). Methods in the prelude over intrinsics, or native
   helpers: decide and record it in D5. `new Map(iterable)`, `get/set/has/delete/clear/size`, `forEach`,
   `keys/values/entries`, `[Symbol.iterator]`, `Map.groupBy`; `console.log` prints them like node
   (`Map(2) { 'a' => 1, … }`, `Set(1) { 1 }`).
3. **Functions and `arguments`**: `Function.prototype.bind` (bound functions: `name` `bound f`, `length`,
   `new` on them), `Function.prototype.toString` (source text: the span is known at compile time),
   `Function.prototype` as a function (F47), `arguments` (F42: an array-like object, unmapped as in strict mode).
   Also F43, F59 (computed keys whose value is a function: SetFunctionName at run time).
4. **Built-ins as values, missing methods**: `Math`, `JSON`, `String`, `Number`, `Array`, `Object` usable as values
   (`arr.map(String)`, `x.constructor === Array`, `console.log(Math)`) (F81, F88, F91, F93); wrapper objects
   `new String/Number/Boolean` (F88) if needed by the rest; `Number.prototype.toPrecision/toExponential`,
   `String.prototype.localeCompare` (code-unit order is not node's: decide), `normalize` (F87),
   `Array.prototype.keys/entries/toLocaleString` (F91). Also F37 (the remaining sentinels: `__defineGetter__`
   & co., built-in prototypes' `constructor`) and F95 (the JSON circular message reads those `constructor`s).
5. **Symbols and printing**: `Symbol.for`/`keyFor`, well-known symbols (`toStringTag`, `toPrimitive`,
   `hasInstance`…), `Object.getOwnPropertySymbols`, symbol keys in spread/`assign` (F53, F94); iteration through
   `Symbol.iterator` for arrays/strings when replaced (F51); `console.log` format strings (F23) and the remaining
   `util.inspect` gaps (F24, F62: circular refs, class instances, null prototypes, > 100 elements). Also F70
   (generator functions' own `prototype`, `%IteratorPrototype%`, `%GeneratorFunction%`, `Symbol.toStringTag`).
6. **Promises and timers**: F64 (`Promise.any` + `AggregateError`, `Promise.withResolvers`, species /
   subclassing, statics honouring `this`), F67 (`queueMicrotask`'s `code: 'ERR_INVALID_ARG_TYPE'`, unhandled
   rejections of functions / errors printed like node), F74 (Timeout objects: `ref`/`unref`/`hasRef`/`refresh`/
   `[Symbol.toPrimitive]`, clearing by numeric id, `setImmediate`/`clearImmediate`, `process.nextTick`,
   `TimeoutOverflowWarning`).

Every open follow-up whose "Pick up in" is T25 belongs to one of the slices above:

| Slice | Follow-ups |
|---|---|
| 1 | F93 |
| 2 | (none yet: `Map`/`Set` had no follow-up) |
| 3 | F42, F43, F47, F59 |
| 4 | F37, F81, F87 (`localeCompare`, `normalize`), F88, F91, F95 |
| 5 | F23, F24, F51, F52, F53, F62, F70, F94 |
| 6 | F64, F67, F74 |

## Out of scope
`Date` (T26), `RegExp` and regex literals (T27), BigInt, typed arrays / `ArrayBuffer`, `Intl`, `Proxy`/`Reflect`
(record them as follow-ups if a slice needs them).

## Acceptance
- [ ] Each slice: fixtures in `tests/fixtures/builtins/` match node (`scripts/diff.sh`), `unsupported.rs` updated.
- [ ] `hello.js` still pulls in no unused built-in (size unchanged, or the growth explained).
- [ ] The follow-ups each slice resolves are ticked in `tasks/README.md`.
- [ ] T25 is done only when every follow-up in the table above is ticked, or moved to another task with the reason
      written in its row (`tasks/README.md`) and in Notes / Result.

## Notes / Result

## Progress
