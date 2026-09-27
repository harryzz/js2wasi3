# T36: `Reflect` (F152, without `Proxy`)

Status: todo (approved 2026-09-27, not started) | Depends on: T25 (namespaces, built-ins as values), T32 (element
descriptors), T35 (`__intrinsic_is_constructor`)

## Goal
The `Reflect` namespace works like node's, written in the prelude over what already exists. `Proxy` stays out
(exotic objects; F152 keeps it). Today any use of `Reflect` is rejected at build time ("unsupported: global
`Reflect`"), which fails ~190 test262 tests in the subset that don't test `Reflect` at all: test262's harness uses it
(`isConstructor.js`: `Reflect.construct(function(){}, [], f)` to test constructability; `deepEqual.js`,
`temporalHelpers.js`, `iteratorZipUtils.js`: `ownKeys`, `get`, `has`, `getOwnPropertyDescriptor`). 268 test files
mention `Reflect.`.

## What node does (match it; check messages with node 24.21.0 before writing each)
`Reflect` is a plain object (`[object Reflect]`, `Symbol.toStringTag` "Reflect"; `console.log(Reflect)` prints
`Object [Reflect] { … }` in node 24; check), not a constructor. Its 13 functions, each throwing
`TypeError: Reflect.<name> called on non-object` for a non-object target (check exact texts):
- `apply(f, thisArg, args)`: `args` CreateListFromArrayLike ("CreateListFromArrayLike called on non-object").
- `construct(C, args, newTarget = C)`: both must be constructors ("… is not a constructor"); `new.target` is
  `newTarget` (the prototype of the result comes from `newTarget.prototype`).
- `defineProperty` (returns `true`/`false`, never throws on a rejected definition),
  `deleteProperty` (`true`/`false`), `get(o, k, receiver)` / `set(o, k, v, receiver)` (receiver: getters/setters get
  it as `this`; `set` returns `false` instead of throwing), `getOwnPropertyDescriptor`, `getPrototypeOf`,
  `setPrototypeOf` (`false` for a non-extensible object or a cycle), `has`, `isExtensible`, `preventExtensions`,
  `ownKeys` (strings in the spec order, then symbols; `length` for arrays, like `getOwnPropertyNames` +
  `getOwnPropertySymbols`).

## Plan (slices)
1. **Namespace + the easy ones**: add `Reflect` like `Math`/`JSON` (`ir::Namespace`, `Intrinsic::Namespace`,
   `builtins.rs` statics: `$$Reflect_get` is `Reflect.get`; T25's Notes explain namespaces as values). Remove it
   from the rejected globals. `get`/`has`/`ownKeys`/`getOwnPropertyDescriptor`/`getPrototypeOf`/`isExtensible`/
   `preventExtensions`/`apply` in `builtins/object.js` (or a new `builtins/reflect.js`, added like the others).
   Receivers for `get`/`set` need a runtime entry that takes a receiver: `props::Helper::SuperGet`/`SuperSet`
   already implement "the home object's prototype's property with `this` as receiver"; a Get/Set with an explicit
   receiver is the same walk starting at `o` (add intrinsics rather than faking it).
2. **`construct`**: `Intrinsic::ConstructList(callee, args, new_target, what)` already passes a `new.target`
   (used by `super(...args)`); expose it as `__intrinsic_construct(C, argsArray, newTarget)` after
   `__intrinsic_is_constructor` checks. Test `isConstructor.js`'s pattern directly.
3. **The boolean-returning writers**: `defineProperty`/`set`/`deleteProperty`/`setPrototypeOf` return `false` where
   `Object.*` / strict assignment throw. Either catch the TypeError in the prelude (simple; check it doesn't also
   catch user errors thrown by setters, which must propagate) or add non-throwing runtime variants.
4. **test262**: add `built-ins/Reflect` to `SUBSET` in `scripts/test262.mjs` and to the sparse checkout in
   `scripts/fetch-test262.sh` (ask before changing the subset: the baseline and HISTORY totals change); rerun the
   tests that failed with "global `Reflect`" (find them in `out/test262/results.tsv` of a full run, or grep the
   test262 sources for `Reflect\.`/`isConstructor.js` within the subset) with `--update`; new causes → follow-ups.

## Read first
- F152 (and F151, F106) in `tasks/README.md`; T25's Notes / Result (namespaces, built-ins as values, `names`
  gating); T35's Notes (`__intrinsic_is_constructor`, `__intrinsic_named`).
- `crates/lower/src/builtins.rs` (prelude conventions, statics), `crates/lower/src/lib.rs` (`builtin_static`,
  `Intrinsic::Namespace`, the unknown-global list near "ReferenceError", "Reflect"),
  `crates/lower/builtins/object.js` (`Object.*` in the prelude), `crates/backend/src/props.rs` (`super_get`,
  `super_set`, `define_property`), `crates/backend/src/func.rs` (construct, `ConstructList`).

## Tests
Fixture `tests/fixtures/builtins/reflect.js` (`scripts/diff.sh`; node is the oracle), `cargo test` (the
`unsupported.rs` case for `Reflect` changes), `wasm-tools validate` + one wasmtime run; test262 as in slice 4.

## Checklist
- [ ] Slice 1: namespace, `get`/`has`/`ownKeys`/descriptors/prototype/extensibility/`apply`
- [ ] Slice 2: `construct` with `newTarget`
- [ ] Slice 3: `defineProperty`/`set`/`deleteProperty`/`setPrototypeOf` returning booleans
- [ ] Slice 4: test262 (subset change asked first), follow-ups, baseline, docs (`docs/keywords.md`, README)

## Notes / Result

## Progress
- 2026-09-27: written as the handoff for a new session after T35 (the user chose `Reflect` over `globalThis`,
  RegExp, Date, BigInt). Not started. State at handoff: `main` = origin at `e3db0a0`; test262 subset 21,831 of
  24,820 run pass (88.0%); whole `diff.sh` 116 pass.
