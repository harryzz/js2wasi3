# T42: The global object, and `eval`/`Function` of code known at build time (F151)

Status: done (2026-09-29) | Depends on: T21 (prelude), T25 (built-ins as values), T41 (linker, build-time code only)

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
- [x] Programs that don't use `globalThis`/`global`/`eval`/`Function` compile to the same bytes (every fixture; F186's nondeterministic ones aside).
- [x] `globalThis` behaves like node's for the implemented built-ins (identity, descriptors, replacement, undeclared
      names through it). Replacing a built-in is rejected at build time (F201).
- [x] `eval("…")` of literals: results (completion values), scope access, run-time SyntaxErrors match node.
- [x] `Function("…")` of literals works, including `Function("return this")()`.
- [x] Code not known at build time is a clear build error (F191).
- [x] test262: the new directories in the subset, results in the baseline, 0 regressions.

## Notes / Result
**What changed** (design: `docs/design.md` D6)
- Global object (slice 1): `crates/lower/builtins/global.js` (`$$GlobalObject` made on first use, `$$GlobalGet`/
  `Read`/`Set`, `$$eval`, `$$Function`); the prelude scan (`builtins.rs`) sets `Prelude::global_object` when the
  user's code names `globalThis`/`global`, lists `global_props` (the `GLOBAL_NAMES` the code names, made values and
  linked) and `replaced` (writes replacing a built-in through `globalThis`/`global`: rejected); the lowering expands
  `__intrinsic_global_init` (`global_value`), reads `globalThis`/`global` as `$$GlobalObject()`, and reads, writes,
  `typeof`s and `++`s undeclared names through the global object (`global_target`/`global_read`/`global_write`/
  `global_assign`/`global_update`); without a global object, writes to undeclared names throw node's ReferenceError
  after the right side (F154), writes to `NaN`/`Infinity`/`undefined` node's TypeError.
- `eval`/`Function` (slices 2–3): `crates/frontend/src/eval.rs`, called by the linker's `load` for every module:
  direct eval, `Function`/`new Function` and indirect eval of literals spliced in place (see D6), `EvalMap` for
  diagnostics (`Program::original`/`evals`, `SourceMap::evals`); a single file with evals goes through `join`.
  `eval` and `Function` are values (`CTOR_VALUES` += `Function`; `GLOBAL_NAMES` += `eval`).
- Fixed on the way: `new.target` in instance field initializers (and their arrows) is `undefined` (closures.rs);
  TDZ flag globals of one pattern were allocated in `HashMap` order (a cause of F186: `modules/json/main.js`).
- test262 runner: `globalThis.$DONE = $DONE;` for async tests including `asyncHelpers.js`; subset +=
  `built-ins/global`, `language/global-code`, `language/eval-code`, `built-ins/eval` (and `fetch-test262.sh` fetches
  them and `built-ins/Function`).

**Tests**: fixtures `tests/fixtures/globals/{global-this,eval-direct,function-ctor}.js` (node is the oracle; all
pass `scripts/diff.sh`); `unsupported.rs::global_object_replacement`; `eval.rs` unit test `offsets_map_back`;
`build.rs` (its unsupported example is now `Proxy`); `cargo test` ok. Gating: old vs new compiler over every
fixture: identical except F186's nondeterministic ones. test262 targeted run with `--update` (see
`tests/test262/HISTORY.md`): 0 regressions, 619 unsupported → pass; subset 26,653 of 28,296 run (94.2%, was 92.2%).

**Unsupported / left** (follow-ups): replacing built-ins F201; global object completeness and printing F202;
`arguments` in eval code F203; sloppy `Function`/indirect code F204; indirect eval `var`s F205; module bindings
named from `Function` code F206; `Promise.try` F207; `Function` details (poison pills, `Function.prototype.constructor`,
subclassing) F208; `built-ins/Function` in the subset F209; Promise statics' constructor check F210; `const`
strings as eval code, V8's SyntaxError messages F211; code only known at run time F191.

**Next smallest step**: F205 (global `var` bindings from indirect eval: 24 tests) or F203 (`arguments` in arrow
functions, which also unblocks eval code using `arguments`).

