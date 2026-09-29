# T50: Promise combinators per the spec (F187, F207, F210)

Status: done (2026-09-29) | Depends on: T17 (promises), T25 (`any`, `withResolvers`),
T35 (prelude statics with a constructor `this`), T42 (`Promise.try` exposed)

## Goal
`Promise.all`/`allSettled`/`any`/`race` behave like node 24.21.0 for any `this`: NewPromiseCapability(C) (a
subclass or any constructor, the executor's checks), GetPromiseResolve (`C.resolve` read once, must be callable,
called per element with `this` = C), `then` looked up and called on each result (Invoke), IfAbruptRejectPromise with
IteratorClose on the right steps, element functions that run once and have `length` 1 and `name` "". Also
`Promise.resolve`/`reject`/`withResolvers` with a `this` that isn't `%Promise%`, the TypeError for a `this` that isn't
a constructor (F210), `Promise.try` (F207). Programs that don't name these statics compile to the same bytes.

## What we know (at handoff)
- **Today**: everything Promise is native Wasm in `crates/backend/src/promise.rs`. `combine(cx, c)` always makes a
  `%Promise%` result, resolves each item with PromiseResolve(%Promise%, x) (never `C.resolve`), calls `then` by
  `Invoke`, and closes the iterator on errors inside the loop only. The statics are installed by `ctor(cx)`;
  `any`/`withResolvers` only when named (`cx.names`). The module notes "Not modelled: subclassing through species".
- **test262**: the four combinator directories run 390 tests: 235 pass, 141 fail, 12 timeout, 2 unsupported
  (`Object.defineProperties`). By file name: `invoke-resolve*` 44, `resolve*` 26, `reject*` 15, `capability*` 13,
  `resolve-element*` 12, `ctx*` 12, `iter*` 8, `call-resolve*` 6, `invoke-then*` 6, others ~13. The 12 timeouts
  are F187 (the iterator isn't closed when `C.resolve` throws). Full list at handoff: the scratchpad's
  `promise-fails.tsv`; rerun `node scripts/test262.mjs --list built-ins/Promise/{all,allSettled,any,race}`.
- **Precedent for prelude statics taking `this`**: `$$Array_from` (`const c = this`, T35); `Array.from(…)` direct
  calls pass the constructor value.

## Design questions (decide first, record in docs/design.md D5 / D3)
1. **Where**: rewrite the combinators in the prelude (`builtins/promise.js`, the spec's steps in JS: `new C(executor)`,
   `C.resolve`, `nextPromise.then(…)`, closures for the element functions) and keep the core (`$promise`, jobs,
   `then`, resolve functions) native; or extend `combine` natively. Leaning prelude: ~15 observable steps per
   combinator, and speed isn't the issue here. Then: how the native `Promise` value gets prelude statics (like
   `builtin_statics` for `CTOR_VALUES`), and how `Promise.all(x)` direct calls pass `this` (`Intrinsic::PromiseCtor`).
2. **Gating**: prelude functions linked only when the code names the static (`all`, `race`, …), so programs that
   don't keep their bytes; the native `combine`/`Element`/`AnyReject` helpers go away (or stay for a fast path?).
3. **`%Promise%` fast path**: when C is `%Promise%` and `C.resolve` is the original, may we skip observable steps?
   No: `Promise.resolve` can be replaced; do the spec's steps always (they're cheap).

## Plan
1. Decide Q1–Q3; move `all`/`allSettled`/`race`/`any` to the prelude with NewPromiseCapability + GetPromiseResolve
   + IteratorClose; fixture `tests/fixtures/promises/combinators-spec.js` (subclass, custom `resolve`, thenables,
   throwing iterators; node is the oracle).
2. `Promise.resolve`/`reject`/`withResolvers` with any constructor `this`; F210's TypeErrors; `Promise.try` (F207).
3. test262: the four directories plus `built-ins/Promise` (targeted `--update`); gating check (scratchpad `cmp.sh`,
   old build from a worktree at HEAD); docs (design.md, keywords.md, HISTORY.md, README), follow-ups.

