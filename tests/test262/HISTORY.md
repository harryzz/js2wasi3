# test262 run history

One entry per full `subset` run (newest first). The per-test results of the latest run are in `baseline.json`.
pass% = pass / tests run (skips excluded).

## 2026-09-30: T51 tagged templates and `String.raw` (targeted run)

- Compiler: tagged templates (template objects per site, frozen, invalid escapes cooked to `undefined`, raw TRV) and
  `String.raw`; programs without one build the same bytes.
- Run on `a-03`: the 1,247 subset tests in `tagged-template`, `template-literal`, `String/raw` or whose source has a
  tagged template or `String.raw`, with `--update` (-j 36, 78 s): 64 unsupported → pass, 4 fail → pass, 1
  unsupported → fail (`cache-eval-inner-function.js`: `eval` code is compiled once, F226), 0 regressions. Subset:
  27,519 of 28,591 run pass (96.25%).

## 2026-09-29: T50 Promise statics per the spec (targeted run)

- Compiler: `Promise.all`/`allSettled`/`any`/`race`/`resolve`/`reject`/`withResolvers`/`try` in the prelude
  (`builtins/promise.js`) for any constructor `this`; `build --unhandled-rejections=none`. Runner: builds with
  `--unhandled-rejections=none` (hosts like d8 don't fail a test on an unhandled rejection; the entry module's
  rejected top-level `await` still fails it, as in node).
- Run on `a-03`: the 6,256 subset tests that mention `Promise` or are async, with `--update` (-j 36, 363 s): 198
  fail → pass, 12 timeout → pass (F187), 2 regressions (`prototype/finally/*-observable-then-calls-PromiseResolve.js`:
  they passed by accident, `finally` ignores species, F225). `Promise/all`, `allSettled`, `race` all pass, `any` 92
  of 94 (`Object.defineProperties`); `built-ins/Promise` 614 of 639. Subset: 27,451 of 28,591 run pass (96.0%).

## 2026-09-29: T49 `Symbol` as a value (full run)

- Compiler: `Symbol` as a value (constructor value, statics, `@@toPrimitive`); `Symbol(x);` as a statement, `new Symbol`,
  `Symbol.for` of a symbol throws; naming `dispose` without a `DisposableStack` no longer fails to build. Runner:
  `built-ins/Symbol` joined the subset (and the sparse clone).
- Run: `built-ins/Symbol` (98: 69 pass, 19 skip; 10 not: F110, F152, F172, F220, F221, module `this`) and the 26 non-passing subset tests using `Symbol` as a value or
  including `testTypedArray.js`, with `--update` (-j 4): 15 unsupported → pass (the 13 `testTypedArray.js` tests,
  2 `class/subclass/builtin-objects/Symbol`), 0 regressions. Subset: 27,243 of 28,591 run pass (95.3%).
- Full run on `a-03` (31,027 tests in 1,592 s, -j 36) confirms it: 27,243 of 28,591 run pass (95.3%); the one change,
  `language/statements/function/S13.2.1_A1_T1.js` pass → timeout, passes alone (4.7 s; slow under load, F185) and
  was set back to pass.

## 2026-09-29: T48 `BigInt64Array`/`BigUint64Array` (targeted run)

- Compiler: the BigInt arrays (kinds 9–10, only in programs naming them). Runner: `HARNESS_ADAPT` no longer hides
  them (`MyBigInt64Array` written out); only `Float16Array` is still reported absent.
- Run: the 96 subset tests naming them or including `testTypedArray.js`/`resizableArrayBufferUtils.js`, with
  `--update` (-j 4): 7 unsupported → pass, 0 regressions; `resizable-arraybuffer` 78 of 89 (11: `typeof Symbol`,
  F218). Subset: 27,159 of 28,512 run pass (95.3%).

## 2026-09-29: T47 resizable `ArrayBuffer` (targeted run)

- Compiler: resizable buffers, length-tracking/out-of-bounds typed arrays, `transfer`/`transferToFixedLength`/
  `detached` (programs without `maxByteLength`/`transfer` in their source build the same bytes); subclasses of
  `Map`/`Set`/`ArrayBuffer`/typed arrays get their prototype. Runner: `HARNESS_ADAPT` rewrites
  `resizableArrayBufferUtils.js` (literal subclasses for `new Function`, Float16/BigInt64 arrays absent) and
  `testTypedArray.js`.
