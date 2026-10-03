# T63: small language and built-in gaps from the zigbee2mqtt survey (F261–F265, F271)

Status: done (2026-10-01) | Depends on: T62

## Goal
The small gaps that cost zigbee2mqtt the most build errors and its first run-time failures (T62), each matching node
24.21.0 on its own fixture:
1. `error.stack` (F261): a string instead of a trap; `console.log(error)` and uncaught-error output unchanged.
2. Uncaught errors say where they were thrown (F271): at least the module path (and line, if cheap).
3. node globals js2wasi3 lacks, named behind `typeof` guards (F262): not build errors where node would never read
   them; missing statics (`Reflect.decorate`) read as `undefined` instead of a build error.
4. `Object.getOwnPropertyDescriptors`; the `%TypedArray%.prototype[Symbol.toStringTag]` getter (F264).
5. A rest parameter with `arguments`; `arguments` in a class constructor or field (F263).
6. `delete a?.[b]`; a `var` read in a `for` condition before its `var` in the body; `console._stdout`/`_stderr`
   (F265).
Programs that don't use them keep their bytes (as every gated feature so far).

## What we know (at handoff)
- T62's counts (docs/zigbee2mqtt-feasibility.md, "Survey 2026-10-01"): unknown globals 83 lowering errors
  (`navigator` 31 from debug's browser build and mqtt), `Reflect.decorate` 15 (tsc's `__decorate`: `typeof
  Reflect === "object" && typeof Reflect.decorate === "function"`, and the call in the branch is an error even when
  the branch is `if (false)`), `getOwnPropertyDescriptors` 7, `console._stdout/_stderr` 6; one each of the rest.
  T62's `tasks/T62-files/patch.sh` has the exact source sites (and `errors-p1.txt` every error with its location).
- Where they come from:
  - unknown globals: `crates/lower/src/lib.rs` `NODE_GLOBALS` (node's global names: one js2wasi3 lacks is
    "unsupported: global `X` (planned: T21)", any other undeclared name throws a ReferenceError at run time, F139);
    the error is raised where the name is resolved (`resolve`, "global `{}`") and for prelude names in
    `builtins.rs` (`user_refs`). **Decision needed:** node 24 *has* `navigator` (`navigator.userAgent` is
    `Node.js/24`), `URL`, `Blob`, …: making `typeof navigator` `"undefined"` would differ from node. Options: keep
    build errors but only where the read isn't guarded (`typeof x` and the `typeof x !== "undefined" && x…`
    shape), throw at run time with node-like text, or provide the cheap ones (`navigator` with `userAgent`,
    `hardwareConcurrency`, `language`, `platform`). Write the choice into design.md.
  - missing statics: `lib.rs` ~5017 ("`{obj}.{prop}` (planned: T21)", calls) and the `typeof`/value reads.
  - `error.stack`: `crates/backend/src/object.rs` ~3096: every error defines own `stack` = `G_BUILTIN` (the
    sentinel, so `"stack" in e` is true and reading traps). `inspect.rs`/`runtime.rs` read `stack` for printing
    (T25: `Promise.any`'s errors); node prints errors from their `stack`. Simplest: `stack` = `${name}: ${message}`
    plus one `    at <anonymous>` frame, set at construction (V8 also sets it at construction), and keep printing
    as today (check fixtures that print errors: `scripts/diff.sh` over `tests/fixtures`).
  - rest parameter + `arguments`: `lib.rs` ~1725 and ~2430 (T25 binds `arguments` from a synthetic rest
    parameter, so a real rest parameter must be sliced from it); `arguments` in a class constructor/field:
    `closures.rs` ~302.
  - `delete a?.[b]`: `lib.rs` ~4164. `var` before its declaration: `lib.rs` ~7198/7252 (`cx.pending` cells:
    a `var` is hoisted, so this is a bug for `var`, right for `let`/`const`).
  - `console._stdout`: `builtins.rs` `console_members` (reported as "`console.x`", T61).
  - TypedArray `toStringTag`: `crates/lower/builtins/typedarray.js` (`Object.getOwnPropertyDescriptor(%TypedArray%.
    prototype, Symbol.toStringTag).get` must be a function returning the type name or `undefined`).
- Uncaught errors (F271): the command prints `Uncaught <Name>: <message>` (`runtime.rs`/`promise.rs`); the source
  map is `Program::locate`. A `stack` with a real first frame would give both F261 and F271.

## Plan
1. `error.stack` (+ F271 if it falls out), fixture `exceptions/stack.js` (what node prints that we can match:
   `e.stack.split("\n")[0]`, `typeof e.stack`, `"stack" in e`, `Object.keys(e)`).
2. The `typeof`-guarded globals and missing statics (decide; design.md), fixture `globals/guarded.js`.
3. `Object.getOwnPropertyDescriptors`, the TypedArray getter, `console._stdout`, fixtures in `builtins/`.
4. Rest parameter + `arguments`, `arguments` in constructors, `delete a?.[b]`, `var` before declaration: fixtures
   in `syntax/` or `closures/`.
5. Probe: T62's `patch.sh` with the matching blocks removed (winston, safe-stable-stringify, debug, tsc
   `__decorate` in zigbee2mqtt's `dist/`) still builds and runs to `Opening TCP socket`.

## Read first
- `tasks/T62-zigbee2mqtt-survey.md` (Notes / Result), `docs/zigbee2mqtt-feasibility.md` (Survey 2026-10-01),
  `tasks/README.md` Follow-ups F261–F265, F271, F139, F152.
- `tasks/T28-test262-quick-wins.md` (undeclared names, F139), `tasks/T61-weakmap-abort.md` (`console` members).

## Tests
Each item's fixture through `scripts/diff.sh`; `cargo test` (the `unsupported` tests: some expected messages
change); test262: the directories touched (`built-ins/Object/getOwnPropertyDescriptors`,
`built-ins/TypedArray/prototype/Symbol.toStringTag`, `language/arguments-object`) through `scripts/test262.mjs`.

