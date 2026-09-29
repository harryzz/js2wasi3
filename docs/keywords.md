# Reserved words: what js2wasi3 supports

Checked 2026-09-26 (after T24) against the lists on
[w3schools: JavaScript Reserved Words](https://www.w3schools.com/JS/js_reserved.asp). Every entry was probed: a
small program built with js2wasi3, run on wasmtime and compared with node (stdout, stderr, exit status).

| Mark | Meaning |
|---|---|
| ✅ | Full: behaves like node in the probes and the fixtures |
| 🟡 | Partial: the common forms work; the listed gaps are rejected at build time (or trap, where said) |
| ❌ | Missing: rejected at build time |
| ➖ | Not applicable: a syntax error in node too (ES modules are strict), or not part of JS for a WASI program |

"Rejected" means a clear `unsupported: … (planned: …)` diagnostic with a source position, never a silent
miscompile. A **trap** (a Wasm crash at run time) is noted where it happens instead. `F<n>` refers to the Follow-ups
table in [`tasks/README.md`](../tasks/README.md).

## Summary

| | ✅ | 🟡 | ❌ | ➖ |
|---|---|---|---|---|
| Keywords (66 entries) | 30 | 12 | 0 | 24 |
| Built-in objects, properties, methods (25) | 11 | 14 | 0 | – |
| Window/HTML, Java, event handlers | the timer functions, `parseInt`/`parseFloat`/`isNaN`/`isFinite` ✅; `encodeURI*`/`escape`… ❌ | | | the rest ➖ |

What's most needed, in order: the rest of the built-in library (`Intl`, BigInt typed arrays), spread into `console.log` and calls through `console.log`
as a value, `typeof` of an undeclared name, then the language gaps below (tagged templates, regex, dynamic
`import()`).

## 1. Keywords

| Word | Status | Notes / what's missing |
|---|---|---|
| `abstract` | ➖ | Removed in ES5 (Java-era reserved word). |
| `arguments` | 🟡 | T25: in ordinary functions, methods, generators and async functions (unmapped, as in strict mode; iterable, printed `[Arguments] {…}`). T43: in arrow functions (the enclosing function's object, also in direct `eval` code). Rejected in class constructors, with a rest parameter and in exported functions (F103). |
| `async` | 🟡 | T19: `async function`, async arrows, methods, static methods. T20: async generators (`async function*`, methods), except `yield*` in them (F115). T20 slice 3: exported async functions are WIT `async func`s. |
| `await` | 🟡 | T19: in async functions and at top level; T20: `for await` (async and sync iterables, node's microtask ticks); T41: top-level `await` in an imported module (node's evaluation order; commands only). Missing: an unsettled top-level `await` exits 0 instead of node's 13 (F76). |
| `boolean` | ➖ | Removed in ES5. |
| `break` | ✅ | Loops, `switch`, labeled statements and blocks; through `finally`. |
| `byte` | ➖ | Removed in ES5. |
| `case` | ✅ | T24: lazy strict-equality tests, fall-through, TDZ across cases. |
| `catch` | ✅ | With or without a binding, destructuring binding; native Wasm EH (T12). |
| `char` | ➖ | Removed in ES5. |
| `class` | 🟡 | T16: fields, statics, private `#x` fields and methods, `extends` (also native errors), `super`, accessors. T31: private getters/setters, computed field keys (evaluated once, in order), `static {}` blocks, `o?.#x`. Missing: `accessor` fields (node 24 lacks them too); a class used before its declaration throws at build time, not at run time (F63). |
| `const` | ✅ | TDZ checks; writes throw `TypeError: Assignment to constant variable.` (fixed in T24). |
| `continue` | ✅ | Plain and labeled, through `finally`. |
| `debugger` | ✅ | A no-op (T24). |
| `default` | 🟡 | ✅ in `switch` (any position). `export default` of anything between modules (T41); a library entry's WIT export only of a named function (T20). |
| `delete` | 🟡 | Properties and array elements work. Missing: `delete o?.a`, `delete` of a private field (a SyntaxError in node anyway) (F80-adjacent, "later"). |
| `do` | ✅ | `do … while`, with `continue`/`break`/labels. |
| `double` | ➖ | Removed in ES5. |
| `else` | ✅ | |
| `enum` | ➖ | Reserved; a syntax error in node too. |
| `eval` | 🟡 | T42: code known at build time (a string literal or a template without substitutions) is compiled where it's called: direct `eval("…")` is strict eval code in the caller's scope (its bindings, `this`, `super`, `new.target`; its result is the completion value; its early errors are a SyntaxError thrown at the call, with oxc's message); indirect `(0, eval)("…")`/`globalThis.eval("…")`/`eval?.("…")` and `Function(…)`/`new Function(…)` of literals run in the global scope (T44: indirect eval code's `var` and function declarations are global-object properties, unless the code is strict). `eval` and `Function` as values work, but calling them with code only known at run time throws an `EvalError` (that needs an engine inside the program: F191); `eval(x)` with a computed `x` is a build error. Missing: sloppy-only syntax in `Function`/indirect code (F204), module bindings named from `Function` code (F206). |
| `export` | 🟡 | The entry's named functions and `export default function f` become WIT exports (T07–T09). T20: `export async function` is an `async func`. T41: between modules every form works (`export const`/`let`/`class`, `export { a as b }`, `export default <expr>`, `export … from`, `export *`, `export * as ns from`); `build --command` builds an entry with exports as a command, as node runs it. Missing: exporting variables/classes/namespaces from a library entry, generator exports, rest parameters in exports (F13, F55, F197). |
| `extends` | ✅ | Classes and native errors (T16). |
| `false` | ✅ | |
| `final` | ➖ | Removed in ES5. |
| `finally` | ✅ | Completion dispatch for every jump, also around `yield`/`await` (T12, T18, T19). |
| `float` | ➖ | Removed in ES5. |
| `for` | ✅ | `for`, `for-in`, `for-of` (iterator protocol, destructuring), `for await` (T20). A replaced or own `Symbol.iterator` of arrays/strings is used (T25). |
| `function` | 🟡 | Declarations, expressions, closures, defaults, rest, destructuring params, generators (`function*`), `call`/`apply`/`bind`, `arguments`, `toString` (source text, T25), names from computed keys (T25). Missing: `"" + f` in code that never names `toString` traps (F104); assigning to a function declaration's name (F45). |
| `goto` | ➖ | Removed in ES5; never a JS statement. |
| `if` | ✅ | |
| `implements` | ➖ | Reserved in strict mode; a syntax error in node too. |
| `import` | 🟡 | T41: other files by relative path (`./x.js`, node's rule: the extension is required), joined into one program at build time: named, default and namespace imports (`import * as ns`, printed `[Module: null prototype] {…}`), live and read-only bindings, cycles, TDZ errors like node's, node's SyntaxError for a missing export (at build time). `import { f } from "ns:pkg/iface"` imports a user world's interface functions (T20 slice 5, `--wit`; see [user-worlds.md](user-worlds.md)). T41 slice 3: top-level `await` in a dependency runs in node's order (async module evaluation), `import.meta` (`url`, `filename`, `dirname`, `main`, `resolve`; the source's paths at build time), JSON modules (`import x from "./x.json" with { type: "json" }`, checked at build time; node's other attribute errors too). T41 slice 4: dynamic `import()` of modules known at build time (string literals, `const`s, `?:`/`||`/`??`, template and `+` patterns matched against the files, `--include <glob>` for the rest; an unknown specifier at run time rejects with node's `ERR_MODULE_NOT_FOUND`, a module that fails to parse or link rejects too; options `{ with: { type: "json" } }`); the load is one job later, node's loader a few more (F199). T41 slice 5: packages (`import x from "pkg"`, `pkg/sub`, `@scope/pkg`, `#imports`) resolved at build time as node does (`node_modules` upwards, `exports` conditions and patterns, legacy `main`). Missing: CommonJS files and packages (F194), node's built-in modules (`node:fs`, `fs`: F200). |
| `in` | ✅ | Objects and arrays; `#x in o` (T16). |
| `instanceof` | ✅ | Classes, functions, native errors, built-ins, `Symbol.hasInstance` (T25). |
| `int` | ➖ | Removed in ES5. |
| `interface` | ➖ | Reserved in strict mode; a syntax error in node too. |
| `let` | ✅ | Block scope, a fresh binding per loop iteration, TDZ (also across `switch` cases). |
| `long` | ➖ | Removed in ES5. |
| `native` | ➖ | Removed in ES5. |
| `new` | ✅ | Functions, classes, native errors, `Promise`; `new.target` in class constructors and ordinary functions and their arrows (T38; `undefined` when called, in methods and in static blocks). |
| `null` | ✅ | |
| `package` | ➖ | Reserved in strict mode; a syntax error in node too. |
| `private` | ➖ | Reserved in strict mode (JS private members are `#x`, see `class`). |
| `protected` | ➖ | Reserved in strict mode. |
| `public` | ➖ | Reserved in strict mode. |
| `return` | ✅ | Also through `finally`, in generators and async functions. |
| `short` | ➖ | Removed in ES5. |
| `static` | ✅ | Static fields and methods (T16), `static {}` blocks (T31). |
| `super` | ✅ | `super(...)`, `super.m()`, `super.x = v` in classes and object-literal methods. |
| `switch` | ✅ | T24. |
| `synchronized` | ➖ | Removed in ES5. |
| `this` | ✅ | Methods, arrows, classes, derived-constructor checks, `call`/`apply`. |
| `throw` | ✅ | Any value; uncaught exceptions print like node and exit 1. |
| `throws` | ➖ | Removed in ES5. |
| `transient` | ➖ | Removed in ES5. |
| `true` | ✅ | |
| `try` | ✅ | |
| `typeof` | 🟡 | All value kinds. Missing: `typeof` of an undeclared name (node: `"undefined"`; js2wasi3 rejects the unknown global). |
| `using` | 🟡 | T45: `using` and `await using` in blocks, functions, static blocks, `switch` blocks, `for`/`for-of` heads and the module's top level (disposal in reverse order at scope exit, `SuppressedError` chains, the spec's awaits), `Symbol.dispose`/`asyncDispose`, `DisposableStack`, `AsyncDisposableStack`, `SuppressedError`. Missing: top-level `using` in a module with imports/exports (rejected); `DisposableStack.prototype[Symbol.dispose]` is a separate function from `dispose` (node: the same). |
| `var` | ✅ | Hoisting, function scope, nested top-level blocks (F28). |
| `void` | ✅ | |
| `volatile` | ➖ | Removed in ES5. |
| `while` | ✅ | |
| `with` | ➖ | Not allowed in strict mode / ES modules: a syntax error in node too. |
| `yield` | ✅ | `yield`, `yield*`, `next`/`return`/`throw`, `finally` on `return()` (T18). |

Other syntax found missing while probing (not on the list): tagged templates (F78; regex literals are done, T27), spread
arguments into `console.log(...xs)`, `console.log` used as a value. BigInt literals and operators work since T40 (see
`BigInt` below).

## 2. JavaScript objects, properties and methods

| Word | Status | Notes / what's missing |
|---|---|---|
| `Array` | 🟡 | Literals, indexing, `length`, holes, spread, destructuring, iteration, `Array.isArray`, `Array(…)`/`new Array(…)`, `Array.from`/`of`, and every prototype method (T21: `map`, `forEach`, `filter`, `reduce`/`reduceRight`, `some`/`every`, `find*`, `indexOf`/`lastIndexOf`/`includes`, `at`, `slice`, `splice`, `concat`, `flat`/`flatMap`, `fill`, `copyWithin`, `reverse`, `shift`/`unshift`, `sort` (stable), `toSorted`/`toReversed`/`toSpliced`/`with`, `push`/`pop`/`join`/`toString`/`values`) and (T25) `keys`/`entries`/`toLocaleString`, `Array` as a value, spread into `Array(…)`. T32: `Object.defineProperty` on elements (accessor elements, non-writable/non-enumerable/non-configurable elements) and on `length` (non-writable `length`, shrinking stops at a non-configurable element), printed like node (`[Getter]`). T33/T34: holes read and writes to absent elements go through the prototype chain (`Array.prototype[i]` getters/setters). T35: `push`/`pop`/`join`/`toString` on any `this`, `Symbol.isConcatSpreadable`, species (`constructor[Symbol.species]`), `Array.fromAsync`, `Array.from` with a constructor `this`. Missing: subclass results (`class X extends Array`, F106, F108); sparse arrays past 2^26 (F50, F160); callbacks are slow (F92). |
| `BigInt` | 🟡 | T40: literals (decimal, hex, octal, binary), `typeof`, every operator (`+ - * / % **`, unary `-`/`~`, `& \| ^ << >>`, `++`/`--`, compound assignment) with V8's errors (mixing with numbers, `+1n`, `>>>`, division by zero, negative exponents, size over 2^30 bits), comparisons with numbers and strings, `==`/`===`/`Object.is`, Map/Set keys, `BigInt(…)`, `asIntN`/`asUintN`, `toString(radix)`/`valueOf`/`toLocaleString` (en-US), `Object(1n)` wrappers, `Number(1n)`, `JSON.stringify` (TypeError, `toJSON`), printed like node (`1n`). Programs without BigInts compile as before. Missing: `BigInt64Array`/`BigUint64Array` (F116), WIT `s64`/`u64` as BigInts (F124), `toLocaleString` with locales or options (needs `Intl`). |
| `Date` | 🟡 | T26: `new Date(…)` (every form; subclasses), `Date()`, `Date.now`/`UTC`/`parse` (ISO and V8's common legacy forms), every getter and setter (local = UTC), `toString`/`toDateString`/`toTimeString`/`toUTCString`/`toGMTString`/`toISOString`/`toJSON`, `valueOf`, `[Symbol.toPrimitive]`, `getYear`/`setYear`, printed like node. Local time is UTC (no time zone in WASI 0.3.1: node with `TZ=UTC` agrees). Missing: `toLocale*String` with locales or options (needs `Intl`), `Date` in a library (the clock import), `toTemporalInstant` (node 24 lacks it). |
| `eval` | 🟡 | See keywords (also `Function(…)`). |
| `function` | 🟡 | See keywords. |
| `hasOwnProperty` | ✅ | T21 (also `propertyIsEnumerable`). |
| `Infinity` | ✅ | |
| `isFinite` | ✅ | T21 (also `Number.isFinite`). |
| `isNaN` | ✅ | T21 (also `Number.isNaN`). |
| `isPrototypeOf` | ✅ | T21. |
| `length` | ✅ | Strings, arrays, functions. |
| `Map` / `Set` | 🟡 | T25: `new Map(iterable)`/`new Set(iterable)`, every method (`get`/`set`/`has`/`delete`/`clear`/`size`/`forEach`/`keys`/`values`/`entries`/`add`), SameValueZero keys, insertion order, iteration that sees changes, `instanceof`, printed like node. Also `Map.groupBy`. Also the ES2025 `Set` methods and `Map`/`Set` as values (T25 slice 4). Missing: subclassing (F106), `WeakMap`/`WeakSet` (F98), printing their iterators (F99). |
| Typed arrays, `ArrayBuffer` | 🟡 | T20: the nine non-BigInt typed arrays (constructors from a length, a buffer, a typed array, an iterable or an array-like; `from`/`of`; element conversions; every `%TypedArray%.prototype` method; `subarray` views; iteration; `Object.keys`/`in`/`for-in`; printed like node) and `ArrayBuffer` (`byteLength`, `slice`, `isView`). Missing: `Float16Array`, BigInt arrays, `DataView`, resizable buffers, subclassing (F116). |
| `Math` | 🟡 | T21: every function and constant, matching node bit for bit except `pow`/`**` (last-bit differences on some inexact results: F77). `Math` (and `JSON`) as values (`const M = Math`, `Math[k]`, printed `Object [Math] {}`, T25; `Math[k]` finds only functions whose name appears in the code, F109); `Math.random` only in commands (F83). |
| `NaN` | ✅ | |
| `name` | ✅ | Functions, classes, methods, accessors (`get x`), inferred names. |
| `Number` | 🟡 | T21: `Number(x)`, string → number (correctly rounded), `isNaN`/`isFinite`/`isInteger`/`isSafeInteger`, `parseInt`/`parseFloat`, the constants, `toString(radix)`, `toFixed`, `toPrecision`, `toExponential` (like node's digits), `toLocaleString` (en-US), `Number` as a value (T25). Missing: `new Number` wrappers (F106), locales/options (F107); exact non-decimal parsing past 2^53 (F89). |
| `Object` | 🟡 | `Object.keys`/`values`/`entries`, `defineProperty`, `getOwnPropertyDescriptor`, `freeze`/`seal`/`preventExtensions` and their tests, `getPrototypeOf`/`setPrototypeOf` (T37), `create` (no descriptors), `assign`, `fromEntries` (T21); `Object.prototype` `toString` (no `Symbol.toStringTag`), `toLocaleString`, `valueOf`, `hasOwnProperty`, `isPrototypeOf`, `propertyIsEnumerable`. T25: `Object(v)`, `Object` as a value, `Object.groupBy`, `__defineGetter__` & co. Missing: symbol keys in `assign` and `getOwnPropertySymbols` (F53); `Object("s")` wrappers (F106); `Object.setPrototypeOf` since T37. |
| `prototype` | ✅ | Function and class `prototype`, prototype chains, `Object.create`. |
| `RegExp` | 🟡 | T27: regex literals and `RegExp(p, f)`/`new RegExp`, flags `dgimsuvy`, `exec`/`test`/`lastIndex`/`toString`/`source`/`flags` and the flag getters, lookaround, backreferences (named, duplicate names), `\p{…}` (Unicode 17), `v` class sets (`&&`, `--`, `\q{…}`), exact case folding for `i`, V8's SyntaxError texts, `RegExp.prototype[Symbol.match/matchAll/replace/search/split]`, printed like node. Missing: species/subclassing and `RegExp(re)` identity (F172), modifiers `(?i:…)` and `RegExp.escape` (F173), `v` properties of strings (F171). T39: linear `String.fromCodePoint`, fast greedy loops in `u` mode, all test262 RegExp timeouts gone. |
| `Reflect` | 🟡 | T36: all 13 functions (`apply`, `construct` with `newTarget`, `defineProperty`, `deleteProperty`, `get`/`set` with a receiver, `getOwnPropertyDescriptor`, `getPrototypeOf`/`setPrototypeOf`, `has`, `isExtensible`/`preventExtensions`, `ownKeys`), node's TypeErrors; `Reflect` as a value (`Object [Reflect] {}`). No `Proxy` (F152). Built-in constructors ignore `newTarget` (F106); a function named in a TypeError shows as `#<Object>`, not its source (F165). |
| `String` | 🟡 | T21: `String(x)`, `String.fromCharCode`/`fromCodePoint`, and `at`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `endsWith`, `includes`, `indexOf`, `lastIndexOf`, `padEnd`/`padStart`, `repeat`, `replace`/`replaceAll` (string patterns), `slice`, `split` (string separators), `startsWith`, `substring`/`substr`, `toLowerCase`/`toUpperCase` (common scripts; others trap), `trim*`, `isWellFormed`/`toWellFormed`, `toString`/`valueOf`; `<`/`>` compare strings. T27: `match`, `matchAll`, `search`, regex `split`/`replace`/`replaceAll` (through `Symbol.*`). T25: `localeCompare` (ICU order for ASCII; others trap), `normalize` (strings no form changes; others trap), `String` as a value. Missing: `new String` (F106), the rest of `localeCompare`/`normalize` (F107). |
| `Symbol` | 🟡 | `Symbol(desc)`, `description`, `toString`, `Symbol.for`/`keyFor`, the well-known symbols (`iterator`, `toPrimitive`, `hasInstance` and `toStringTag` are honoured), symbol keys (`getOwnPropertySymbols`, spread, rest, `assign`, `console.log`) (T25). Missing: `Symbol()` and `Symbol("")` both have `description` `undefined` (F110); `Symbol` as a value (F106). |
| `toString` | 🟡 | User `toString` is called by ToPrimitive (T16); arrays' `toString` works. Numbers' `toString(radix)` works (T21); plain objects' built-in `toString` traps when called directly (F37, T21); implicit string conversion (`"" + x`, templates) works. |
| `undefined` | ✅ | |
| `valueOf` | ✅ | User `valueOf` is called by ToPrimitive (hint-aware). Calling the built-in one directly traps (F37). |

## 3. Java reserved words

`getClass`, `java`, `JavaArray`, `javaClass`, `JavaObject`, `JavaPackage`: ➖ (LiveConnect in old browsers, not
JavaScript).

## 4. Window and HTML words, event handlers

A js2wasi3 program is a WASI 0.3 command or library, not a browser page, so the DOM/window names (`alert`,
`document`, `window`, `navigator`, `location`, `history`, `frames`, `onclick`, `onload`, … all of the page's
"Other Reserved Words" and "HTML Event Handlers") are ➖, as in node. The exceptions, which are ECMAScript or
node/WASI globals:

| Word | Status | Notes |
|---|---|---|
| `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval` | ✅ | T19, on the P3 event loop (commands only; libraries: T20). T25: Timeout objects have `ref`/`unref`/`hasRef`/`refresh`/`[Symbol.toPrimitive]` (the id, which the clear functions also take); printing them differs from node (F112); a string delay traps if nothing else converts to a number (F85). |
| `setImmediate`, `clearImmediate`, `process.nextTick` | ✅ | T25, commands only; `process` itself is not provided (F112). |
| `constructor` | ✅ | Classes, functions, built-in prototypes. |
| `globalThis`, `global` | 🟡 | T42: the global object, made when the code names it: the built-ins the code names are its properties (the same values: `globalThis.Array === Array`, node's descriptors), undeclared names are read, written and `typeof`'d through it. Missing: replacing a built-in (`globalThis.Array = X` is rejected, F201); `console`, `process`, `Symbol`, … aren't on it and it prints as a plain object (F202). |
| `parseInt`, `parseFloat` | ✅ | T21 (also `isNaN`, `isFinite`). Non-decimal `parseInt` past 2^53 isn't correctly rounded (F89). |
| `decodeURI`, `decodeURIComponent`, `encodeURI`, `encodeURIComponent`, `escape`, `unescape` | ❌ | Rejected (T21). |
| `crypto` | ❌ | Not provided (a WASI `random` import would back `crypto.getRandomValues`; not planned yet). |
