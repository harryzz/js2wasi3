# T42: The global object, and `eval`/`Function` of code known at build time (F151)

Status: todo (handoff written 2026-09-29, not started) | Depends on: T21 (prelude), T25 (built-ins as values), T41 (linker, build-time code only)

## Goal
`globalThis` (and node's alias `global`) is a real object holding the global bindings, and names no scope declares
are looked up on it (reads, writes, `typeof`), as in node. Then `eval` and the `Function` constructor for code that
is **known at build time** (string literals, like T41's `import()`), compiled ahead of time; code only known at run
time stays out (it needs an engine inside the output: F191). A program that uses none of these compiles to the same
bytes as before.

## What we know (at handoff)
- **Every file is an ES module** (strict; top-level `this` is `undefined`; top-level `var`/`function` are not
  global-object properties, as in node's ESM). There is no sloppy mode anywhere; the test262 runner skips `noStrict`
  and `raw` tests.
- **Globals today** (`crates/lower/src/lib.rs`): an identifier with no symbol is either a built-in the lowering knows
  statically (`Math.max(…)` → a direct call of `$$Math_max`, constructors as values through `ctor_values` (T25 slice
  4), prelude `$$` names), one of `NODE_GLOBALS` that isn't implemented → `unsupported: global \`X\`` (planned: T21),
  or an undeclared name → `ReferenceError: x is not defined` at run time (F139, `undeclared`, the helper near
  `NODE_GLOBALS`). `globalThis`, `global`, `eval` and `Function` are in `NODE_GLOBALS`, so they're rejected.
- **test262** (baseline 2026-09-29, tests that don't pass): 216 unsupported ones name `globalThis` (213 of them
  without `eval`/`Function`); 41 include `fnGlobalObject.js` (`Function("return this;")()`); 402 mention `eval` —
  of the 329 that call `eval(…)`, 318 pass only string literals, 15 use indirect eval (`(0, eval)(…)`); 52 call
  `Function(…)`. Where the `eval` tests are: `language/statements/class` + `expressions/class` 107 (PerformEval's
  early errors: `super`, `new.target`, `arguments` in field initializers), `statements/switch` 21, `try` 14,
  `for-in` 12, …: 36 are completion-value tests (`cptn-*.js`: `eval("…")`'s result is the statements' completion
  value). Not checked out yet (not in the subset): `built-ins/global` (29), `built-ins/eval` (10),
  `built-ins/Function` (509), `language/eval-code` (347: direct 286, indirect 61), `language/global-code` (42).
- **Spec points that decide the design**:
  - Direct eval (`eval(x)` where `eval` is the global `%eval%`): in strict code the eval'd code is strict, gets its
    own variable and lexical environments, sees the enclosing bindings, `this`, `super`, `new.target` and the
    function's `arguments`; its result is the completion value; its early errors (and PerformEval's extra ones:
    `super()` outside a derived constructor, `new.target` outside a function, `arguments` in a field initializer)
    are a SyntaxError **thrown at the call**, not a build error. A non-string argument is returned as is.
  - Indirect eval and `Function(…)` run in the **global** scope and are **sloppy** unless the code says `"use
    strict"`: in a sloppy function `this` is the global object when called without one (`fnGlobalObject.js`);
    sloppy indirect eval's `var`/function declarations become global-object properties.
  - The global object's built-in properties are data properties (writable, non-enumerable, configurable; `NaN`,
    `Infinity`, `undefined` read-only). `globalThis.Array === Array`, and replacing one (`globalThis.Array = X`)
    changes what the name `Array` reads.
- **Prior decisions this follows**: T41 compiled `import()` only for modules known at build time and made an
  unresolvable one a build error; F191 (an embedded engine, like scriptc's quickjs "island") is the only way to
  run code that isn't known at build time, and is out of scope here.

## Design questions (decide first, record in docs/design.md, D5 or a new D6)
1. **The global object's representation.** An ordinary `$object` made at startup with the implemented built-ins as
   data properties (values made lazily? spec wants data properties, so `getOwnPropertyDescriptor(globalThis, "Map")`
   must work), plus node's globals we have (`console`, `setTimeout`, `queueMicrotask`, `process`?). Printing
   (`console.log(globalThis)`: node prints `<ref *1> Object [global] {…}`) and `Object.keys(globalThis)` (node's
   list) can be approximate at first. Gating: only when the program names `globalThis`/`global`/`eval`/`Function`
   (or includes code compiled from them).
2. **Static built-ins vs. a replaceable global.** Today `Array`, `Math.max`, … compile to direct references. Options:
   (a) keep them static and detect, at build time, writes that could replace a built-in (`globalThis.X = …`,
   `defineProperty(globalThis, "X", …)`, `delete globalThis.X` with a literal key naming a built-in → those names read
   the global object; a computed key → all of them); (b) always read through the global object when it exists
   (simple, slower, and it defeats the direct calls). Recommendation at handoff: (a).
3. **Undeclared names** when the program has a global object: read → HasProperty/Get on it, else the ReferenceError
   as today; `typeof x` → `"undefined"` if absent; assignment (strict) → Set if the property exists, else
   ReferenceError; `delete x` is a SyntaxError in strict code already.
4. **Direct `eval` of a literal**: where to compile it. (a) In the frontend, splice the code into the source like the
   linker does: `eval("S")` → an arrow IIFE with S's statements (strict eval code gets its own var scope; arrows keep
   `this`/`super`/`new.target`/`arguments`), S first parsed alone as a Script with the caller's context flags to find
   its early errors (then the call becomes a `throw new SyntaxError(…)` expression), and a completion-value rewrite
   (a temp assigned by expression statements; `if`/loops/`switch`/`try`/blocks follow the spec's UpdateEmpty rules;
   the `cptn-*` tests check exactly this). (b) In the lowering, with a nested scope. Recommendation at handoff: (a),
   reusing `frontend::link`'s edits and `SourceMap` (diagnostics inside eval'd code should point at the string).
   Which arguments count as known: string literals, template literals without substitutions, `const`s of those
   (T41's `patterns` without wildcards). `eval` shadowed by a local binding is an ordinary call.
5. **`Function(…)` / `new Function(…)` with literal arguments**: compile at build time as a function in the global
   scope named `anonymous` (its `toString`: `function anonymous(a,b\n) {\nbody\n}`), plus the minimal sloppy mode it
   needs: `this` → the global object when undefined/null (a per-function flag); other sloppy-only semantics (`with`,
   legacy octal, mapped `arguments`, assignment creating a global) rejected or left out. Same for indirect eval of a
   literal (global scope; sloppy `var` → global property: maybe a later slice).
6. **Code not known at build time**: `eval(x)` / `Function(x)` with a computed string is a build error pointing at
   F191; `eval` or `Function` used as a value (`const e = eval`) → decide: a function that throws `EvalError` when
   called with a string (node never does; but a build error would reject harmless code such as `typeof eval`).

## Plan (slices; each ends with fixtures + a targeted test262 run)
1. **Global object**: `globalThis`, `global`, built-ins as its properties (identity with the static ones),
   undeclared names through it, replacement detection (question 2), descriptors; add `built-ins/global` to the
   subset. Fixtures: `globals/global-this.js` (reading/writing/`typeof`/descriptors, a replaced built-in).
2. **Direct `eval` of literals**: splice + completion values + PerformEval's early errors as run-time
   SyntaxErrors; add `language/eval-code/direct` (strict parts) to the subset.
3. **`Function`/`new Function` of literals** (and `fnGlobalObject.js`), sloppy `this`; indirect eval of literals.
4. test262: whole subset (targeted `--update` over every test naming `globalThis`/`global`/`eval`/`Function`, or ask
   before using `a-03` for a full run); docs.

## Read first
- `docs/design.md` D5 (built-ins, the prelude) and the T41 paragraph in D4 (the linker's edits and `SourceMap`).
- `crates/lower/src/lib.rs`: `undeclared`, `NODE_GLOBALS`, the `global \`X\`` rejections, how built-in constructors
  become values (`ctor_values`, `Intrinsic`s for built-in values), `console`/timers globals.
- `crates/lower/src/builtins.rs` (`Prelude::new`: `user_refs`, roots from generated code).
- `crates/frontend/src/link.rs` (edits, `SourceMap`, `is_generated`/`is_inserted`, `patterns`).
- `scripts/test262.mjs` (`skipReason`: `noStrict`/`raw`; `SUBSET`) and `scripts/fetch-test262.sh` (sparse dirs).
- F139, F151, F191, F194 in `tasks/README.md`; T25 slice 4 and T28 Notes / Result (built-ins as values, undeclared
  names).

## Tests
Fixtures in `tests/fixtures/globals/` (commands; node is the oracle via `scripts/diff.sh`), a library case if the
global object is visible from exports, `cargo test`, `wasm-tools validate` + a wasmtime run. Gating: old-vs-new build
of every fixture (byte-identical except F186's nondeterministic three). test262: the tests counted above plus the
new directories; 0 regressions; ask before committing the baseline.

## Acceptance
- [ ] Programs that don't use `globalThis`/`global`/`eval`/`Function` compile to the same bytes.
- [ ] `globalThis` behaves like node's for the implemented built-ins (identity, descriptors, replacement, undeclared
      names through it).
- [ ] `eval("…")` of literals: results (completion values), scope access, run-time SyntaxErrors match node.
- [ ] `Function("…")` of literals works, including `Function("return this")()`.
- [ ] Code not known at build time is a clear build error (F191).
- [ ] test262: the new directories in the subset, results in the baseline, 0 regressions.

## Notes / Result
(to be written)

## Checklist
- [ ] design questions decided and recorded
- [ ] slice 1: global object
- [ ] slice 2: direct `eval` of literals
- [ ] slice 3: `Function` of literals, indirect eval
- [ ] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-29: handoff written after T41 (the user chose F151 next). State at handoff: `main` at `7324f69`, pushed;
  test262 subset 25,911 of 28,107 run pass (92.2%).