- Run: the 917 subset tests naming `ArrayBuffer`/a typed array/`maxByteLength`/`transfer`/`Reflect.construct`/
  `extends Map|Set`, including those harness files or `isConstructor.js`, or with `class … extends`, with
  `--update` (-j 6): 74 unsupported → pass, 25 fail → pass (`class/subclass-builtins`), 0 regressions; the 89
  `resizable-arraybuffer` tests: 75 pass, 11 blocked by `typeof Symbol` (F218), 3 name `BigInt64Array` (F217).
  (17 `built-ins/Function` tests the list pulled in were left out of the baseline: not in the subset.) Subset:
  27,152 of 28,512 run pass (95.2%).

## 2026-09-29: T46 RegExp modifiers (targeted run)

- Compiler: modifier groups `(?ims-ims:…)`; programs without one build the same bytes.
- Run: the 122 subset tests whose source contains `(?i`/`(?m`/`(?s`/`(?-`, with `--update`: 67 fail → pass, all 122
  pass (6 timed out under load at -j 8, up to ~4 s each alone). Subset: 27,053 of 28,512 run pass (94.9%).

## 2026-09-29: T45 explicit resource management (targeted runs)

- Compiler: `using`/`await using` (source rewrite), `Symbol.dispose`/`asyncDispose`, `SuppressedError`,
  `DisposableStack`, `AsyncDisposableStack`. Subset += `built-ins/DisposableStack`, `built-ins/AsyncDisposableStack`,
  `built-ins/SuppressedError`.
- Runs (`--update`): `language/statements/{using,await-using,for-of}` (100 unsupported → pass), then every subset
  test naming `using`/`SuppressedError`/`DisposableStack`/`Symbol.dispose` plus the new directories (2,760 tests,
  ~11 min at -j 8): 205 new → pass, 0 regressions (one RegExp load timeout passes alone). `using` 69 of 77 run,
  `await-using` 84 of 96, the three built-ins directories 205 of 219 run. Subset: 26,983 of 28,512 run pass (94.6%).

## 2026-09-29: T44 indirect eval declarations (targeted run)

- Compiler: non-strict indirect eval code's `var`/function declarations are global-object properties.
- Run: the subset tests naming `eval` and `language/eval-code`, `built-ins/eval`, `built-ins/global`,
  `language/global-code`, with `--update`: 11 unsupported → pass, 1 fail → pass, 0 regressions; 6 unsupported →
  fail (script-only: top-level `this`/`var`s; one later passed with the strict-code fix).
  `language/eval-code/indirect`: 32 of 59 run (was 21). Subset: 26,677 of 28,296 run pass (94.3%).

## 2026-09-29: T43 `arguments` in arrow functions (targeted run)

- Compiler: arrows capture the enclosing function's `arguments`; captured parameters of a function using
  `arguments` are no longer `undefined` (an older miscompile).
- Run: the 659 subset tests naming `arguments`, with `--update` (~4 min at -j 8): 12 unsupported → pass, 1 fail →
  pass, 0 regressions (4 async-generator tests timed out under load and pass at -j 2). Subset: 26,666 of 28,296 run
  pass (94.2%).

## 2026-09-29: T42 global object, `eval`/`Function` of literals (targeted run)

- Compiler: `globalThis`/`global` (the global object; undeclared names through it; writes to undeclared names throw
  a ReferenceError instead of being rejected), direct `eval` of literals (spliced in place, completion values,
  early errors as run-time SyntaxErrors), `Function`/`new Function`/indirect eval of literals, `eval`/`Function` as
  values; `new.target` in instance field initializers is `undefined`.
- Runner: `globalThis.$DONE = $DONE;` for async tests including `asyncHelpers.js` (a script's function declarations
  are global properties; the harness checks `globalThis.$DONE`). Subset += `built-ins/global`,
  `language/global-code`, `language/eval-code`, `built-ins/eval`.
