# T38: `new.target` in ordinary functions and arrows (F57)

Status: done (2026-09-28) | Depends on: T16 (class construct entries), T36 (`Reflect.construct`)

## Goal
`new.target` works in ordinary `function`s (the new target under `new`/`Reflect.construct`, `undefined` when called)
and in arrow functions (captured from the enclosing function, like `this`). Today both are rejected at build time
("`new.target` outside a class constructor", "`new.target` in an arrow function"); test262: ~65 non-passing tests in
the subset mention `new.target`.

## Design
Like class constructors (T16): a function that uses `new.target` (itself or through its arrows) takes a hidden
`new.target` param after `this`. Direct calls and its call adapter pass `undefined`; its function value gets its own
construct entry `f$construct(self, new_target, args)`: CreateThis(new_target), call the IR function, then the object
result or `this` (`FnKind::OrdinaryNt { construct }`; `func::Helper::New` uses a given construct entry even with
`NEW_ORDINARY`). Arrows capture it (`Cap::NewTarget`). Methods, accessors, generators and async functions can't be
constructed: `new.target` is `undefined` there.

## Tests
Fixture `tests/fixtures/new-target.js` (`scripts/diff.sh`), `cargo test -p js2wasi3-lower` (unsupported cases),
`wasm-tools validate` + a wasmtime run; test262: non-passing subset tests mentioning `new.target`, `--update`. F168
(super references) only if small.

## Checklist
- [x] analysis + lowering (functions, arrows)
- [x] construct entry, backend
- [x] fixture, unit tests, test262
- [x] docs, follow-ups, status

## Notes / Result
**What changed**
- `closures.rs`: `new.target` marks its nearest non-arrow function in `uses_new_target` (classes as before, now also
  functions); arrows between capture it (`Cap::NewTarget`); in a static block or static field it is `undefined`
  (`static_new_target`), also when the class sits inside a constructor.
- `lib.rs`: a constructable function (not a method, arrow, generator or async function) in `uses_new_target` takes a
  hidden `new.target` param after `this`; direct calls, its `$code` adapter and exports pass `undefined`. Its value is
  `FnKind::OrdinaryWith { construct }`, `construct` = `ctor_adapter`'s `f$construct(self, new_target, args)`:
  CreateThis(new_target), the call, `CtorReturn { derived: false }`. `func::Helper::New` keeps a given construct entry
  for `NEW_ORDINARY` (the shared one otherwise).
- F168: `props::super_get`/`super_set` read the home prototype first, then ToPropertyKey, and throw node's
  "Cannot read/set properties of null (…)" for a `null` one.

**Commands**: fixtures `tests/fixtures/new-target.js`, `tests/fixtures/classes/super-null-proto.js`, whole
`scripts/diff.sh` (120 pass), `cargo test` (the `unsupported.rs` `new.target` cases now lower), `wasm-tools validate` +
wasmtime; test262 `--update`: 38 + 5 + 2 → pass; regression runs (function/class/`new`/`super` directories, ~1,900
tests): none. Subset: 22,211 of 24,973 run pass (88.9%, was 22,166).

**Unsupported / left**: the rest of F57 (`super()` in an arrow function; `this`/`super` in an arrow in a static field
initializer); `new.target` in an instance field initializer gives the constructor's (spec: `undefined`; F169); node 24
converts `super[key]`'s key before reading the base (so the fixture leaves that order to test262).

**Next smallest step**: the rest of F57 (`super()` in arrows), or pick the next task (T27 RegExp suggested).

## Progress
- 2026-09-28: written and started (the user chose F57 after T37).
- 2026-09-28: implemented: analysis (`uses_new_target` for any non-arrow owner, `Cap::NewTarget` for arrows), hidden `new.target` param for constructable functions (`undefined` in direct calls, `$code` adapters, exports), `ctor_adapter` → `f$construct` (CreateThis + CtorReturn), `FnKind::OrdinaryWith`, `func::New` keeps a given construct entry with `NEW_ORDINARY`. Fixture `tests/fixtures/new-target.js` passes.
- 2026-09-28: unsupported.rs updated (`new.target` cases now lower; `Proxy` as the remaining rejection); cargo test -p js2wasi3-lower ok; validate ok; whole diff.sh 119 pass.
- 2026-09-28: `new.target` in static blocks/static fields: `undefined` (`Analysis::static_new_target`). test262: 68 non-passing subset tests mentioning `new.target`: 38 → pass, 0 fail (rest: `eval` 24, `globalThis` 2, …); baseline 22,204 of 24,973 (88.9%).
- 2026-09-28: F168: `props::super_get`/`super_set` read the home prototype before ToPropertyKey and throw node's TypeErrors for a `null` one; fixture `classes/super-null-proto.js` (the base-before-key order is test262-only: node 24 converts the key first); the 5 test262 tests pass.
- 2026-09-28: done. Docs (`keywords.md`, `design.md`), HISTORY, F57 (first part) and F168 ticked, F169 recorded.
