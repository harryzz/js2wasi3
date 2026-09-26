# T25: Built-ins, second batch

Status: in progress | Depends on: T21

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
| 2 | (none: `Map`/`Set` had no follow-up; slice 2 found F97–F100) |
| 3 | F42, F43, F47, F59 |
| 4 | F37, F81, F87 (`localeCompare`, `normalize`), F88, F91, F95, F97, F101 |
| 5 | F23, F24, F51, F52, F53, F62, F70, F94, F99 |
| 6 | F64, F67, F74 |

## Out of scope
`Date` (T26), `RegExp` and regex literals (T27), BigInt, typed arrays / `ArrayBuffer`, `Intl`, `Proxy`/`Reflect`
(record them as follow-ups if a slice needs them).

## Acceptance
- [x] Each slice: fixtures in `tests/fixtures/builtins/` match node (`scripts/diff.sh`), `unsupported.rs` updated.
- [x] `hello.js` still pulls in no unused built-in (size unchanged, or the growth explained).
- [x] The follow-ups each slice resolves are ticked in `tasks/README.md`.
- [x] T25 is done only when every follow-up in the table above is ticked, or moved to another task with the reason
      written in its row (`tasks/README.md`) and in Notes / Result.

## Notes / Result

### Slice 1: `JSON.parse` (done 2026-09-26)
**What changed**
- `crates/lower/builtins/json.js`: `$$JSON_parse` (`JSON.parse(text, reviver)`). ToString of the argument
  (`__intrinsic_to_string`: a symbol throws), then a recursive-descent parser over code units. Its state
  (`$$jsonSrc`, `$$jsonLen`, `$$jsonPos`) is in prelude globals: no user code runs while parsing (the reviver runs
  after), and `$$jsonSrc` is reset to `null` (not `""`: string literals aren't global initializers) on every exit.
  Strings: a fast path returns one substring when there are no escapes, else a validating pass counts the length and
  a second builds it (`str_new`/`str_set`). Numbers: the grammar is checked by hand; integers of ≤ 15 digits are
  accumulated directly, anything else goes through `$$StringToNumber` (correctly rounded). Members are stored with
  `__intrinsic_define` (CreateDataProperty: `__proto__` is an own key; a repeated key keeps its first position,
  the last value wins). Arrays: `[]` + native `push`. The reviver walk (`$$jsonInternalize`/`$$jsonRevive`):
  depth first, holder as `this`, array length read once, keys from `Object.keys` read before the walk, `undefined`
  deletes (holes in arrays), the root is called with `""` on a `{"": value}` wrapper.
- node's messages, all reproduced exactly (probed with node 20.19.2; node 24 adds ` (line l column c)` and drops the
  first-property quirk below, see Progress): the position form (`… in JSON at position
  n`), "Unexpected end of JSON input", "Unexpected string/number in JSON at position n" (a token starting with `"`,
  a digit or `-` where it doesn't fit), `"undefined" is not valid JSON` (whole source `undefined`, `NaN`,
  `Infinity` or `[object Object]`), and "Unexpected token 'c', <context> is not valid JSON": the whole source if it
  is shorter than 21 code units, else 10 units on each side of the position with `...` on a cut side
  (V8's `kMaxContextCharacters`/`kMinOriginalSourceLengthForContext`). V8 quirk: only an object's *first* property
  gets "Expected ':' after property name" / "Expected property name or '}'"; later ones get the generic token error
  / "Expected double-quoted property name". The specific messages keep the position form at the end of the input
  (`{` → position 1), the generic ones say "Unexpected end of JSON input".
- Tests: `tests/fixtures/builtins/json-parse.js` (values, escapes incl. surrogates, numbers incl. `-0`/`1e400`/
  17+ digits, duplicate keys, `__proto__`, ToString of the argument, ~140 error inputs incl. every context case,
  the reviver: call order, `this`, deletion, holes, root wrapper, added properties not visited, round trips);
  `builtins/lib.js` gets `reparse` (+ a `build.rs::builtins_in_a_library` ok and err case); `unsupported.rs` uses
  `JSON.rawJSON` as the unimplemented static.

**Commands run:** `scripts/diff.sh -j 4 tests/fixtures/builtins` (9 passed, 1 skipped), `cargo test -q
--no-fail-fast` (all ok), `wasm-tools validate` + `wasmtime run --invoke` on the library.

**Size:** `hello.js` is 52335 bytes both at `2e216b2` and after this slice (byte-identical; the handoff's 52177 was
stale). **Speed:** a 5.1 MB array of 40k small objects: ~0.8–1.4 s per parse vs node's ~0.05 s (15–25×; this
machine is noisy), recorded as F96.

**Unsupported / left:** `JSON.rawJSON`/`isRawJSON`, `JSON` as a value (slice 4, F93); the reviver's third
`context` argument (node 24 doesn't pass one either); deep nesting recurses (Wasm stack trap, F33); speed (F96).

**Next smallest step:** slice 2, `Map`/`Set`: decide native helpers vs prelude over intrinsics (D5) and start with
the backend hash table keyed by SameValueZero.

### Slice 2: `Map` / `Set` (done 2026-09-26)
**Decision (D5):** native storage, prelude methods. The hash table and iterator stepping are backend helpers
(`crates/backend/src/mapset.rs`); every JS-visible method is prelude JS (`crates/lower/builtins/map.js`) over
`__intrinsic_map_*` (`ir::Intrinsic::Map(MapOp)`: `new_map/new_set`, `kind`, `get`, `has`, `set`, `delete`, `clear`,
`size`, `iter_keys/values/entries`, `iter_kind`, `step`, `key`, `value`), so receiver checks, node's messages and
callbacks stay in JS.

**What changed**
- Runtime types: `$map` (a final subtype of `$object`, class 2/3, with a `$mtable`), `$mtable` (entries in insertion
  order, deleted = Wasm null key, open-addressing index, `next`/`removed` for obsolete tables), `$mapiter` (a
  subtype of `$object`: table, position, mode). SameValueZero hashing: numbers by value (`i31` and boxed alike, `-0`
  = `+0`, one NaN), strings/symbols by content (symbols then by identity), objects by an identity hash assigned
  lazily into `class` bits ≥ 12 (`G_ID_HASH`); a `-0` key is stored as `+0`.
- Iterators that survive changes: V8's OrderedHashTable scheme. A rehash (grow/compact) or `clear` makes a new table
  and marks the old one obsolete with the ascending list of dropped entries (null: cleared); an iterator on an
  obsolete table walks to the newest, mapping its position. So for-of/forEach see entries added meanwhile, skip
  deleted ones, and survive `clear` + adds (tested).
- `ir::Builtin::{Map, Set, MapIterator, SetIterator}` (prototypes with sentinels; iterator prototypes have no
  `constructor`); `Builtin::parse_prelude` maps `$$MapIterator_prototype_next` to `%MapIteratorPrototype%`. The
  backend installs `Map.prototype.entries` also as `[Symbol.iterator]`, `Set.prototype.values` also as `keys` and
  `[Symbol.iterator]` (the same function objects), and `size` as a getter (`get size`).
- Reachability (`builtins.rs`): `IMPLIED` deps (a constructed Map needs `entries` + its iterator's `next`, never
  named by user code) and `ALIASES` (`.keys` keeps `$$Set_prototype_values`).
- Lowering: `new Map(…)`/`new Set(…)` direct-call `$$Map`/`$$Set` (like `new Array`); `Map(…)`/`Set(…)` evaluate
  the arguments and throw node's `Constructor Map requires 'new'`; `instanceof Map` and `Map.prototype` come from
  `Builtin::Map`. `array.js`'s `$$describe` names maps `#<Map>`/`#<Set>` (their prototypes' `constructor` is a
  sentinel).