- Run: every subset test that was unsupported, every test naming `globalThis`/`global`/`eval`/`Function`/
  `new.target` or including `asyncHelpers.js`/`fnGlobalObject.js`, and the four new directories, with `--update`
  (1,695 tests + the directories, ~9 min at -j 8): 0 regressions; 619 unsupported → pass, 41 unsupported → fail
  (EvalError for code only known at run time, sloppy-only code, script `this`, `Promise.try` F207, `ctx-non-ctor`
  F210, `caller` poison pills F208); new directories: `language/eval-code/direct` 62 of 67 run, `indirect` 21 of 59,
  `built-ins/global` 15 of 29, `language/global-code` 17 of 24, `built-ins/eval` 8 of 10.
  Subset: 26,653 of 28,296 run pass (94.2%).

## 2026-09-29: T41 slice 5, packages (targeted run)

- Compiler: bare specifiers resolved from `node_modules` (test262 has no packages; bare specifiers there name missing
  modules, still errors).
- Run: `language/module-code`, `language/import`, `language/export`, `language/expressions/dynamic-import` and the 17
  other subset tests naming a bare specifier, with `--update` (2,031 tests, ~7 min at -j 4): no changes. Subset:
  25,911 of 28,107 run pass (92.2%).

## 2026-09-29: T41 multi-file modules, slice 4 (targeted run)

- Compiler: dynamic `import()` of modules known at build time (patterns, `--include`); modules only `import()`
  loads that fail to parse or link reject at run time.
- Runner: a test with `import()` is built with `--include <its file>` and, when it names `./…_FIXTURE.js` files,
  `--include <dir>/*_FIXTURE.js` (the modules a run-time specifier may name); those fixtures are linked next to it.
- Run: the module directories and the 1,035 other subset tests with `import`/`export`/`import(`/`import.meta`/
  attributes, with `--update` (1,984 tests, ~7 min at -j 4): 0 regressions, 330 → pass. `dynamic-import`: 454 of 548
  run (82.8%); still failing: 20 syntax tests whose `import('')` rejects unhandled (node exits 1 too), `unary-expr`
  (a specifier that is `true`: a package name, rejected as in node), 2 namespace `defineProperty` (F193); unsupported: `globalThis`
  (53), assignment to a function binding (18). Subset: 25,911 of 28,107 run pass (92.2%).

## 2026-09-28: T41 multi-file modules, slice 3 (targeted run)

- Compiler: top-level `await` in dependencies in the spec's order (module functions + `$$EvaluateModules`),
  `import.meta`, JSON modules. Single-file programs and multi-file ones without an async dependency build
  byte-identical modules (checked on every fixture).
- Runner: `import-text` and `import-bytes` skipped (node 24 lacks them: 10 tests).
- Run: `language/module-code`, `language/import`, `language/export` and the 402 other subset tests with
  `import`/`export`/`import.meta`/attributes, with `--update` (1,351 tests, ~4.5 min at -j 4): 0 regressions, 25
  unsupported → pass (JSON modules 11, attribute syntax 9, `import.meta` 5), 10 → skip, `import.meta/syntax/goal-script.js`
  unsupported → fail (it wants a script-goal SyntaxError; every file is a module here). test262's TLA-order tests
  still need `globalThis`/`import()`; three checked by hand match node. Subset: 25,581 of 28,108 run pass (91.0%);
  module-code 539 of 593 (90.9%).

## 2026-09-28: T41 multi-file modules, slices 1–2 (targeted run; subset + `language/module-code`, `language/import`, `language/export`)

- Compiler: T41 slices 1–2 (modules joined at the source level: static imports and exports, re-exports, `export *`,
  namespace objects, read-only imports; `build --command`). Single-file programs build byte-identical modules
  (checked on every fixture), so only tests with `import`/`export` can change.
- Runner: tests importing `./` files are compiled under their own name next to (symlinks of) the files they import;
  resolution-phase negatives run (a build error passes); every test is built with `--command`.
- Subset change: `language/module-code` (755 files), `language/import` (191), `language/export` (3) added: 537 pass,
  8 fail (namespace internals in the TDZ and `defineProperty`, F193; a test importing another test, F196), 77
  unsupported (import attributes 31, `Function` 19, `globalThis` 12, dynamic `import()` 10), 327 skipped (`import
  defer`/`source`, fixtures, `$262`).
- Run: those directories and the 1,032 subset tests containing `import`/`export`, with `--update` (1,981 tests, ~5
  min at -j 4): 0 regressions, 1 unsupported → pass. Subset: 25,556 of 28,118 run pass (90.9%; the old subset
  25,019 of 27,496).