## Acceptance
- [x] Items 1, 3–6 equal node on their fixtures; programs without them keep their bytes. Item 2 (locations) is
  deferred (F271, design below).
- [x] T62's probe needs fewer patches (list which went away).
- [x] Follow-ups updated.

## Notes / Result
**What changed**
- `error.stack` (F261): lazy like V8, gated on the code naming `stack` (`builtins.rs` GATED). Each error's own
  `stack` is a hidden accessor with the sentinel as get/set (`object.rs` `new_error`); the first read
  (`props.rs` `value`) calls `error_stack`: `${String(e)}\n    at <anonymous>`, stored as a hidden data property;
  a write (`lazy_stack_set`) stores the value the same way. `console.log(e)` and uncaught output are unchanged
  (`frameless_stack`: a one-frame stack doesn't count as a custom one). Deviation: its descriptor shows a data
  property (node: get/set). design.md (T13 paragraph).
- node's globals js2wasi3 lacks (F262): T54's rule, design.md "node's globals js2wasi3 lacks": a build error only
  where the read always runs (`lib.rs` `may_skip`), else `ReferenceError: node's global \`X\` isn't available in
  js2wasi3 (planned: T21)` when it runs; `typeof X` is `"undefined"` (deviation: node has them). Missing statics
  (`Reflect.decorate(…)`, `Math.nope(…)`, `Math.NOPE`): same rule, node's TypeError / `undefined` at run time.
- `Object.getOwnPropertyDescriptors`; `get %TypedArray%.prototype[Symbol.toStringTag]` (symbol-keyed prototype
  getters in `object.rs`); `console._stdout`/`_stderr` are `process.stdout`/`stderr` (F264, F265).
- A rest parameter beside `arguments` (IR `ArrayRest(k)`, backend `array_rest`), `arguments` in a class
  constructor, `delete a?.b` / `delete a?.[k]` (`delete_chain`), a `var` read before its declaration (its local
  starts as `undefined`, `var_cells`) (F263, F265).
- `frontend/src/using.rs`: the disposal environment's `stack` property is now `list` (it made every `using`
  program name `stack`).

