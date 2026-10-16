# T127: Promise species and `[Promise]` display for subclasses (F438)

Status: done | Depends on: T17, T47, T126 | Local (+ a-03 only after asking)

## Goal
F438 (found in T126): on a `class P extends Promise`, `then`/`catch`/`finally` and the statics (`P.resolve`,
`P.reject`, `P.all`, …, `P.withResolvers`) make `%Promise%` promises (node: `P`), because `promise.rs` doesn't model
species (NewPromiseCapability for any constructor); `console.log` shows `P { … }` where node shows
`P [Promise] { … }`. User chose F438 on 2026-10-16.

## Plan
- [x] `Promise[Symbol.species]` getter; SpeciesConstructor; NewPromiseCapability(C) (`new C(executor)`, the executor
  records `resolve`/`reject`); `then` and the statics use it; all only in modules with subclassing.
- [x] Display; fixture `promises/subclass.js`; fixtures; gate (laptop); full test262 on a-03 (ask).

## Progress
- 2026-10-16: task created. Design: a capability whose promise is a native `$promise` settled by our own resolving
  functions (a subclass calling `super(executor)`) is used as the reaction's `derived` (exact spec order); any other
  capability: an internal promise chained to its `resolve`/`reject` (one more job). Display: `inspect.rs` prefixes
  ` [Promise] ` for a promise whose constructor's name isn't `Promise` (modules with subclassing, unless the code
  names `toStringTag`, which installs the real tag).
- 2026-10-16: finding: the statics are prelude code (T50: `$$promiseCapability`, `$$Promise_resolve`, `all`, …) and
  already make `P` instances; only the native `then` (and `catch`/`finally`, which call it) made `%Promise%`.
  Implemented: a hook `ir::Module::promise_capability` = the prelude's `$$promiseCapability` (rooted in modules with
  subclassing that name `then`/`catch`/`finally`); native `then` (only with the hook): SpeciesConstructor (node's
  "The .constructor property is not an object", "object.constructor[Symbol.species] is not a constructor"), for a
  constructor other than `%Promise%` the capability: our own resolving functions of a native promise (`super`) → it
  is `derived`; else handlers `CapHandler { rejected }` (env `[handler, resolve, reject]`) call the handler, then the
  capability's `resolve`/`reject` in the same job (the spec's order; a first version chained an inner promise: one
  job late, fixed). `get Promise[Symbol.species]` (native `SpeciesGetter`, modules with subclassing).
  `inspect.rs`: `P [Promise] { … }`. Probes (`scratchpad/t127/p.js`, `p2.js`: order against plain promises, species
  override, a wrapping constructor, executor/constructor/species TypeErrors, `finally`, `catch`, `await`, display)
  = node. Fixture `promises/subclass.js` = node.
- 2026-10-16: `cargo test` passes; fixtures `promises` 15, `async`, `classes`, `builtins`, `exceptions`, `generators`,
  `modules`, `commonjs`, `objects`, `arrays`, `node` pass (`fetch.js` timed out under load, passes alone). Laptop gate
  vs `61baaab` (`out/gate-t127-laptop.log`): all at equal instructions, but every module differed: `then` declared
  its 4 new locals in every module; now only with the species path: benchmark and micro modules byte-identical to
  base again (checked). Next: full test262 on a-03 (ask).
- 2026-10-16: user: yes. Full test262 on a-03 started (`~/t262-t127.log`).
- 2026-10-16: test262 (a-03, `out/t262-t127.log`): 28,119 → 28,123 (+4: `built-ins/Promise/prototype/then/`
  `capability-executor-called-twice`, `capability-executor-not-callable`, `ctor-custom`, `deferred-is-resolved-value`),
  0 regressions; baseline updated for those 4 (laptop, `out/t262-t127-update.log`). Task done.

## Notes / Result
- **What changed:** native `then` uses SpeciesConstructor and, for a constructor other than `%Promise%`, the prelude's
  NewPromiseCapability (`ir::Module::promise_capability` hook): a native promise settled by our own resolving
  functions is the reaction's `derived`; any other capability gets handlers that call its `resolve`/`reject` in the
  same job (`CapHandler`). `get Promise[Symbol.species]`; `console.log` shows `P [Promise] { … }`. Only in modules
  with built-in subclasses (the hook is rooted when such a module names `then`/`catch`/`finally`): other modules are
  byte-identical. The statics were already species-aware (prelude, T50).
- **Result:** fixture `promises/subclass.js` = node (job order included); fixtures and `cargo test` pass; laptop
  gate: benchmark/micro modules byte-identical; full test262 +4 (28,123), no regressions.
- **Next smallest step:** choose: F407 (raytrace), F436 when wasmtime 50.0.0 is final.