## 2026-09-28: T40 BigInt (targeted run; subset + `built-ins/BigInt`)

- Compiler: T40 (BigInt values, operators, `BigInt(…)` and its methods, gated so programs without BigInts compile to
  the same bytes; `Object.is`).
- Subset change: `built-ins/BigInt` added (77 files: 75 pass, 1 fail (`prop-desc.js`: the global object), 1 skip).
- Run: every test whose compiled output can differ, with `--update` (the other tests build byte-identical modules:
  checked on the fixtures): `built-ins/BigInt`, the 267 baseline-"unsupported" subset tests mentioning BigInt, the
  96 including a harness file that names `BigInt`, the 4 using `Object.is` (400 tests, ~3 min at -j 4): 0
  regressions; 205 unsupported → pass; still unsupported for other reasons: `Function` (122), `Float16Array`,
  `globalThis`, BigInt typed arrays (F116), dynamic `import`; 4 time out (a pre-existing `Promise.all` hang,
  F187). Subset: 25,018 of 27,496 run pass (91.0%).

## 2026-09-28: T26 `Date` (full subset run on a-03; subset + `built-ins/Date`)

- Compiler: T26 (`Date` in the prelude, local time = UTC; constructor values inherit `Object.prototype` methods;
  `Math.random` through a value in a library throws). Runner: `Temporal` added to NODE_MISSING.
- Subset change: `built-ins/Date` added (594 files: 579 pass, 4 fail (top-level `this`, F151), 11 skip).
- Run: the whole subset with `--update` on `a-03` (Xeon Gold 6240, 72 threads, WSL2; 29,256 tests in 1,365 s at
  -j 36; the laptop takes ~3 h at -j 4, so timeouts may differ): 0 regressions; 36 unsupported → pass, 2
  unsupported → fail (F172, F151). Subset: 24,738 of 27,420 run pass (90.2%).

## 2026-09-28: T39 RegExp and string-building speed (full subset run)

- Compiler: T39 (linear `String.fromCodePoint`; `STAR` fast path in `u`/`v` mode; unboxed regex helpers; unanchored
  searches skip positions the first instruction rejects).
- Runs: `built-ins/RegExp` + `built-ins/String/fromCodePoint` (1,890 tests, 1,166 s at -j 4): all 447 former timeouts
  → pass, 0 regressions; then the whole subset with `--update` (28,662 tests, 10,372 s at -j 4): the same 447, and
  `language/statements/function/S13.2.1_A1_T1.js` pass → timeout under load (same wasm bytes as before; 4–5 s alone;
  rerun alone: pass, F180). Subset: 24,123 of 26,837 run pass (89.9%); `built-ins/RegExp` 1,635 pass, 0 timeouts.

## 2026-09-28: T27 `RegExp` (full subset run; subset + `built-ins/RegExp`)

- Compiler: T27 (`RegExp` and regex literals: backtracking engine in the prelude, string methods through `Symbol.*`,
  `\p{…}`, `v` class sets).
- Subset change: `built-ins/RegExp` added. Subset: 23,676 of 26,837 run pass (88.2%); `built-ins/RegExp` 1,188 pass,
  447 time out (441 `property-escapes/generated`, 6 `CharacterClassEscapes`: F170).

## 2026-09-28: T38 `new.target` in functions, `super` with a `null` base (targeted rerun)

- Compiler: T38 (`new.target` in ordinary functions and arrows; `super.x` throws for a `null` home prototype and reads
  the base before converting the key).
- Runs: the 68 non-passing subset tests mentioning `new.target`: 38 → pass (the rest mostly `eval`); the 5 F168 tests →
  pass; regression checks `language/expressions/{function,arrow-function,new,super,call,new.target}`,
  `language/statements/{function,class/subclass,class/super}`, `language/expressions/object/method-definition`
  (~1,900 tests): 2 more pass, 0 regressions. Subset: 22,211 of 24,973 run pass (88.9%).

## 2026-09-28: T37 `Object.setPrototypeOf` (targeted rerun)

- Compiler: T37 (`Object.setPrototypeOf`; `Object.prototype` methods read from namespaces).
- Runs: the 35 non-passing subset tests mentioning `setPrototypeOf`: 19 → pass, 5 → fail (super references, F168);
  `built-ins/Reflect/enumerate/undefined.js` → pass; regression check `built-ins/{Math,JSON,Reflect}` (645 tests): no
  changes. Subset: 22,166 of 24,973 run pass (88.8%).

