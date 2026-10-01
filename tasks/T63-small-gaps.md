# T63: small language and built-in gaps from the zigbee2mqtt survey (F261–F265, F271)

Status: todo (handoff written 2026-10-01, not started) | Depends on: T62

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
- [ ] Items 1–6 equal node on their fixtures; programs without them keep their bytes.
- [ ] T62's probe needs fewer patches (list which went away).
- [ ] Follow-ups updated.

## Notes / Result
(to be written)

## Checklist
- [ ] `error.stack`, error locations
- [ ] guarded globals, missing statics
- [ ] `getOwnPropertyDescriptors`, TypedArray `toStringTag`, `console._stdout`
- [ ] rest + `arguments`, `delete a?.[b]`, `var` before declaration
- [ ] zigbee2mqtt probe, docs, follow-ups

## Progress
- 2026-10-01: handoff written after T62.
