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
| 4 | F37, F81, F87 (`localeCompare`, `normalize`), F88, F91, F95, F97 |
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
- node 20's messages, all reproduced exactly (probed with node 20.19.2): the position form (`… in JSON at position
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
`context` argument (not in node 20); deep nesting recurses (Wasm stack trap, F33); speed (F96).

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

**Unsupported / left:** `Map`/`Set` as values and subclassing (F97, slice 4), `WeakMap`/`WeakSet`, `Map.groupBy`
(not in node 20) (F98), printing iterators / own properties / > 100 entries (F99, slice 5), speed (F100).

**Next smallest step:** slice 3: `Function.prototype.bind` (a bound-function object kind in `func.rs`: target, bound
`this`, bound args; `name` `bound f`, `length`, `new` through it).

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