## 2026-09-28: T36 `Reflect` (targeted reruns; subset + `built-ins/Reflect`)

- Compiler: T36 (`Reflect` namespace, all 13 functions in the prelude; built-in functions as values aren't
  constructors).
- Subset change: `built-ins/Reflect` added (153 files: 135 pass, 1 fail, 17 unsupported).
- Runs: the 239 subset tests using `Reflect` (source or `isConstructor.js`/`deepEqual.js`/… harness) that didn't
  pass: 179 → pass; then every `built-ins/*` directory of the old subset (6,116 tests, 2,201 s at -j 4) as a
  regression check for the constructor change: 1 fail → pass, 0 regressions.
  Subset: 22,146 of 24,973 run pass (88.7%; was 21,831 of 24,820, 88.0%).

## 2026-09-27: T35 arrays: generic methods, `isConcatSpreadable`, species, `fromAsync` (targeted rerun)

- Compiler: T35 (the native `push`/`pop`/`join`/`toString` fall back to the prelude's generic algorithms; `concat`'s
  IsConcatSpreadable; ArraySpeciesCreate; `Array.fromAsync`; `Array.from` with a constructor `this`).
- Run: `built-ins/Array`, `language/statements/for-of` (3,833 tests, 1,244 s at -j 4): 105 → pass, 0 regressions.
  Subset: 21,831 of 24,820 run pass (88.0%); `built-ins/Array` 2642 of 3030 (87.2%).

## 2026-09-27: T34 array writes through the prototype chain (targeted rerun)

- Compiler: T34 ([[Set]] on absent elements walks the prototype chain behind a flag; CreateDataProperty for elements;
  the prelude defines its own arrays).
- Run: `built-ins/{Array,JSON,String,Math,Map,Set}`, `language/statements/for-of`,
  `language/expressions/{array,assignment/dstr}` (6,555 tests): 6 fail → pass, 1 pass → fail (fixed; `sort`/`toSorted`
  rerun: 2 → pass). Subset: 21,726 of 24,820 run pass (87.5%); `built-ins/Array` 2537 of 3030 (83.7%).

## 2026-09-27: T33 holes through the prototype chain (targeted rerun)

- Compiler: T33 (native `join`/`pop` and array iteration read holes with `Get`).
- Run: `built-ins/Array/prototype/{pop,join,toString,shift,unshift}`, `built-ins/Array/from`,
  `language/statements/{for-of,for}`, `language/expressions/array`, `language/expressions/assignment/dstr`,
  `language/statements/const/dstr` (1,795 tests): 3 fail → pass, 0 regressions. Subset: 21,719 of 24,820 run pass
  (87.5%); `built-ins/Array` 2532 of 3030 (83.6%).

## 2026-09-27: T32 `defineProperty` on arrays (targeted rerun)

- Compiler: T32 (element attributes and accessor elements on `$array`, ArraySetLength, non-writable `length`).
- Run: all of `built-ins/Array`, `built-ins/JSON`, `language/statements/for-of`, `language/expressions/array` (4,050
  tests, 1,299 s at -j 4): 237 fail → pass, 0 regressions; then `built-ins/Array/prototype/{push,pop}` again: 4 more.

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2529 | 171 | 319 | 0 | 11 | 52 | 83.5% |
| built-ins/JSON | 109 | 19 | 34 | 0 | 0 | 3 | 67.3% |
| built-ins/Map | 134 | 10 | 23 | 0 | 2 | 35 | 79.3% |
| built-ins/Math | 276 | 2 | 39 | 0 | 0 | 10 | 87.1% |
| built-ins/Promise | 406 | 186 | 39 | 0 | 8 | 93 | 63.5% |
| built-ins/Set | 311 | 16 | 53 | 0 | 1 | 2 | 81.6% |
| built-ins/String | 890 | 101 | 226 | 0 | 0 | 6 | 73.1% |
| language/expressions | 8843 | 165 | 1067 | 0 | 0 | 1089 | 87.8% |
| language/statements | 8218 | 107 | 504 | 0 | 1 | 520 | 93.1% |
| **total** | **21716** | **777** | **2304** | **0** | **23** | **1810** | **87.5%** |