## Checklist
- [x] design questions decided and recorded (D6)
- [x] slice 1: global object
- [x] slice 2: direct `eval` of literals
- [x] slice 3: `Function` of literals, indirect eval
- [x] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-29: handoff written after T41 (the user chose F151 next). State at handoff: `main` at `7324f69`, pushed;
  test262 subset 25,911 of 28,107 run pass (92.2%).
- 2026-09-29: session start. Tools unchanged (`check-tools.sh` ok). Recount of the baseline (status ≠ pass/skip):
  only 12 tests are blocked by `globalThis` alone (the handoff's 216 included skipped ones: `import-defer` etc.);
  `eval(` 402 unsupported (235 pass only literals), `Function`/`fnGlobalObject` 88. So slice 2 (eval) pays most in
  the current subset; slice 1 mostly pays in the new directories (`built-ins/global`, `language/global-code`).
- 2026-09-29: design decided and recorded as D6 in `docs/design.md`: handoff recommendations (static built-ins +
  replacement detection, splice for eval); refinements: the replacement check keys on the *property name* written
  anywhere (covers aliases of `globalThis`), the global object holds only the built-ins the code names (prelude
  rule), writes to undeclared names throw a ReferenceError at run time even without a global object (were
  "unsupported"), `eval`/`Function` as values throw an `EvalError` for strings.
- 2026-09-29: slice 1 core: `builtins/global.js` (`$$GlobalObject` made on first use, `$$GlobalGet/Read/Set`),
  scan (`Prelude::global_object`, `global_props` = `GLOBAL_NAMES` the code names, made values/rooted),
  `__intrinsic_global_init` expanded by the lowering (`global_value`: namespaces, error/built-in ctors, Promise,
  timers, `$$parseInt`…; node's own globals enumerable), undeclared reads/`typeof`/`=`/`op=`/`++` through it
  (`global_target`, `global_assign`, `global_update`; reuses `generators.rs`'s `prelude_call`); writes to
  `NaN`/`Infinity`/`undefined` throw node's TypeError; undeclared writes without a global object throw the
  ReferenceError (were unsupported). Fixture `globals/global-this.js` matches node; a library export using
  `globalThis` works (`--invoke`).
- 2026-09-29: gating check (old vs new compiler, every fixture): only `generators/basics.js` differs (F186). Found
  and fixed one more F186-style order: TDZ flag globals were allocated in `HashMap` order for bindings of one
  pattern (same end offset; `modules/json/main.js`): now sorted by `(end, symbol)`. Replacement (question 2):
  `globalThis.X = …` / `delete global.X` / `Object.defineProperty(globalThis, "X", …)` for a built-in `X` is
  rejected at build time (`Prelude::replaced`; reads of `X` stay static, so accepting it would miscompile);
  real replacement is a follow-up. Unit test `unsupported.rs::global_object_replacement`. `cargo test` ok.
- 2026-09-29: test262 targeted: `built-ins/global` 13/29 pass, `language/global-code` 17/42 (18 skip; the rest are
  script-only: top-level `this`, `let undefined`, `Object = 12` rejected). Subset tests naming
  `globalThis`/`global`: 9 unsupported → pass, 0 regressions. `harness/asyncHelpers.js` names `globalThis` and
  checks `globalThis.$DONE`: the runner now adds `globalThis.$DONE = $DONE;` for async tests including it (a
  script's function declarations are global properties): of the 247 such tests (all unsupported before), 117 pass;
  new fails: `Promise.try` (not implemented, 4) and one unhandled-rejection (known noise).
- 2026-09-29: slice 2 core: `crates/frontend/src/eval.rs`: the linker's `load` splices each module's direct
  `eval(<literal>)` calls before parsing it (so renames reach eval'd code): `((() => {\n S' \n})())`, S' returning
  the completion value when the call's value is used (`eval$completion<k>`; UpdateEmpty prefixes `{ c = void 0; … }`
  for if/loops/switch/try, labels kept on loops, `finally` not counted). S is parsed alone inside
  `class extends Object { constructor() { S } }` (permits `super`/`new.target`/`arguments`; the body span must be
  exactly S, so `} m() {` can't escape; `return` found by a statement walk); the module with S in place is parsed
  again and an early error inside a region makes that call `throw new SyntaxError(msg)` (context rules: `super()`,
  `new.target`, `arguments` in fields). Nested evals: further rounds (≤ 16). `EvalMap` maps spliced offsets back
  (a call's code → the call); `Program::original`/`evals`, `SourceMap::evals`. Bug found on the way: `parse()`
  set `original` to the spliced text. Fixture `globals/eval-direct.js` matches node. Limitation: `arguments` in
  eval code inside a function hits the lowering's "`arguments` in an arrow function" (follow-up).
- 2026-09-29: slice 2 test262: subset tests naming `eval` (795 run): 297 unsupported → pass, 0 regressions; fixed
  on the way: edits at one offset (inner `)` before outer ` }`: for-in `S12.6.4_A3*`), `finally` values count
  when it completes abruptly (saved/restored around it; `try/completion-values.js`), and a lowering bug outside
  eval: `new.target` in an instance field initializer (also in its arrows) is `undefined` (closures.rs, like
  static ones). `language/eval-code/direct` 62 of 67 run pass (219 sloppy-only skipped), `indirect` 0 (slice 3).
  Left: `toString` as a bare name in eval code (node's global object inherits `Object.prototype`, 2 tests).
- 2026-09-29: slice 3 core (eval.rs): `Function(lits…)`/`new Function(lits…)` → `(function anonymous(P\n) {\nB\n})`
  in place (a new object per evaluation; checked alone: parameters and body spans exact, else "Arg string
  terminates parameters early"); sloppy `this` in a non-strict body → a `const` of `this == null ? globalThis :
  Object(this)`; indirect eval `(0, eval)(lit)`/`globalThis.eval(lit)` → `(function () { S' }).call(globalThis)`
  (sloppy script parse; top-level `var`/function declarations rejected: they'd be global properties).
  References from `Function`/indirect code to module bindings are build errors (`module_refs`; node: only globals,
  a ReferenceError there). Build errors of a round are all reported (sorted). Lowering: `eval` is `$$eval` (returns
  non-strings, throws `EvalError` for a string), `Function` a constructor value over `$$Function` (throws
  `EvalError`), both on the global object. Fixture `globals/function-ctor.js` matches node; `build.rs` test used a
  bare `eval` as its unsupported example: now `Proxy`. Gating: only F186's fixtures differ; `modules/json/main.js`
  is now deterministic (the old compiler gave two outputs, 16/9 over 25 builds: the TDZ-flag order fix).
- 2026-09-29: slice 3 test262: `language/eval-code/indirect` 21 of 59 run (was 0; 24 rejected for `var`/function
  declarations, 4 `eval` through a variable → EvalError, script-only `this`), `built-ins/eval` 8/10; subset
  tests naming `Function` (185): 40 unsupported → pass, 15 unsupported → fail (subclassing `Function`/`new F(…)`
  reach `$$Function` → EvalError (7); strict functions' `caller` poison pill missing (5); sloppy-only code in
  `Function('var arguments;')` (2); `Function.prototype.constructor` traps (1)), 0 regressions.
  `built-ins/Function` (509) not added to the subset: 45 `prototype/toString` tests take ~7 s each (the
  `nativeFunctionMatcher.js` harness) and time out under load; 169 of its prototype tests pass. Subset += 
  `language/eval-code`, `built-ins/eval` (and slice 1's `built-ins/global`, `language/global-code`).
- 2026-09-29: `eval?.(…)` is indirect (was skipped as an optional call). Targeted test262 run with `--update`
  (1,695 tests: all that were unsupported + those naming `globalThis`/`global`/`eval`/`Function`/`new.target`, the
  asyncHelpers ones, the new directories): 0 regressions, 619 unsupported → pass, 41 unsupported → fail (known
  causes, follow-ups F201–F211); subset 26,653 of 28,296 run (94.2%). Docs: D6, keywords.md, README, HISTORY.md;
  follow-ups F201–F211; F151, F154 resolved; F186 note. Task done.
