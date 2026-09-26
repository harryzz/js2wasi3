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
| Keywords (66 entries) | 27 | 11 | 4 | 24 |
| Built-in objects, properties, methods (20) | 7 | 6 | 7 | – |
| Window/HTML, Java, event handlers | the timer functions ✅; `parseInt`/`parseFloat`/`encodeURI*`/`escape`… ❌ | | | the rest ➖ |

What's most needed, in order: the built-in library (`Math`, `Number`, `String`, `Object`/`Array` statics and
prototype methods, `parseInt`…: T21), `arguments` (F42), spread into `console.log` and calls through `console.log`
as a value, `typeof` of an undeclared name, then the language gaps below (async generators, `for await`, tagged
templates, regex, multi-file `import`).

## 1. Keywords

| Word | Status | Notes / what's missing |
|---|---|---|
| `abstract` | ➖ | Removed in ES5 (Java-era reserved word). |
| `arguments` | ❌ | Rejected (F42, T21). Rest parameters work instead. |
| `async` | 🟡 | T19: `async function`, async arrows, methods, static methods. Missing: async generators `async function*` (F72); exported async functions at the WIT boundary (T20). |
| `await` | 🟡 | T19: in async functions and at top level. Missing: `for await` (F72); an unsettled top-level `await` exits 0 instead of node's 13 (F76). |
| `boolean` | ➖ | Removed in ES5. |
| `break` | ✅ | Loops, `switch`, labeled statements and blocks; through `finally`. |
| `byte` | ➖ | Removed in ES5. |
| `case` | ✅ | T24: lazy strict-equality tests, fall-through, TDZ across cases. |
| `catch` | ✅ | With or without a binding, destructuring binding; native Wasm EH (T12). |
| `char` | ➖ | Removed in ES5. |
| `class` | 🟡 | T16: fields, statics, private `#x` fields and methods, `extends` (also native errors), `super`, accessors. Missing: private getters/setters, `static {}` blocks, `accessor` fields, computed instance field keys (F58, F80); methods/accessors with computed keys whose value is a function (F59); `Symbol.hasInstance` (T21); a class used before its declaration throws at build time, not at run time (F63). |
| `const` | ✅ | TDZ checks; writes throw `TypeError: Assignment to constant variable.` (fixed in T24). |
| `continue` | ✅ | Plain and labeled, through `finally`. |
| `debugger` | ✅ | A no-op (T24). |
| `default` | 🟡 | ✅ in `switch` (any position). `export default` only of a named function (T20). |
| `delete` | 🟡 | Properties and array elements work. Missing: `delete o?.a`, `delete` of a private field (a SyntaxError in node anyway) (F80-adjacent, "later"). |
| `do` | ✅ | `do … while`, with `continue`/`break`/labels. |
| `double` | ➖ | Removed in ES5. |
| `else` | ✅ | |
| `enum` | ➖ | Reserved; a syntax error in node too. |
| `eval` | ❌ | Rejected. An ahead-of-time compiler can't compile code built at run time; not planned (would need an interpreter). |
| `export` | 🟡 | Named functions and `export default function f` become WIT exports (T07–T09). Missing: exporting variables/classes, async and generator exports, rest parameters in exports, WIT types beyond `f64`/`string` (T20: F13, F55); library top-level code (F11). |
| `extends` | ✅ | Classes and native errors (T16). |
| `false` | ✅ | |
| `final` | ➖ | Removed in ES5. |
| `finally` | ✅ | Completion dispatch for every jump, also around `yield`/`await` (T12, T18, T19). |
| `float` | ➖ | Removed in ES5. |
| `for` | 🟡 | `for`, `for-in`, `for-of` (iterator protocol, destructuring) work. Missing: `for await` (F72). Arrays/strings iterate without looking up a replaced `Symbol.iterator` (F51). |
| `function` | 🟡 | Declarations, expressions, closures, defaults, rest, destructuring params, generators (`function*`), `call`/`apply`. Missing: `arguments` (F42); `bind`, `toString` (F43); `new.target` outside class constructors (F57); assigning to a function declaration's name (F45). |
| `goto` | ➖ | Removed in ES5; never a JS statement. |
| `if` | ✅ | |
| `implements` | ➖ | Reserved in strict mode; a syntax error in node too. |
| `import` | ❌ | Rejected: single-file programs only (multi-file modules F7; `import.meta` and dynamic `import()` F79). WIT imports as ES imports come with T20. |
| `in` | ✅ | Objects and arrays; `#x in o` (T16). |
| `instanceof` | 🟡 | Classes, functions, native errors, built-ins. Missing: `Symbol.hasInstance` (T21). |
| `int` | ➖ | Removed in ES5. |
| `interface` | ➖ | Reserved in strict mode; a syntax error in node too. |
| `let` | ✅ | Block scope, a fresh binding per loop iteration, TDZ (also across `switch` cases). |
| `long` | ➖ | Removed in ES5. |
| `native` | ➖ | Removed in ES5. |
| `new` | ✅ | Functions, classes, native errors, `Promise`; `new.target` in class constructors (see `function` for the rest). |
| `null` | ✅ | |
| `package` | ➖ | Reserved in strict mode; a syntax error in node too. |
| `private` | ➖ | Reserved in strict mode (JS private members are `#x`, see `class`). |
| `protected` | ➖ | Reserved in strict mode. |
| `public` | ➖ | Reserved in strict mode. |
| `return` | ✅ | Also through `finally`, in generators and async functions. |
| `short` | ➖ | Removed in ES5. |
| `static` | 🟡 | Static fields and methods (T16). Missing: `static {}` blocks (F80). |
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
| `using` | ❌ | Explicit resource management (`using x = …`): rejected (F80). |
| `var` | ✅ | Hoisting, function scope, nested top-level blocks (F28). |
| `void` | ✅ | |
| `volatile` | ➖ | Removed in ES5. |
| `while` | ✅ | |
| `with` | ➖ | Not allowed in strict mode / ES modules: a syntax error in node too. |
| `yield` | ✅ | `yield`, `yield*`, `next`/`return`/`throw`, `finally` on `return()` (T18). |