## 2026-09-27: T31 class features (targeted rerun)

- Compiler: T31 (private getters/setters, computed class field keys, `static {}` blocks, `o?.#x`; ToPropertyKey of
  objects through ToPrimitive). Runner: `nonextensible-applies-to-private` skipped (not in node 24).
- Run: the 977 non-passing tests under `language/{statements,expressions}/class`, plus
  `language/expressions/{optional-chaining,assignment/dstr,object}` and `language/statements/class/subclass` for
  regressions (2,618 tests, 855 s at -j 4): 563 → pass, 2 → skip, 2 → fail (`arguments` in a parameter default,
  F158), 0 regressions.

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2290 | 410 | 319 | 0 | 11 | 52 | 75.6% |
| built-ins/JSON | 108 | 20 | 34 | 0 | 0 | 3 | 66.7% |
| built-ins/Map | 134 | 10 | 23 | 0 | 2 | 35 | 79.3% |
| built-ins/Math | 276 | 2 | 39 | 0 | 0 | 10 | 87.1% |
| built-ins/Promise | 406 | 186 | 39 | 0 | 8 | 93 | 63.5% |
| built-ins/Set | 311 | 16 | 53 | 0 | 1 | 2 | 81.6% |
| built-ins/String | 890 | 101 | 226 | 0 | 0 | 6 | 73.1% |
| language/expressions | 8843 | 165 | 1067 | 0 | 0 | 1089 | 87.8% |
| language/statements | 8217 | 108 | 504 | 0 | 1 | 520 | 93.1% |
| **total** | **21475** | **1018** | **2304** | **0** | **23** | **1810** | **86.5%** |

## 2026-09-27: T30 wrapper objects (targeted rerun)

- Compiler: T30 (`new String/Number/Boolean`, `Object(prim)`, ToObject for `Array.prototype` methods, unwrapping in
  ToPrimitive, this-value checks, `JSON.stringify`, `Object.prototype.toString`, `console.log`).
- Run: the 765 non-passing tests that failed on a wrapper trap or use wrappers in their source (331 s at -j 4):
  745 → pass, 0 regressions. The 20 left: native `join`/`pop`/`push` on non-arrays and `Symbol.isConcatSpreadable`
  (F108), `String.prototype.match`/`search` (RegExp, T27).

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2290 | 410 | 319 | 0 | 11 | 52 | 75.6% |
| built-ins/JSON | 108 | 20 | 34 | 0 | 0 | 3 | 66.7% |
| built-ins/Map | 134 | 10 | 23 | 0 | 2 | 35 | 79.3% |
| built-ins/Math | 276 | 2 | 39 | 0 | 0 | 10 | 87.1% |
| built-ins/Promise | 406 | 186 | 39 | 0 | 8 | 93 | 63.5% |
| built-ins/Set | 311 | 16 | 53 | 0 | 1 | 2 | 81.6% |
| built-ins/String | 890 | 101 | 226 | 0 | 0 | 6 | 73.1% |
| language/expressions | 8585 | 165 | 1325 | 0 | 0 | 1089 | 85.2% |
| language/statements | 7912 | 106 | 813 | 0 | 1 | 518 | 89.6% |
| **total** | **20912** | **1016** | **2871** | **0** | **23** | **1808** | **84.2%** |

## 2026-09-27: T29 `yield*` in async generators (targeted rerun)

- Compiler: T29 (`yield*` in async generators, CreateAsyncFromSyncIterator in the prelude).
- Run: the 1,128 tests rejected for `yield*` in an async generator (606 s at -j 4): 1,127 → pass, 1 → fail
  (needs the AsyncGeneratorFunction constructor, F106), 0 regressions.

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2194 | 506 | 319 | 0 | 11 | 52 | 72.4% |
| built-ins/JSON | 96 | 32 | 34 | 0 | 0 | 3 | 59.3% |
| built-ins/Map | 134 | 10 | 23 | 0 | 2 | 35 | 79.3% |
| built-ins/Math | 276 | 2 | 39 | 0 | 0 | 10 | 87.1% |
| built-ins/Promise | 406 | 186 | 39 | 0 | 8 | 93 | 63.5% |
| built-ins/Set | 311 | 16 | 53 | 0 | 1 | 2 | 81.6% |
| built-ins/String | 697 | 294 | 226 | 0 | 0 | 6 | 57.3% |
| language/expressions | 8163 | 587 | 1325 | 0 | 0 | 1089 | 81.0% |
| language/statements | 7890 | 128 | 813 | 0 | 1 | 518 | 89.3% |
| **total** | **20167** | **1761** | **2871** | **0** | **23** | **1808** | **81.2%** |