- `console.log`: `Map(n) { k => v, … }` / `Set(n) { … }`, `[Map]`/`[Set]` past depth 2, laid out like objects;
  linked only when the module can make a map (`ir::Module::maps`). Iterators, own properties, > 100 entries trap.
- Tests: `tests/fixtures/builtins/map-set.js` (SameValueZero keys incl. NaN/±0/objects/symbols/concatenated strings,
  order, constructors incl. generators/strings/maps, mutation during for-of/forEach incl. rehash and `clear`, a
  waiting iterator, 5000-entry rehash/identity-hash run, every error message, printing); `builtins/lib.js`
  `distinct` + `build.rs`; `unsupported.rs` (Map/Set lower; `WeakMap` is the example of an unsupported global now).

**Commands run:** `scripts/diff.sh -j 4` (75 passed, 0 failed, 10 skipped), `cargo test -q --no-fail-fast` (all ok).

**Size:** `hello.js` 52335 → 52534 bytes (+199): the three new struct types in the runtime's rec group and five
globals (four prototypes, `G_ID_HASH`), which every module has. Map printing and `define_accessor` are linked only
when used. **Speed:** 300k string-keyed set/get/delete + 300k numeric for-of + 300k object adds: ~4.3–5.1 s vs node
0.76 s (~6×, F100).

**Unsupported / left:** `Map`/`Set` as values and subclassing (F97, slice 4), `WeakMap`/`WeakSet` (F98), printing iterators / own properties / > 100 entries (F99, slice 5), speed (F100).

**Next smallest step:** slice 3: `Function.prototype.bind` (a bound-function object kind in `func.rs`: target, bound
`this`, bound args; `name` `bound f`, `length`, `new` through it).

### Slice 3: functions and `arguments` (done 2026-09-26)
**First session:**
- `Function.prototype.bind` (native, `func.rs` `Helper::Bind`): a `$function` of kind `KIND_BOUND` whose env is
  `[target, bound this, ...bound args]`; its code calls the target with the bound arguments first, its construct
  entry (only if the target has one) `new`s the target (`new_target` = the target when it's the bound function
  itself); `name` `bound f`, `length` max(0, target.length − bound args), no `prototype`; `instanceof` looks
  through bound functions. Installed only when the user's code names a `bind` property (`ir::Module::bind`).
- `arguments` (F42): the closure analysis marks non-arrow functions using it (`uses_arguments`); such a function is
  lowered as `f(...args)` (one rest parameter, so every call site already passes all arguments), its declared
  parameters are bound in the prologue from `args[i]` like default/pattern parameters, and
  `Intrinsic::MakeArguments` makes the unmapped object (class `CLASS_ARGUMENTS`: indices, hidden `length` and
  `[Symbol.iterator]` = `Array.prototype.values`; printed `[Arguments] { '0': 1 }`). `Array.prototype.values`
  now iterates any object as an array-like (`iter.rs` `ARRAY_LIKE`). Rejected: arrow functions, class
  constructors, with a rest parameter, exported functions (F103).
- V8's CallPrinter for object literals (`{(intermediate value)…}` per property) in "not a function/constructor/
  iterable" messages.