## Read first
- `crates/backend/src/promise.rs` (module docs, `ctor`, `combine`, `element`, `Helper`).
- `docs/design.md` D3 (T17 note) and D5 (prelude; T35's `this` for statics).
- `crates/lower/builtins/array.js` `$$Array_from`, `$$fromCreate`; `crates/lower/src/builtins.rs` (`CTOR_VALUES`,
  statics linking); `crates/lower/src/lib.rs` (`Intrinsic::PromiseCtor`, `builtin_static`).
- F187, F207, F210 in `tasks/README.md`.

## Tests
Fixture via `scripts/diff.sh`, `cargo test`, `wasm-tools validate` + wasmtime; `scripts/diff.sh tests/fixtures/promises`.
Gating: old vs new build of every fixture byte-identical except those naming the changed statics. test262: targeted
`--update`; ask before a full run on `a-03`.

## Acceptance
- [x] The combinators follow the spec for any `this` (fixture matches node).
- [x] F187, F207, F210 resolved or narrowed with a named follow-up; test262 combinator directories mostly pass;
      0 regressions; baseline updated.
- [x] Programs not naming the changed statics compile to the same bytes.

## Notes / Result
- **What changed**: `crates/lower/builtins/promise.js` has the eight statics as the spec's steps (NewPromiseCapability
  through `new C(executor)`, GetPromiseResolve, the iterator protocol by hand with IteratorClose, element functions
  as arrows with a shared `alreadyCalled`); intrinsic `is_promise` (`Intrinsic::IsPromise`). The lowering sends
  `$$Promise_*` to `ir::Module::promise_statics`; the backend's `Promise` value installs them in node's order in
  place of the native ones (kept, with the same bytes, when none is linked). `Promise.x(…)` is a direct call with
  `this` = `Promise` (`direct_call_this`); `Promise.x` read as a value is the property; `Promise.call` & co. aren't
  rejected as missing statics. `build --unhandled-rejections=throw|none` (`Module::ignore_unhandled`; with `none`
  only the entry module's rejected evaluation promise, marked with `handled` bit 2, is reported).
- **Design**: recorded in `docs/design.md` D5 (T50 note) and D2 (the wasmtime caveat).
- **Found**: a wasmtime 49 bug: GC references assigned to a local inside a `try` and read in its `catch` are lost or
  stale after a collection (F224; the prelude avoids the pattern). `Promise.try`'s `avoids-wrap*` tests check a 2026
  spec change node 24 lacks (kept failing).
- **Gating**: 31 fixtures change bytes, each names one of the statics (or imports a module that does); all pass
  `diff.sh` (`timers-more.js` is timing-sensitive under parallel load, passes alone).
- **test262**: the combinator directories pass 388 of 390 (`any`: 2 use `Object.defineProperties`); `built-ins/
  Promise` 614 of 639; subset 27,451 of 28,591 run (96.0%, was 95.3%); 2 regressions that passed by accident
  (`finally` species, F225). Targeted `--update` on `a-03` (6,256 tests); no full run yet.
- **Commands**: `scripts/diff.sh tests/fixtures/{async,promises,library,modules,using}`, `cargo test`, `node
  scripts/test262.mjs --update <6,256 tests>` on `a-03`.
- **Next smallest step**: F225 (species in `then`/`finally`: SpeciesConstructor + NewPromiseCapability in the native
  `then`, or `then` in the prelude too), or F224 (minimize to WAT, report upstream).

## Checklist
- [x] design questions decided and recorded
- [x] combinators in the prelude (or native) per the spec
- [x] `resolve`/`reject`/`withResolvers` with any `this`, `Promise.try`
- [x] fixture, gating check, test262 + baseline
- [x] docs (HISTORY.md, keywords.md, design.md, README), follow-ups

## Progress
- 2026-09-29: handoff written after T49. State: `main` at `9794016`, pushed; test262 subset 27,243 of 28,591 run
  pass (95.3%); the combinator directories are the largest failing cluster (155 of 390).
- 2026-09-29: decisions: Q1 prelude (`builtins/promise.js`: all eight statics `all`/`allSettled`/`any`/`race`/
  `resolve`/`reject`/`withResolvers`/`try`, the spec's steps in JS; core stays native); the backend installs them on the
  native value (`ir::Module::promise_statics`, node's order) in place of the native ones, which stay (unchanged bytes)
  when none is linked; `Promise.x(…)` direct calls pass `this` = `Promise` (`direct_call_this`). Q2: linked by
  `Promise.x` references, or by name when the code names `Promise`/`constructor`/the global object. Q3: no fast path.
  New prelude intrinsic `is_promise` (`Intrinsic::IsPromise`). A scratch script of 25 error/order cases matches node.
- 2026-09-29: first run of `built-ins/Promise`: 585 pass (91.5%), 8 regressions. ~20 fails are unhandled rejections the
  combinators now make per the spec (node would exit 1 too; test262 hosts don't fail on them): new build flag
  `--unhandled-rejections=throw|none` (node's name; `ir::Module::ignore_unhandled`, default unchanged), the test262
  runner builds with `none`. Two tests crash wasmtime's default copying collector ("BUG: gc heap corruption", or an
  out-of-bounds trap in `find_own`); `-C collector=drc`/`null` and drc with `gc-zeal-alloc-counter=1` run them
  correctly, so it looks like wasmtime (like F134, but a single component). Reducing a repro.
- 2026-09-29: the crashes are a wasmtime 49 bug, not the collector alone: a GC reference assigned to a local inside a
  `try` and read in its `catch` is wrong after a collection during the throwing call (`drc`: freed when not also live
  on the normal path; `copying`: stale even then; values defined before the `try` are fine). 14-line repro (user
  code, no Promise): scratchpad `g5.js`/`vA`–`vE.js`, recorded as F224. `promise.js` avoids the pattern (the
  iterator record is made before the `try` and filled by element writes); the two tests pass. Also: `Promise.x`
  read as a value is the `Promise` value's property (identity); `any`'s AggregateError gets a frameless `stack` like
  the native one did (`console.log` form); fixture `promises/combinators-spec.js` passes; `Promise.try`
  `avoids-wrap*` (2) are a 2026 spec change node 24 lacks (kept failing: node is the oracle).
- 2026-09-29: `built-ins/Promise`: 613 of 639 run pass (95.9%). 3 regressions: `undefined-newtarget.js` (`Promise.call`
  rejected as a static the prelude lacks: fixed, `Promise` counts like a constructor value for inherited props); the 2
  `finally/*-observable-then-calls-PromiseResolve.js` passed by accident (`MyPromise.resolve` made a `%Promise%`; now a
  `MyPromise`, and the native `finally` ignores species, so it wraps once more): `then`/`finally` species is a
  follow-up. Gating: 31 fixtures change bytes, all name a static (`tla`/`tla-cycle` import `ticks.js`, which calls
  `Promise.resolve`).
- 2026-09-29: targeted run on `a-03` (6,256 tests, 368 s): 4 more regressions, `module-code/top-level-await/*-rejection*`:
  with `none` the entry module's rejected top-level `await` was ignored too (node keeps it fatal). Fixed: in `none`
  mode `run` marks the entry's evaluation promise (`handled` bit 2, `promise::MAIN`) and only its rejection is
  reported. Rerun (363 s): 0 new regressions; subset 27,451 of 28,591 (96.0%). Docs, follow-ups; task done.