**Commands run**: `scripts/diff.sh` on `exceptions`, `promises`, `classes`, `closures`, `syntax`, `objects`,
`builtins`, `globals`, `using` (all pass); the four new fixtures (`exceptions/stack.js`, `globals/guarded.js`,
`builtins/small-gaps.js`, `syntax/small-gaps.js`) equal node; `cargo test --no-fail-fast`: all pass except the 4
`*_compose_with_wac` (wac isn't installed here); bytes: every command fixture built by HEAD and by this tree, the
21 that differ all name `stack` or `toStringTag` (or use `using`: only a string differs).

**zigbee2mqtt probe** (`tasks/T63-files/`: T62's `patch.sh` minus what T63 fixed, release build): still builds
(70 s, 32 MB) and runs to `Opening TCP socket with 127.0.0.1:6638` (`net.Socket`, as in T62), and the start
failure is now logged with its `stack`. Gone: `Object.getOwnPropertyDescriptors` rewrite, both `Reflect.decorate`
edits, winston's `console._stdout` and rest-parameter edits, `delete settings.devices?.[id]`, regenerator's
`var o`, safe-stable-stringify's `toStringTag` getter, the `error.stack` → `String(error)` edit (9 edits in 7 blocks). With
`T63_GLOBALS=0` (no `web.js` bindings for `URL`, `navigator`, …) the graph builds with **no errors**; at run time
herdsman's `new URL(path)` then throws and it stops at `Invalid TCP path`, so `URL` is a run-time need (F266).

**Unsupported / deferred**
- F271 (uncaught error locations): not done. Design: a position global set by the lowering before each
  statement that can throw, only with a build flag (`--locations`), read by `runtime.rs`'s uncaught report and
  `error_stack` (`at file:line:col` from `Program::locate`); without the flag, nothing changes. F275.
- An uncaptured `var` assigned only in a branch reads Wasm's default when the branch didn't run (`if (c) { var x
  = 1 } return x` gives `0`; a string-typed one traps): pre-existing miscompile found here. F276.
- `Object.getOwnPropertyDescriptor(e, "stack")` is a data descriptor (node: accessor); a guarded missing
  static's call doesn't evaluate its arguments; `typeof` of a lacked node global is `"undefined"`. F277.
- F274 (link and lowering errors in one run): not looked at.

**Next smallest step**: T64, `URL`/`URLSearchParams`, `TextEncoder`/`TextDecoder`, `DataView` (F266), then
rerun `tasks/T63-files/build-run.sh` with `T63_GLOBALS=0`.

## Checklist
- [x] `error.stack` (error locations deferred: F271, F275)
- [x] guarded globals, missing statics
- [x] `getOwnPropertyDescriptors`, TypedArray `toStringTag`, `console._stdout`
- [x] rest + `arguments`, `delete a?.[b]`, `var` before declaration
- [x] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T62.
- 2026-10-01: started. `error.stack` (F261): lazy like V8. Every error's own `stack` is a hidden accessor whose
  get/set are the `G_BUILTIN` sentinel (only when a program names `stack`, GATED in `builtins.rs`; else it stays
  the old data sentinel, so other programs keep their bytes). The first read (`props.rs` `value`) calls
  `object.rs` `error_stack`: `${String(e)}\n    at <anonymous>`, then redefines `stack` as a hidden data property;
  a write replaces it the same way. Printing (`inspect.rs`, `runtime.rs`) treats a stack that is just the header
  plus our one frame as "no stack" (`frameless_stack`), so `console.log(e)` and uncaught output are unchanged.
  Deviation: `Object.getOwnPropertyDescriptor(e, "stack")` shows a data property (node 24: get/set functions).
  Fixture `exceptions/stack.js` equals node.
- 2026-10-01: guarded globals and missing statics (F262), decision in design.md ("node's globals js2wasi3
  lacks"): T54's rule. A build error only where the code always runs (`lib.rs` `may_skip`: functions other than a
  CommonJS wrapper, classes, `try`, branches, loop bodies); elsewhere a run-time ReferenceError naming the global,
  `typeof X` is `"undefined"` (`typeof_operand` probes the identifier with `in_typeof`; a deviation from node,
  where they exist). Missing statics: called → node's TypeError `X.y is not a function`, `Math.NOPE` read →
  `undefined`. `crates/lower/tests/unsupported.rs`: the `global`/static cases moved to top level, new cases for the
  guarded shapes. Fixture `globals/guarded.js` (tsc `__decorate`, never-called functions, dead branches) = node.
- 2026-10-01: F264 and `console._stdout` (F265): `$$Object_getOwnPropertyDescriptors` (object.js; keys defined,
  so `__proto__` stays a key); `$$TypedArray_prototype_$toStringTag` (typedarray.js), linked by naming
  `toStringTag` (builtins.rs) and defined as a getter (`object.rs` prototype maker: symbol-keyed getters);
  `console._stdout`/`_stderr` read `process.stdout`/`stderr` (builtins.rs `CONSOLE_STREAMS` links `$$Process`,
  lib.rs member read). Fixture `builtins/small-gaps.js` = node; `scripts/diff.sh tests/fixtures/builtins` 62 pass.
- 2026-10-01: F263/F265 language items. Rest parameter beside `arguments`: bound in the prologue from the
  all-arguments array (new IR intrinsic `ArrayRest(k)`, backend `array_rest`); both rejections removed (`lib.rs`
  top-level and nested signatures). `arguments` in a class constructor: `closures.rs` marks the class scope as
  using it, its signature takes one rest array (`lib.rs` class signature), the T25 prologue binds the parameters.
  `delete a?.[b]` / `delete a?.b.c`: `lib.rs` `delete_chain` (`true` when the chain stops). A `var` read before
  its declaration: `var_cells` starts such an uncaptured `var`'s local as `undefined` (`read_before_declaration`).
  Found while testing: an uncaptured `var` assigned only in a branch reads Wasm's default when the branch didn't
  run (`function f(c) { if (c) { var x = 1 } return x }` gives `0`, a string one traps): pre-existing, recorded as
  a follow-up. Fixture `syntax/small-gaps.js` = node; tests updated (`unsupported.rs` closures, `build.rs`:
  `rest.cjs` and its test removed, `module_errors_are_diagnostics` uses a top-level `Proxy`). `cargo test`: all
  pass but the 4 `*_compose_with_wac` (no wac here). diff.sh classes/closures/syntax/objects: 38 pass.
- 2026-10-01: bytes check (every command fixture built by HEAD and by the working tree, `cmp`): 21 differ, each
  naming `stack` (the lazy accessor; node's `stream`/`fs` modules too) or `toStringTag` (the TypedArray getter
  is linked by the name, like other `%TypedArray%` methods), plus `using/*`: the `using` lowering's environment
  had a `stack` property (`frontend/src/using.rs`), renamed `list` (only the string differs now). Printing's
  frameless check is only emitted when the code names `stack`.
- 2026-10-01: zigbee2mqtt probe with `tasks/T63-files/` (7 patch blocks, 9 edits, removed): builds, runs to `Opening TCP
  socket`; without the globals bindings it builds with no errors and stops at herdsman's `URL`. F271 deferred
  (design in Notes / Result). Done.
