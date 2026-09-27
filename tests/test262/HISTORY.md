# test262 run history

One entry per full `subset` run (newest first). The per-test results of the latest run are in `baseline.json`.
pass% = pass / tests run (skips excluded).

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