- Tests: `tests/fixtures/builtins/functions.js`, `tests/fixtures/syntax/callee-text.js`, `unsupported.rs`.
- `hello.js`: 52740 bytes (+206: the iterator's array-like branch and the `[Arguments]` check in existing helpers).

**Second session (the rest):**
- F47: `Function.prototype` is a `$function` (built in `object.rs` `proto()`; its code `func::Helper::Empty` returns
  `undefined`, not a constructor, `name: ""`, `length: 0`). `console.log` prints it `[Function (anonymous)] Object`
  like node (a function whose prototype is `Object.prototype` gets node's constructor-name suffix); a class's
  `extends` printing skips it.
- `Function.prototype.toString` (F43): `$function` has a new field `source: (mut (ref null $string))` (`F_SOURCE`).
  The lowerer records each function's span (`Lowerer::fn_source`: a method's from its key without `static`, an
  accessor's from `get`/`set`, a class's `c.span`) and `ir::Closure::source` carries the text; the backend sets it
  after `New` (`func::Helper::SetSource`). Only when the user's code names a `toString` property
  (`ir::Module::to_string`, like `bind`): then the real `toString` (`func::Helper::ToString`) replaces the sentinel,
  so ToPrimitive (`"" + f`, templates, `String(f)`) works too. No source (prelude functions, built-ins, bound
  functions): `function name() { [native code] }` / `function () { [native code] }`. A non-function `this`:
  node's TypeError.
- F59: anonymous functions/classes under a computed (or numeric) key get their `name` at run time
  (`Intrinsic::SetFunctionName` → `func::Helper::SetName`: prefix `get `/`set ` + the key, a symbol as `[desc]`):
  object literal values, methods, accessors; class methods, accessors, static fields.
- Tests: `tests/fixtures/builtins/function-source.js` (new), `unsupported.rs` (the computed-key case now lowers),
  `build.rs` comment. `cargo test` ok; `scripts/diff.sh -j 4`: 77 passed, 0 failed, 11 skipped.
- `hello.js`: 52865 bytes (+125 over 52740: the `$function` field, the `Function.prototype` code, two inspect checks).

**Not done (follow-ups):** F104 (sources only when `toString` is named; implicit conversion otherwise traps), F105
(SetFunctionName edge cases), F103 (`arguments` limits).

**Next smallest step:** slice 4 (built-ins as values) or T20, the user's choice. Slice 4's first piece: `Math`/`JSON`
as values (F81), then `String`/`Number`/`Array`/`Object` as function values with their prototypes' `constructor`
(F37), which F95's circular-JSON message needs.

### Slice 4: built-ins as values, missing methods (done 2026-09-26)
- **Constructors as values** (`Object`, `String`, `Number`, `Boolean`, `Array`, `Map`, `Set`): the prelude marks a
  global used other than called / `B.x` / `instanceof B` (plus spread calls and `B.apply|call|bind|name|length`) as a
  value use (`Prelude::ctor_values`), roots `$$B` and the statics the code names; the lowering emits
  `Intrinsic::BuiltinCtor(b)`, the backend `func::Helper::BuiltinCtor` makes the function object once (globals
  `G_CTORS`): code and construct = the prelude adapter (`Map`/`Set` calls throw "Constructor Map requires 'new'",
  `new String/Number/Boolean` trap: wrappers are F106), `prototype` = the built-in prototype, which gets it as
  `constructor`; `Number` gets its constants. New prelude functions `$$Object` (`Object(v)`, `new Object`) and
  `$$Boolean`. A program that names `constructor` but not the constructor gets **stubs** (`ir::Module::ctor_stubs`:
  identity, `name`, `length`, `prototype`; calling traps): +783 bytes instead of +20 KB of `$$Number`/`$$String`.
  This also fixes F95 (JSON's circular message reads `constructor`).
- **`Math`/`JSON` as values** (`ir::Namespace`, `Intrinsic::Namespace`, `func::Helper::Namespace`): an object of class
  `CLASS_MATH`/`CLASS_JSON` (printed `Object [Math] {}`) with `Math`'s constants and the prelude functions the code
  names, non-enumerable. `ir::MATH_CONSTANTS`/`NUMBER_CONSTANTS` are shared with the lowering.
- **Names from strings:** string literals in the user's code count as names for statics (`Math[k]`) and prototype
  methods (`s[m]()`), so a key read from a string finds its method when the name appears in the code. A name built
  at run time (`"fl" + "oor"`) finds nothing: F109.
- **Methods:** `Object.groupBy`; the ES2025 `Set` methods (GetSetRecord, node's messages); `__defineGetter__`,
  `__defineSetter__`, `__lookupGetter__`, `__lookupSetter__`; `Array.prototype.keys`/`entries` (native: `iter.rs`
  kinds `ARRAY_KEYS`/`ARRAY_ENTRIES`, printed `Object [Array Iterator] {}`), `Array.prototype.toLocaleString`,
  `Number.prototype.toLocaleString` (en-US: grouping, ≤ 3 fraction digits), `toExponential`/`toPrecision` (exact
  decimal expansion, ties up: `$$exactDigits`, `$$roundDigits`), `String.prototype.localeCompare` (ICU root
  collation for ASCII, order derived from node, verified on 2000 random pairs; other characters trap) and
  `normalize` (strings no form changes; others trap).
- **Gates** (`ir::Module::names`, the lowering's `GATED`): `keys`/`entries`, the iterator-step branch and the
  array-iterator printing are linked only when the code names `keys`/`entries`/`values`/`iterator`; the
  `Math`/`JSON` printing only when they're used as values.
- Tests: fixtures `builtins/ctor-values.js`, `namespaces.js`, `more-methods.js`, `number-format.js` (with a 300-value
  sweep; built without `**` because of F77), `locale.js`; `unsupported.rs` (values now lower; `Symbol` as the
  rejected example), `build.rs` (the sentinel example is now `String.prototype.match`). `cargo test` ok;
  `scripts/diff.sh -j 4`: 82 passed, 0 failed, 11 skipped.
- `hello.js`: 53043 bytes (+178 over 52865: the global slots for constructor and namespace values).

**Not done (follow-ups):** F109 (built-ins named only at run time aren't linked), F106 (wrapper objects, subclassing built-ins, native statics on constructor values,
stubs trap when called), F107 (`localeCompare`/`normalize`/`toLocaleString` beyond ASCII/en-US), F108 (arrays:
species, `isConcatSpreadable`, generic `push`/`join`; `Array.fromAsync`, iterator helpers). `[object Math]` and
`Symbol.toStringTag` are slice 5 (F94).

**Next smallest step:** slice 5 (symbols and printing) or T20, the user's choice. Slice 5's first piece:
`Symbol.toStringTag` on the built-in prototypes (F94), which `Object.prototype.toString` and `console.log` read.

### Slice 5: symbols and printing (done 2026-09-26)
- **Symbols (F53):** `builtins/symbol.js` (`Symbol.prototype` `toString`/`valueOf`/`description` getter,
  `Symbol.for`/`keyFor` over two maps); the well-known symbols (`ir::WELL_KNOWN_SYMBOLS`,
  `Intrinsic::WellKnownSymbol`, made on first use in `G_WELL_KNOWN`); `Object.getOwnPropertySymbols`
  (`object::Helper::OwnSymbols`), symbol keys in spread/rest (`copy_props`) and `Object.assign`; a symbol `==` only
  itself. `Symbol.toPrimitive` (ToPrimitive, with the hint strings; `string + x` now lowers to the run-time `+` so
  the hint is `default`) and `Symbol.hasInstance` (`instanceof`) are honoured when the code names them.
- **`Symbol.toStringTag` (F94):** `object::Helper::BuiltinTag` (the user's tag when `toStringTag` is named, else
  type checks: Map/Set and their iterators, Promise, Generator, Array Iterator, Math/JSON, generator and async
  functions) behind `Object.prototype.toString` (prelude, `__intrinsic_builtin_tag`) and the native `"" + obj`
  (gated on `Cx::has_tags`); tag properties on the built-in prototypes and `Math`/`JSON` when the code names
  `toStringTag`.
- **`console.log` (F24, F62, F99, F52):** `inspect_props` (string then symbol keys) shared by objects, arrays (extra
  properties) and maps/sets (own properties); at most 100 entries then `... n more items` (node's
  `groupArrayElements` tail and number-alignment rules); node's `strEscape` in `inspect_quote`; cycles
  (`inspect_format` keeps a stack of the objects being formatted: `[Circular *n]`, `<ref *n>` before the braces so it
  counts in the width like node's); Map/Set iterator previews (`inspect_map_iterator` steps a copy); user
  `Symbol.toStringTag` prefixes (`T [Tee] {}`).
- **Format strings (F23):** `builtins/console.js` `$$consoleFormat` (node's `formatWithOptions`: `%s %d %i %f %j %O
  %o %c %%`), linked only when a string in the user's code has a `%`; intrinsics `inspect(v, depth)` and
  `builtin_to_string` (node's `hasBuiltInToString`). Without a `%` string in the code a dynamic first argument
  keeps the run-time check (it traps on a `%`).
- **Iteration (F51):** `Array.prototype.values` and `[Symbol.iterator]` are one function object; built-in `values`
  functions have kind `KIND_VALUES`, and when the code names `iterator`, for-of/spread keep the array/string fast
  path only while `v[Symbol.iterator]` is one (a replaced or own one is called).
- Tests: fixtures `builtins/symbols.js`, `inspect-more.js` (23 cases), `console-format.js`, `iterator-lookup.js`;
  `exceptions/trap.js` and the streamed-output test now trap on `new Number(1)` (format strings work);
  `unsupported.rs`, `build.rs`. `cargo test` ok; `scripts/diff.sh -j 4`: 86 passed, 0 failed, 11 skipped.
- `hello.js`: 55175 bytes (+2132 over 53043): the formatter's new features (cycles, escapes, props, more-items) are
  linked into every command because the uncaught-exception path can print any value (F110).

**Not done (follow-ups):** F110 (errors nested in objects, non-ASCII widths, `%o` of objects, `Symbol()` vs
`Symbol("")`, F70's generator prototype chain, the formatter's size in every command).

**Next smallest step:** slice 6 (promises and timers: F64, F67, F74) or T20, the user's choice. Slice 6's first
piece: `Promise.withResolvers` and `Promise.any` + `AggregateError` (F64).

### Slice 6: promises and timers (done 2026-09-26)
- **`AggregateError` / `Promise.any` / `Promise.withResolvers` (F64):** `ir::ErrorKind::AggregateError` (also in
  `Builtin::ALL`); `new AggregateError(errors, message, options)` lowers to a 3-argument `NewError` whose helper
  iterates `errors` into an own hidden array (`AggregateError.length` 2). `promise::Combinator::Any` with
  `Element::AnyRejected`; `Helper::AnyReject` makes node's error with an own `stack` of just its header (V8 creates
  it without frames), so `console.log` and the uncaught report print `[AggregateError: All promises were rejected]
  { [errors]: [ … ] }` like node (inspect's error branch and `runtime::describe`, gated on `any`/`AggregateError`;
  an `AggregateError` in the code puts its name in `ir::Module::names`). `Promise.withResolvers` returns
  `{ promise, resolve, reject }`. Both statics are installed only when the code names them (`builtins::GATED`).
- **node errors (F67):** `promise::Helper::ThrowArgType`: the `ERR_INVALID_ARG_TYPE` TypeError gets an own
  enumerable `code` and an own `toString` (`TypeError [ERR_INVALID_ARG_TYPE]: …`), which also gives node's uncaught
  header. V8's NoSideEffectsToString (`promise::Helper::Describe`) of a function is its source (via
  `Function.prototype.toString`) when the code names `toString`; else the old text (F104).
- **Timers (F74):** `$timer` gained `delay`, `id`, `flags` (unref, immediate, cleared); Timeouts and Immediates get
  `Builtin::Timeout` / `Builtin::Immediate` prototypes (`ref`, `unref`, `hasRef`; Timeout: `refresh`,
  `[Symbol.toPrimitive]` = the id; internal names `%Timeout%`/`%Immediate%` so user code can't name them).
  `clearTimeout`/`clearInterval` take a Timeout or its id (number or string); a cleared timer stays dead on
  `refresh`; `refresh` after firing re-arms. An unref'd timer or immediate doesn't keep the loop alive
  (`timers::Helper::Next` returns -1 when nothing ref'd is left). `setImmediate`/`clearImmediate`: a FIFO
  (`G_IMMEDIATES`) run at the end of each timers pass (`RunImmediates`), only those queued before it.
  `process.nextTick` (lowered from `process.nextTick` only; `process` itself stays unsupported): `G_TICKS`, run by
  `RunTicks` (ticks, then microtasks, until both are empty) after every timer/immediate callback, and after the
  main module's microtasks (fixtures are ES modules: node runs the microtasks first there). The gates are
  `$timers`/`$immediates`/`$ticks` in `ir::Module::names` (set by the lowering); with timers, ToPrimitive looks up
  `Symbol.toPrimitive` (so `+timeout`, `` `${timeout}` `` give the id).
- Tests: fixtures `promises/any.js`, `any-unhandled.js`, `arg-type.js`, `unhandled-function.js`,
  `async/timers-more.js` (delays far apart: node's order of timers due together is racy), `timer-ids.js`;
  `unsupported.rs` (`nextTick`/`setImmediate` in a library). `cargo test` ok; `scripts/diff.sh -j 4`: 92 passed,
  0 failed, 11 skipped.
- `hello.js`: 55367 bytes (+192 over 55175): 15 more runtime globals declared in every module (the new error kind's
  and the two prototypes' slots, three timer-function slots, the immediate/tick queues and the id counter), which
  also pushes later global indices past 127 (2-byte LEBs). No new code is linked (F113).

**Not done (follow-ups):** F111 (promise species/subclassing, statics ignoring `this`), F112 (printing Timeout
objects, `Timeout`/`Immediate` constructors, id registration, warnings, `process` beyond `nextTick`), F113 (declare
only the runtime globals a module uses), F114 (`JSON.rawJSON`), F104 (function sources for rejection reasons
without a `toString` in the code), F31 (stacks).

**T25 is done:** every follow-up in the slice table is ticked or moved (F70 → F110, F93's rest → F114, F64's rest →
F111, F74's rest → F112).

**Next smallest step:** T20 (WIT async + user worlds): start with its "Read first" list.

## Handoff for slice 6 (done; written 2026-09-26, end of the slices 3–5 session)

**State:** `main` at `31a8218`, pushed. Slices 1–5 are done (Notes / Result above). Slice 6 is the last one:
F64, F67, F74 (rows in `tasks/README.md`). T25 is done when those are ticked or moved with a reason.
Commit/push only when the user asks; no attribution lines.

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo build -q --release && cargo test -q --no-fail-fast 2>&1 | grep -E 'FAILED|panicked'   # nothing
scripts/diff.sh -j 4 | tail -1                              # 86 passed, 0 failed, 11 skipped
target/release/js2wasi3 build tests/fixtures/hello.js -o /tmp/h.wasm && stat -c %s /tmp/h.wasm   # 55175
```

**Slice 6, smallest first:**
1. F64: `Promise.withResolvers` (`{ promise, resolve, reject }`), `Promise.any` + `AggregateError` (a new
   `ErrorKind`? check `ir::ErrorKind` and `object::Helper::NewError`; message `All promises were rejected`,
   `errors` own property). Promise code is native: `crates/backend/src/promise.rs`; statics are lowered from
   `Promise.x` (grep `PromiseCtor` in `crates/lower/src/lib.rs`). `Promise.prototype[Symbol.toStringTag]` is already
   handled by `object::builtin_tag_name`. Species/subclassing and statics honouring `this`: likely a follow-up.
2. F67: `queueMicrotask`'s `code: 'ERR_INVALID_ARG_TYPE'` (uncaught print `TypeError [ERR_INVALID_ARG_TYPE]: …`);
   unhandled rejection of a function (node prints its source: needs F104's gating); errors' stacks stay F31.
3. F74: Timeout objects (`crates/backend/src/timers.rs`): printing (`Timeout {…}` is long in node; check first),
   `ref`/`unref`/`hasRef`/`refresh`, `[Symbol.toPrimitive]` (numeric id) and `clearTimeout(id)`;
   `setImmediate`/`clearImmediate`, `process.nextTick` (ordering vs microtasks: check node). Warnings are optional.

**Pitfalls learned in slices 3–5:**
- Wasm `select` picks its **first** operand when the condition is non-zero: three bugs this session came from
  swapped operands. Check every `select`/`typed_select`.
- An `if` with `BlockType::Empty` can't consume stack values; `local_tee` of a nullable local gives a nullable ref
  (add `ref_as_non_null`).
- `cargo fmt --all` reflows code, so later scripted string replacements must match the formatted text.
- Size: anything not always needed is gated. Gates: `ir::Module::names` (user property names/strings listed in
  `builtins::GATED`: add a name there, test with `cx.names("…")`), `has_maps`, `has_tags`, `has_namespaces`,
  `has_bind`, `has_to_string`, `ctor_stubs`. `cx.str(…)` emits a string global even if unused: intern inside the
  gate. The object formatter is linked into every command (uncaught-exception path), so inspect code costs size.
- Prelude: string literals and property names in user code root prelude methods/statics (F109); intrinsics are
  listed in `Lowerer::intrinsic_call`. Scratch `.js` files run as CommonJS in node; wrap fixture sections in `{}`.
- `10 ** k` differs from V8 in last bits (F77): don't use `**` to generate test inputs.

## Handoff for the rest of slice 3 (written 2026-09-26, end of the slices 1–3 session; done, kept for reference)

**State:** `main`, committed, not pushed. Slices 1 (`JSON.parse`) and 2 (`Map`/`Set`, `Map.groupBy`) are done; slice 3
is half done (`bind`, `arguments`: see "Slice 3 (in progress)" above). The oracle is node **24.21.0** (latest LTS,
`TOOLS.md`; `scripts/check-tools.sh` checks it). Commit/push only when the user asks; no attribution lines.
The user watches cost: keep investigations short, record open questions as follow-ups instead of chasing them.

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo test -q --no-fail-fast 2>&1 | grep -E 'FAILED|panicked'   # nothing
scripts/diff.sh -j 4 | tail -1                                   # 76 passed, 0 failed, 11 skipped
```
(`syntax/exponent.js` is skipped until F77: node 24's `**` is V8's own `pow`.)

**Left in slice 3, in this order (smallest first):**
1. **F47, `Function.prototype` as a function:** today it's a plain object from `object.rs` `proto()`. Spec: it's
   callable (returns `undefined`), `typeof Function.prototype === "function"`, `name: ""`, `length: 0`; node prints
   it as `Object [Function] {}`? (check with node). Probably: allocate it as a `$function` with a no-op `$code`
   in `proto()` when `b == Builtin::Function` (its proto is still Object.prototype).
2. **`Function.prototype.toString` (F43's rest):** node returns the exact source slice for user functions/classes/
   methods/accessors (`toString` of a method is `m() { … }`), `function name() { [native code] }` for built-ins
   (also prelude ones and bound functions: `function () { [native code] }`). Plan: `ir::Closure` (made by
   `Lowerer::func_value` and the closure/class/method paths, `crates/lower/src/lib.rs` ~2068) gets a
   `source: Option<String>` (the span's text from `self.p.source`); the backend stores it on the function object
   (a hidden own property under a private symbol, or a `$function` field — decide; watch `func::Helper::New`'s ~25
   callers). Emit sources only when the user's code names a `toString` property or converts functions to strings
   (gate like `ir::Module::bind`, see `builtins::Prelude::bind`), else keep the sentinel/trap. ToPrimitive of a
   function (`"" + f`) goes through it too (today `user_to_primitive_not_implemented`).
3. **F59:** methods/accessors/fields with a computed key whose value is a function are rejected (except
   `[Symbol.iterator]`): SetFunctionName at run time (`name` from the key: a symbol gives `[desc]`, getters
   `get k`). Find the rejection with `grep -n "computed" crates/lower/src/classes.rs crates/lower/src/lib.rs`.

Then the end-of-slice steps (CLAUDE.md): Notes / Result for slice 3 (replace "Slice 3 (in progress)"), tick F43,
F47, F59 in `tasks/README.md`, docs (D5 if the design changes, `docs/keywords.md`, README). Next after slice 3:
slice 4 (built-ins as values: F37, F81, F87, F88, F91, F95, F97, F101) or T20 — the user's choice.

**Pitfalls learned this session:**
- Scratch `.js` files run as CommonJS (sloppy) in node: use `.mjs` or put fixtures in `tests/fixtures` (ESM).
- Anything a program may never use (formatting, `bind`) must be gated, or it grows every binary: check
  `hello.js` (now 52740 bytes) with `target/release/js2wasi3 build tests/fixtures/hello.js` and diff the
  function list (`wasm-tools print … | grep -o '(func \$[^ ]*'`).
- Wasm `select` picks its first operand when the condition is non-zero; an `if` with `BlockType::Empty` can't
  consume stack values (use `BlockType::Result`).
- `cargo fmt --all` after editing Rust (the tree is kept formatted).

## Handoff for slice 1: `JSON.parse` (written 2026-09-26, end of the T21 session)

**State:** `main` at `8dc3c68`, clean, pushed. T21 is done (all four slices; read its Notes / Result, slices 2–4).
Commit/push only when the user asks; no attribution lines (user's global rules).

**Run first (baseline):**
```sh
scripts/check-tools.sh
cargo test -q --no-fail-fast 2>&1 | grep -E 'test result|FAILED'   # all ok
scripts/diff.sh -j 4                                               # 73 passed, 0 failed, 10 skipped
```
Set T25 to `in progress` in `tasks/README.md` and log "slice 1 started" in Progress.

**Read first:** `docs/design.md` D5 (the prelude, T21 slices 1–4), `crates/lower/src/builtins.rs` (prelude
conventions and the reachability scan), `crates/lower/builtins/json.js` (`JSON.stringify`: the file to extend),
`crates/lower/builtins/string.js` top (string intrinsics, `$$toInteger`, `$$StringToNumber`),
`Lowerer::intrinsic_call` in `crates/lower/src/lib.rs` (the list of `__intrinsic_*`).

**What's already in place (no Rust changes should be needed):**
- `JSON.parse(…)` maps to `$$JSON_parse` automatically (`builtins::static_name`: a static `A.b` of an undeclared
  global → `$$A_b`); today it's rejected as `` `JSON.parse` `` by `Prelude::has_statics`. Remove/replace that case
  in `unsupported.rs::builtins_prelude` (it's the example of an unimplemented static: use another, e.g.
  `JSON.rawJSON` or `Math.nope`, which is there already).
- Intrinsics for the parser: `__intrinsic_str_len(s)`, `__intrinsic_unit(s, i)` (a code unit),
  `__intrinsic_sub(s, a, b)` (a new substring: use it for runs without escapes instead of building char by char),
  `__intrinsic_str_new(n)` + `__intrinsic_str_set(r, i, u)` (build a string with escapes),
  `__intrinsic_define(o, k, v)` (CreateDataProperty: `JSON.parse('{"__proto__": 1}')` must make an own key,
  so never `o[k] = v` for parsed members), `__intrinsic_to_string(v)`.
- Numbers: validate the JSON number grammar yourself (node's errors below), then convert the slice with
  `$$StringToNumber` (correctly rounded, same digits as node). Arrays: `[]` + `push` (native, fast).
- `String(text)` first (ToString of the argument: `JSON.parse(1)` is `1`, `JSON.parse(undefined)` throws).

**Spec points:** whitespace is only tab/LF/CR/space; strings reject raw U+0000–U+001F; escapes `\" \\ \/ \b \f \n
\r \t \uXXXX` (hex any case; lone surrogates allowed); duplicate keys: last wins but the key keeps its first
position (`define` on an existing key does that: check); numbers: `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
Reviver (InternalizeJSONProperty): depth-first, holder is `this`, keys are strings (`"0"` for array elements),
array length read once; `undefined` from the reviver deletes the property (`delete`), otherwise
CreateDataProperty; the root is called with key `""` on a wrapper `{"": value}` (as `stringify` does).

**node 20's SyntaxError messages** (probed; reproduce them exactly, position = code-unit index):
```
""          Unexpected end of JSON input            (also "tru", "nul", " ", "[")
"{"         Expected property name or '}' in JSON at position 1      (also "{a:1}")
'{"a":1,}'  Expected double-quoted property name in JSON at position 7
'{"a" 1}'   Expected ':' after property name in JSON at position 5
'{"a":1'    Expected ',' or '}' after property value in JSON at position 6
"[1,]"      Unexpected token ']', "[1,]" is not valid JSON
"'a'"       Unexpected token ''', "'a'" is not valid JSON
"trux"      Unexpected token 'x', "trux" is not valid JSON
'"abc'      Unterminated string in JSON at position 4
"01"        Unexpected number in JSON at position 1
"1."        Unterminated fractional number in JSON at position 2
"-"         No number after minus sign in JSON at position 1
"1e"        Exponent part is missing a number in JSON at position 2
'"\x"'      Bad escaped character in JSON at position 2
'"\u12"'    Bad Unicode escape in JSON at position 5
'"<U+0001>"' Bad control character in string literal in JSON at position 1
"[1] x"     Unexpected non-whitespace character after JSON at position 4
"undefined" "undefined" is not valid JSON
```
Long inputs in the "Unexpected token" form are cut around the position: a 163-unit input gave
`Unexpected token 'x', ..."1,1,1,1,1,x,2,2,2,2,"... is not valid JSON` (10 units each side, `...` on a cut side).
Find V8's exact rule (source length threshold for the short form, what happens near either end) in V8's
`json-parser.cc` (`ReportUnexpectedToken`, `kMaxContextCharacters`, `kMinOriginalSourceLengthForContext`) and
probe a few lengths with node before writing it.

**Pitfalls learned in T21 (prelude code):**
- Prelude code is typed by the same inference as user code: `x.length` is `Any`; write `+x.length` to keep loop
  indices `f64` (otherwise every `<` becomes a slow `$js_less`).
- A `for (;;)` whose only exits are inner `return`s needs a trailing `return` after it, or the result type joins
  `undefined` (`Any`).
- Built-in prototypes' `constructor` (other than Object's; Array's is a sentinel too) and methods not yet
  implemented are sentinels that **trap when read** (F37): don't read them from prelude code.
- Method names read anywhere in prelude code (`.slice`, `.includes`) pull in every prelude method of that name
  (all prototypes): prefer intrinsics in hot helpers.
- BigInt is not supported by the compiler at all: no `1n` in fixtures.
- Don't `pkill -f <pattern>` from a Bash call whose own command line contains the pattern: it kills the shell.
- Speed: calls through values are ~230 ns (F92); a recursive-descent parser with direct calls between prelude
  functions is fine. Check a ~5 MB input against node (T21's `stringify` did 8 MB in 5.3 s vs 0.25 s) and record
  the ratio; `hello.js` must stay 52177 bytes.

**Tests to add:** `tests/fixtures/builtins/json-parse.js` (values, nesting, escapes incl. surrogates, numbers
incl. `-0`/`1e400`/`0.1`, duplicate keys, `__proto__`, reviver: deletion, `this`, order of calls, key types; every
error above printed as `e.name + ": " + e.message` in try/catch; round trip with `stringify`), a `json` case in
`builtins/lib.js` / `build.rs::builtins_in_a_library`, `unsupported.rs` updates. Then the usual end-of-slice steps
(CLAUDE.md): Notes / Result for the slice, tick F93's `JSON.parse` part (the rest of F93 — `JSON` as a value,
`rawJSON` — is slice 4), docs (D5, `docs/keywords.md`, README).

## Progress
- 2026-09-26: slice 1 (`JSON.parse`) started. Baseline: tools ok, cargo test all ok, diff.sh 73 passed / 0 failed / 10 skipped.
- 2026-09-26: `$$JSON_parse` in `builtins/json.js` (recursive descent over code units, parser state in prelude globals `$$jsonSrc/Len/Pos`; `$$jsonSrc` starts `null`: string literals aren't global initializers). node 20's messages probed (incl. V8's quirk: only an object's *first* property gets "Expected ':' after property name", later ones the generic unexpected-token error; context window: sources ≥ 21 units, 10 units each side). `tests/fixtures/builtins/json-parse.js` PASS; `unsupported.rs` now uses `JSON.rawJSON` as the unimplemented static.
- 2026-09-26: library case (`reparse` in `builtins/lib.js`, ok + err via `--invoke`) passes; `hello.js` 52335 bytes, identical to HEAD (handoff's 52177 was stale). 5 MB parse ~15–25× node (F96). cargo test all ok; diff.sh builtins 9 passed. Slice 1 done: Notes / Result, README (F93 partial, F96, Next up), D5, keywords.md, README.md updated.
- 2026-09-26: slice 2 (`Map`/`Set`) started (slice 1 still uncommitted in the working tree).
- 2026-09-26: slice 2 design: native storage `backend/src/mapset.rs` (`$map`/`$mtable`/`$mapiter`, SameValueZero hash table in insertion order, V8-style obsolete-table transitions for live iterators, identity hashes in `class` bits ≥ 12 from `G_ID_HASH`), JS-visible methods in `builtins/map.js` over `__intrinsic_map_*` (`Intrinsic::Map(MapOp)`); `new Map/Set(…)` direct-calls `$$Map/$$Set`, `Map(…)` throws; `instanceof Map`/`Map.prototype` via `Builtin::Map`; `Map` as a value deferred (slice 4); `Map.groupBy` skipped (node 20 lacks it). Smoke test (get/set/has/delete, -0/NaN keys, for-of, spread, forEach) matches node.
- 2026-09-26: slice 2 done: map-set.js PASS, lib `distinct` ok, unsupported.rs updated (WeakMap as the unsupported example), full diff.sh 75/0/10, cargo test ok; hello.js 52534 (+199, types/globals); console.log map printing gated on `ir::Module::maps`; Map/Set iterators print as a trap (was `{}`). F97–F100 recorded.
- 2026-09-26: tools: node oracle pinned to v24.21.0 (latest LTS; was 20.19.2) in TOOLS.md, check-tools.sh, CLAUDE.md. Other tools already latest. With node 24, diff.sh: 70 passed / 5 failed (json-parse: '(line 1 column n)' suffix; array-methods: 'not a function' texts, sort comparator message; map-set: '[object Map Iterator]' receiver; exponent: pow last digits; async/timers: TimeoutNegativeWarning/TimeoutNaNWarning on stderr).
- 2026-09-26: node 24 fix 1/5: JSON.parse position errors get ` (line l column c)` (`$$jsonWhere`: \n, \r\n, \r end lines); a missing ':' is 'Expected ':' after property name' for every property now (node 24 dropped the first-property quirk). json-parse.js PASS on node 24.
- 2026-09-26: node 24 fixes 2–3/5: callbacks' 'X is not a function' use `$$typeDescribe` everywhere (moved to array.js; flatMap too), the sort comparator error appends `: $$describe(v)`; map/set iterators as receivers are '[object Map Iterator]'/'[object Set Iterator]'. array-methods.js (+ a loop over value types) and map-set.js PASS.
- 2026-09-26: node 24 fix 4/5: diff.sh runs node with --no-warnings (warnings carry the pid); timers.js PASS. Map.groupBy added (`$$Map_groupBy`; node's own messages, incl. the plain '5 is not a function'); map-set.js PASS. exponent.js: node 24's pow is bit for bit glibc 2.41's (0 diffs on 20k pairs vs Python math.pow; ARM optimized-routines algorithm), ours (fdlibm) differs on ~10% — F77 updated; porting it needs the user's go (numeric work). F101: Object.groupBy + Set methods.
- 2026-09-26: pow investigation stopped at the user's request (time/credits): node 24 uses V8's own pow (deps/v8/src/base/ieee754.cc), not glibc (666/1M diffs vs glibc 2.41 FMA and non-FMA builds). Recorded in F77; tests/fixtures/syntax/exponent.js skipped (exponent.skip) until F77. F102 recorded: optional Wasm SIMD speed-ups.
- 2026-09-26: slice 3 started (bind first).
- 2026-09-26: bind done natively (func.rs Helper::Bind/BoundCall/BoundConstruct, KIND_BOUND/NEW_BOUND; instanceof unwraps bound targets); smoke test (call, new through bound class, instanceof, name/length, errors, map callback) matches node.
- 2026-09-26: arguments done: a function using it is lowered as f(...all) (params bound in the prologue from all[i], like default params; no call-site change), Intrinsic::MakeArguments -> object class CLASS_ARGUMENTS (indices, hidden length, hidden @@iterator = Array.prototype.values), printed '[Arguments] {…}'; Array.prototype.values now iterates any object as an array-like (iter.rs ARRAY_LIKE). Rejected: arguments in arrows/class ctors, rest param + arguments. Smoke test matches node (strict).
- 2026-09-26: checkpoint: bind + arguments done, functions.js/callee-text.js PASS, full diff.sh 76/0/11, cargo test ok; bind gated on a `bind` property name (hello.js 52740, +206). F42 ticked, F43 partial, F103 recorded.
- 2026-09-26: handoff for the rest of slice 3 written (toString, F47, F59).
- 2026-09-26: rest of slice 3 started (F47, toString, F59).
- 2026-09-26: F47 done: `Function.prototype` is a `$function` (func::Helper::Empty code returns undefined, no construct, KIND_FUNCTION) built in object.rs proto(); console.log appends ' Object' to a function whose proto is Object.prototype (node's constructor name), class 'extends' printing skips Function.prototype. Full diff.sh 76/0/11; hello.js 52851 (+111: inspect checks).
- 2026-09-26: Function.prototype.toString done: `$function` gets field 11 `source` (mut, null = native); ir::Closure::source (lowerer fn_source spans: methods from the key without `static`, accessors from get/set, classes c.span), set after New via func::Helper::SetSource; real toString (func::Helper::ToString) installed and sources emitted only when user code names `toString` (ir::Module::to_string); else sentinel/trap as before. Probes (methods, accessors, static, arrows, class exprs, bound, built-ins, template/String/+ conversion) match node.
- 2026-09-26: F59 done: Intrinsic::SetFunctionName (func::Helper::SetName: prefix + ToString(key), a symbol as [desc], empty desc -> ''), applied to anonymous functions/classes under computed or numeric keys: object literal values/methods/accessors, class methods/accessors, class fields (instance computed field keys stay unsupported). Probe matches node.
- 2026-09-26: slice 3 done: function-source.js fixture PASS, cargo test ok, diff.sh 77/0/11, hello.js 52865. Notes / Result, README (F43, F47, F59 ticked; F104, F105), D5, keywords.md updated.
- 2026-09-26: slice 4 (built-ins as values) started.
- 2026-09-26: 4a step 1: `$$Object` (Object(v)/new Object(v); primitives trap, F88) and `$$Boolean` in object.js; `new Object` direct-calls $$Object. Smoke test matches node.
- 2026-09-26: 4a done: built-in constructors as values (Object/String/Number/Boolean/Array/Map/Set): Intrinsic::BuiltinCtor -> func::Helper::BuiltinCtor (function object over the prelude $$B code; Map/Set calls throw 'requires new', new String/Number/Boolean trap (F88); prototype <-> constructor; prelude statics the code names as own methods; globals G_CTORS). Prelude detects value uses (not called, not `B.x`, not instanceof RHS). Naming `constructor` gives the other prototypes stub constructors (trap when called; +783 bytes instead of +20 KB for full ones). F95 fixed by it. ctor-values.js PASS; hello.js 53024 (+159: 21 global slots).
- 2026-09-26: 4b done: `Math`/`JSON` as values (ir::Namespace, Intrinsic::Namespace -> func::Helper::Namespace: object class CLASS_MATH/CLASS_JSON printed 'Object [Math] {}', Math constants + prelude functions the code names (property names or string literals: `Math[k]`), non-enumerable); Number constants on the Number ctor value; ir::MATH_CONSTANTS/NUMBER_CONSTANTS shared with the lowerer. console.log checks gated on use (ir::Module::namespaces). namespaces.js PASS; hello.js 53038. `[object Math]` (toStringTag) left for slice 5 (F94).
- 2026-09-26: 4c part 1 done: Object.groupBy, the ES2025 Set methods (union/intersection/difference/symmetricDifference/isSubsetOf/isSupersetOf/isDisjointFrom over GetSetRecord, node's messages), __defineGetter__/__defineSetter__/__lookupGetter__/__lookupSetter__ (object.js), Array.prototype.toLocaleString + Number.prototype.toLocaleString (en-US: ≤3 fraction digits, grouping; locales/options trap), Array.prototype.keys/entries natively (iter.rs ARRAY_KEYS/ARRAY_ENTRIES), console.log 'Object [Array Iterator] {}'. Gates: ir::Module::names (user property names in lowering's GATED) links keys/entries and the iterator print only when named. more-methods.js PASS; full diff.sh 80/0/11; hello.js 53043.
- 2026-09-26: 4c part 2 done: Number.prototype.toExponential/toPrecision in string.js ($$exactDigits: exact decimal expansion m·5^-k; $$roundDigits ties up; undefined digits: the shortest digits from Number::toString). number-format.js (edge cases + 300-value LCG sweep, built without ** because of F77) PASS.
- 2026-09-26: 4c part 3 done: String.prototype.localeCompare (ICU root collation for ASCII: ignorable controls, primary order derived from node, then lowercase-first case difference; other chars/locales/options trap; 2000-pair sweep matches), normalize (ASCII for all forms, < U+0300 for NFC; else trap; RangeError text), string literals in user code root prototype methods too (`s[m]()`, F88). locale.js PASS; diff.sh 82/0/11; hello.js 53043.
- 2026-09-26: spread calls of built-in constructors (`Array(...xs)`) and their function properties (`Array.apply`, `String.name`: builtins::FUNCTION_PROPS) go through the constructor value. ctor-values.js PASS.
- 2026-09-26: slice 4 done: Notes / Result, README (F37, F81, F87, F88, F91, F95, F97, F101 ticked; F106–F108; Next up), D5, keywords.md updated.
- 2026-09-26: recorded F109 (built-ins linked only when named in the code) in README, D5, keywords.md, slice 4 notes.
- 2026-09-26: slice 5 (symbols and printing) started.
- 2026-09-26: 5a done (F53 mostly): symbol.js prelude (Symbol.prototype toString/valueOf/description getter, Symbol.for/keyFor over two maps); well-known symbols (ir::WELL_KNOWN_SYMBOLS, Intrinsic::WellKnownSymbol, runtime G_WELL_KNOWN made on first use); Symbol.toPrimitive in ToPrimitive and Symbol.hasInstance in instanceof (gated on the names via ir::Module::names); `string + any` lowers to the run-time + (hint default); Object.getOwnPropertySymbols (object::Helper::OwnSymbols), symbol keys in spread/rest (copy_props) and Object.assign; a symbol == only itself. Left: Symbol() vs Symbol('') (description undefined for both). symbols.js PASS; diff.sh 82/0/11; hello.js 53050.
- 2026-09-26: 5b part 1: Symbol.toStringTag (F94): object::Helper::BuiltinTag (the user's @@toStringTag when `toStringTag` is named, else type checks: Map/Set/iterators, Promise, Generator, Array Iterator, Math/JSON, generator/async functions), used by the native `"" + obj` path (gated on Cx::has_tags: maps/namespaces/promises/generator or async functions/iterator names) and by the prelude's Object.prototype.toString (__intrinsic_builtin_tag); tag properties on the built-in prototypes and Math/JSON when `toStringTag` is named. hello.js 53050; diff.sh 83/0/11.
- 2026-09-26: 5c part 1: console.log: inspect_props (own enumerable string keys, symbol keys as `Symbol(x): v`) shared by objects, arrays (extra properties) and maps/sets (own properties); arrays and maps/sets show at most 100 entries then `... n more items` (node's groupArrayElements tail and number-alignment rules); strings escape like node's strEscape (\\b \\t \\n \\f \\r, \\xHH, \\uxxxx lone surrogates, \\', \\\\) instead of trapping; own_symbols select fix. Trap test example now a nested error. hello.js 54609 (+1559: the formatter is linked into every command through the uncaught-exception path). diff.sh 83/0/11, cargo test ok.
- 2026-09-26: 5c done: cycles (inspect_format = cycle check over G_INSPECT_SEEN/CIRC, then inspect_format_value; `<ref *n>` before the braces via inspect_ref_open, so it counts in the width check like node), Map/Set iterator previews (inspect_map_iterator: a copy of the iterator steps the rest; `{  }` when empty; ≤ 100), user Symbol.toStringTag prefixes `Name [tag] ` (gated on `toStringTag`). inspect-more.js PASS (23 cases); errors nested in objects still trap (no stacks, F31). hello.js 55169 (+2119 over 53050: the formatter is linked into every command via the uncaught-exception path).
- 2026-09-26: 5d done (F23): console.js prelude `$$consoleFormat` (node's formatWithOptions: %s %d %i %f %j %O %o(primitives; objects trap) %c %%), linked only when a user string has a `%` (then console.log calls it when the first of several arguments may be a string); intrinsics inspect(v, depth) (Intrinsic::InspectAt, inspect::Helper::At) and builtin_to_string (hasBuiltInToString). Trap tests (exceptions/trap.js, streamed output) now use `new Number(1)`. console-format.js PASS; cargo test ok; hello.js 55169.
- 2026-09-26: 5e done (F51): KIND_VALUES/NEW_VALUES; one function object for Array.prototype.values and [Symbol.iterator]; Open keeps the fast path only while [Symbol.iterator] is a built-in values (gated on `iterator`); values() builds its state directly. iterator-lookup.js PASS; diff.sh 86/0/11; hello.js 55175.
- 2026-09-26: slice 5 done: Notes / Result, README (F23, F24, F51, F52, F53, F62, F94, F99 ticked; F70 moved to F110; Next up), D5, keywords.md updated.
- 2026-09-26: handoff for slice 6 written.
- 2026-09-26: slice 6 started (user's choice over T20). Baseline: cargo test ok; diff.sh 86/0/11; hello.js 55175.
- 2026-09-26: 6a done (F64 part): ErrorKind::AggregateError (8th kind, Builtin::ALL entry, 3-arg NewError: errors iterated into an own hidden array; length 2); Promise.any (Combinator::Any, Element::AnyRejected, AnyReject builds the error with an own frame-less `stack`) and Promise.withResolvers, both gated on their names (GATED); inspect/describe print a frame-less error as `[AggregateError: …] { [errors]: [...] }` (gated on `any`/AggregateError, lowering notes the name). promises/any.js, any-unhandled.js PASS; diff.sh 88/0/11; hello.js 55204 (+29: three always-declared null globals).
- 2026-09-26: 6b done (F67): Helper::ThrowArgType (NodeError: TypeError + own enumerable `code` + own `toString` `TypeError [ERR_INVALID_ARG_TYPE]: …`, so the uncaught header matches) used by CheckCallback; promise Describe of a function = Function.prototype.toString (the source when kept, F104; else native-code form). promises/arg-type.js, unhandled-function.js PASS; promises+async 21/0.
- 2026-09-26: 6c done (F74): Builtin::Timeout/Immediate prototypes (ref/unref/hasRef, Timeout: refresh, [Symbol.toPrimitive] = id); $timer + delay/id/flags (unref, immediate, cleared); clearTimeout/clearInterval by numeric/string id; setImmediate/clearImmediate (G_IMMEDIATES, run after the due timers, only those queued before the pass); process.nextTick (lowered from `process.nextTick`, G_TICKS, RunTicks = ticks then microtasks until empty; after the ES-module main: microtasks, then ticks); Next ignores unref'd timers for liveness; ToPrimitive's @@toPrimitive lookup also when timers are used. Gates: names $timers/$immediates/$ticks from the lowering. async/timers-more.js, timer-ids.js PASS; diff.sh 92/0/11; unsupported tests + nextTick/setImmediate in libraries. hello.js 55367 (+192 over 55175: 15 more always-declared runtime globals and 2-byte global indices past 127).
- 2026-09-26: slice 6 done: Notes / Result, README (F64, F67, F74, F93 ticked; F111–F114 added; T25 done; Next up T20).
