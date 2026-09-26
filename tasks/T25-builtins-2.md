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
- [ ] Each slice: fixtures in `tests/fixtures/builtins/` match node (`scripts/diff.sh`), `unsupported.rs` updated.
- [ ] `hello.js` still pulls in no unused built-in (size unchanged, or the growth explained).
- [ ] The follow-ups each slice resolves are ticked in `tasks/README.md`.
- [ ] T25 is done only when every follow-up in the table above is ticked, or moved to another task with the reason
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