Other syntax found missing while probing (not on the list): tagged templates and regex literals (F78), BigInt
literals (not scheduled), spread arguments into `console.log(...xs)`, `console.log` used as a value.

## 2. JavaScript objects, properties and methods

| Word | Status | Notes / what's missing |
|---|---|---|
| `Array` | 🟡 | Literals, indexing, `length`, holes, spread, destructuring, iteration, `Array.isArray`, `push`/`pop`/`join`/`toString`/`values`/`[Symbol.iterator]`. Missing: `Array(...)`/`new Array`, `Array.from`/`of`; **other prototype methods (`map`, `forEach`, `slice`, `indexOf`, … ) trap when called** (F49); sparse arrays past 2^26 (F50). |
| `Date` | ❌ | Rejected (T21). |
| `eval` | ❌ | See keywords. |
| `function` | 🟡 | See keywords. |
| `hasOwnProperty` | 🟡 | Traps when called (a built-in sentinel, F37, T21). |
| `Infinity` | ✅ | |
| `isFinite` | ❌ | Rejected (T21). |
| `isNaN` | ❌ | Rejected (T21). |
| `isPrototypeOf` | 🟡 | Traps when called (F37, T21). |
| `length` | ✅ | Strings, arrays, functions. |
| `Math` | ❌ | Rejected (T21). `**` works (T24; last-bit differences vs node on some inexact results: F77). |
| `NaN` | ✅ | |
| `name` | ✅ | Functions, classes, methods, accessors (`get x`), inferred names. |
| `Number` | ❌ | `Number(x)` and the statics are rejected; number methods (`toFixed`, `toString(radix)`, …) trap when called (T21). String → number conversion is rejected or traps (F20). |
| `Object` | 🟡 | `Object.keys`/`values`/`entries`, `defineProperty`, `getOwnPropertyDescriptor`, `freeze`/`seal`/`preventExtensions` and their tests, `getPrototypeOf`/`setPrototypeOf`, `create` (no descriptors). Missing: `assign`, `fromEntries`, `getOwnPropertySymbols`… (F39, F53); `Object(...)` as a function. |
| `prototype` | ✅ | Function and class `prototype`, prototype chains, `Object.create`. |
| `String` | ❌ | `String(x)` is rejected (use `"" + x` or a template); **string methods (`slice`, `indexOf`, …) trap when called** (T21). Concatenation, templates, `length`, indexing and iteration work. |
| `toString` | 🟡 | User `toString` is called by ToPrimitive (T16); arrays' `toString` works. Built-in `toString` of numbers and plain objects traps when called directly (F37, T21); implicit string conversion (`"" + x`, templates) works. |
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
| `setTimeout`, `setInterval`, `clearTimeout`, `clearInterval` | ✅ | T19, on the P3 event loop (commands only; libraries: T20). Timeout objects are plain objects (F74); string delays trap (F20). |
| `constructor` | ✅ | Classes, functions, built-in prototypes. |
| `parseInt`, `parseFloat` | ❌ | Rejected (T21). |
| `decodeURI`, `decodeURIComponent`, `encodeURI`, `encodeURIComponent`, `escape`, `unescape` | ❌ | Rejected (T21). |
| `crypto` | ❌ | Not provided (a WASI `random` import would back `crypto.getRandomValues`; not planned yet). |