## 2026-09-27: T28 test262 quick wins (targeted rerun)

- Compiler: T28 (undeclared reads throw a ReferenceError, `await using` reported, WIT names from file names,
  `Object.getOwnPropertyNames`, built-in statics as values for `propertyHelper.js`).
- Run: the 4,816 non-passing tests whose reason was a fixed cause (1,795 s at -j 4), then 16 more after a fix.
  3,136 → pass, 9 crash → unsupported, 0 regressions.

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2194 | 506 | 319 | 0 | 11 | 52 | 72.4% |
| built-ins/JSON | 96 | 32 | 34 | 0 | 0 | 3 | 59.3% |
| built-ins/Map | 134 | 10 | 23 | 0 | 2 | 35 | 79.3% |
| built-ins/Math | 276 | 2 | 39 | 0 | 0 | 10 | 87.1% |
| built-ins/Promise | 406 | 186 | 39 | 0 | 8 | 93 | 63.5% |
| built-ins/Set | 311 | 16 | 53 | 0 | 1 | 2 | 81.6% |
| built-ins/String | 697 | 294 | 226 | 0 | 0 | 6 | 57.3% |
| language/expressions | 7720 | 587 | 1768 | 0 | 0 | 1089 | 76.6% |
| language/statements | 7206 | 127 | 1498 | 0 | 1 | 518 | 81.6% |
| **total** | **19040** | **1760** | **3999** | **0** | **23** | **1808** | **76.7%** |

## 2026-09-27: reclassification, no new run (T23 slice 3)

- Runner: tests using features node 24 lacks are skipped (proper tail calls, `upsert`, `Math.sumPrecise`, …): 554
  tests rerun, now skips. Compiler unchanged.
- Baseline totals: 15,904 pass, 1,682 fail, 7,205 unsupported, 9 crash, 22 timeout, 1,808 skip: **64.1%** of 24,822 run.
- Causes → follow-ups: T23's Notes / Result, slice 3.

## 2026-09-27: first subset run (T23 slice 2)

- Compiler: `b933c92` "T23 slice 1: test262 runner" (compiler crates unchanged since `bae9723`).
- Results committed in: `b1b729d` "T23 slice 2: first test262 subset run and baseline" (2026-09-27 17:21 +0300).
- test262: `7ab7fafa0003f73fc85c1b95d88094d33f7eb8bd`; wasmtime 49.0.1; node 24.21.0.
- Run: `node scripts/test262.mjs --update --causes 25 subset`, 26,630 files, 6,500 s at -j 4 (i7-8565U, WSL2);
  then 130 tests rerun after two runner fixes (script-only tests → skip; plain temp file names, F143).

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2047 | 494 | 478 | 0 | 11 | 52 | 67.6% |
| built-ins/JSON | 87 | 22 | 53 | 0 | 0 | 3 | 53.7% |
| built-ins/Map | 101 | 25 | 74 | 0 | 2 | 2 | 50.0% |
| built-ins/Math | 162 | 0 | 165 | 0 | 0 | 0 | 49.5% |
| built-ins/Promise | 367 | 181 | 173 | 0 | 7 | 4 | 50.4% |
| built-ins/Set | 260 | 13 | 107 | 0 | 1 | 2 | 68.2% |
| built-ins/String | 616 | 278 | 323 | 0 | 0 | 6 | 50.6% |
| language/expressions | 6642 | 593 | 3191 | 0 | 0 | 738 | 63.7% |
| language/statements | 5840 | 139 | 2861 | 9 | 1 | 500 | 66.0% |
| **total** | **16122** | **1745** | **7425** | **9** | **22** | **1307** | **63.7%** |

Top causes: unsupported global `Array` (2,302, mostly propertyHelper), `yield*` in async generators (1,050), undeclared
globals (~1,200, F139), wrapper-object traps (722, F106), dynamic `import()` (469), `eval` (400). Details: T23's Notes / Result.
