# T45: Explicit resource management: `using`, `await using`, `DisposableStack`

Status: done (2026-09-29) | Depends on: T12 (`finally`), T19/T20 (async), T25 (symbols, AggregateError), T42 (`frontend::eval`'s splicing and `EvalMap`)

## Goal
`using x = v` / `await using x = v` in blocks, functions, `for`/`for-of` heads, `switch` cases and the module's top
level, disposing in reverse order at scope exit (normal or abrupt) with `SuppressedError` chaining, as node 24 does;
`Symbol.dispose`, `Symbol.asyncDispose`, `SuppressedError`; then `DisposableStack` and `AsyncDisposableStack`.
test262: `language/statements/using` (50), `await-using` (69), `for-of` (4) are in the subset (all unsupported);
`built-ins/{DisposableStack,AsyncDisposableStack,SuppressedError}` (219) aren't yet.

## Design (decide in slice 1)
Lower `using` by a source rewrite in the frontend (like T42's eval splicing, TypeScript's downlevel emit): a
statement list with a `using` becomes `{ const env = …; try { … const x = add(env, v, async); … } catch (e) { env
error } finally { dispose(env) [await] } }`, the helpers generated once per module as plain JS (no prelude names in
user code), so `break`/`continue`/`return`/`yield` go through the existing `finally` lowering.

## Plan
1. `using`/`await using` + `Symbol.dispose`/`asyncDispose` + `SuppressedError`; fixtures; test262 `using`,
   `await-using`, `for-of`.
2. `DisposableStack`/`AsyncDisposableStack`; add the built-ins directories to the subset.
3. Docs, follow-ups.

## Checklist
- [x] slice 1
- [x] slice 2
- [x] test262 + baseline, docs, follow-ups

## Notes / Result
- `using`/`await using`: `crates/frontend/src/using.rs`, a source rewrite after the eval splicing (design D7): a
  disposal environment and `try`/`catch`/`finally` per scope, `const x = v, r = add(env, x, async)`, `for (using x
  of …)` binding `const x` with the environment in the body, the async disposal loop inline in `finally` (the spec's
  awaits; tick order matches node); helpers generated once per module as plain JS. Direct eval code rejects a
  top-level `using` (a Script).
- `SuppressedError`: a native error kind outside `ErrorKind::ALL` (`new_error` defines `error`/`suppressed`, length 3);
  `DisposableStack`/`AsyncDisposableStack`: `Builtin`s outside `Builtin::ALL`, classes 14/15, data under the private
  state key (`state_new`/`state_data`, shared with Date), methods in `builtins/dispose.js`, `disposed` getter,
  constructors require `new`, toString tags. All their globals come after the BigInt ones only when used
  (`runtime::runtime_globals`); `Symbol.dispose`/`asyncDispose` are well-known symbols 12–13 (the array's size
  grows only then). Every other fixture builds byte-identical.
- Tests: fixtures `using/{using,await-using,disposable-stack}.js` (node matches); `cargo test` ok. test262: see
  `tests/test262/HISTORY.md` (subset 94.6%, 0 regressions).
- Left: F212 (error constructors' [[Prototype]]), F213 (`[Symbol.dispose]` identity, subclass printing), F214
  (top-level `using` with imports/exports); TDZ reads before a `using` (the existing "used before its declaration"
  limitation); script-only tests.
- Next smallest step: F214 (hoist `import`/`export` out of the top-level `try`).

## Progress
- 2026-09-29: task created (user chose explicit resource management; the largest failing test262 feature in the
  subset: 123 tests). node 24.21.0 has `using`, `await using`, `DisposableStack`, `AsyncDisposableStack`,
  `SuppressedError`, `Symbol.dispose`/`asyncDispose`.
- 2026-09-29: `SuppressedError` is a native error kind outside `ErrorKind::ALL`/`Builtin::ALL` (globals after the
  BigInt ones, which are emitted too then: `runtime::runtime_globals`; `Module::suppressed` from the prelude scan);
  `Symbol.dispose`/`asyncDispose` are well-known symbols 12–13 (the array has 14 slots only when the code names
  them; `GATED` += both). `crates/frontend/src/using.rs`: the rewrite (after eval splicing, one more `EvalMap`
  layer): per scope `const env = …; try { … } catch … finally { dispose }`, declarations → `const x = v, r =
  add(env, x, async)`, `for (using x of …)` binds a temporary, inline async disposal loop. Fixtures
  `using/using.js`, `using/await-using.js` match node (tick order too).
- 2026-09-29: slice 1 test262 (`--update`): `using` 69 of 77 run, `await-using` 84 of 96, `for-of` 667 of 713
  (+100 unsupported → pass); 0 regressions (4 `for-of/dstr` load timeouts pass at -j 2). Fixed on the way:
  `for (using x of …)` binds `const x` itself (its head TDZ is then the existing `const` one: the 2 TDZ tests
  are "used before its declaration", like `const`), `using` at the top level of eval code is a SyntaxError.
  Left: TDZ reads (12, existing limitation), top-level `using` in a module with imports/exports (3), tagged
  templates (3), script-only syntax tests (2). Gating clean, `cargo test` ok.
- 2026-09-29: slice 2: `Builtin::DisposableStack`/`AsyncDisposableStack` outside `Builtin::ALL` (prototype and
  constructor globals after `SuppressedError`'s when `Module::stacks`, which brings those too), classes 14/15 with
  their data under the private `G_SYM_STATE` key (`__intrinsic_stack_new`/`async_stack_new`/`stack_data`,
  `object::state_new`/`state_data` shared with Date), `disposed` a getter, constructors require `new`,
  `Object.prototype.toString` tags (`Cx::stacks`). `builtins/dispose.js`: the methods with node's messages;
  `[Symbol.dispose]`/`[Symbol.asyncDispose]` are linked by naming `dispose`/`asyncDispose` (separate functions:
  node's are the same object as `dispose`/`disposeAsync`). Fixture `using/disposable-stack.js` matches node.
- 2026-09-29: well-known symbol array: 14 slots also when stacks exist (the prelude reads `Symbol.asyncDispose`
  without the code naming it: a trap in 2 tests). Subset += the three built-ins directories; targeted `--update`
  run: 205 new → pass, 0 regressions; subset 26,983 of 28,512 run (94.6%). Docs: D7 in design.md, keywords.md,
  README, HISTORY.md; follow-ups F212–F214; F80 note. Task done.
