# T36: `Reflect` (F152, without `Proxy`)

Status: done (2026-09-28) | Depends on: T25 (namespaces, built-ins as values), T32 (element
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
- [x] Slice 1: namespace, `get`/`has`/`ownKeys`/descriptors/prototype/extensibility/`apply`
- [x] Slice 2: `construct` with `newTarget`
- [x] Slice 3: `defineProperty`/`set`/`deleteProperty`/`setPrototypeOf` returning booleans
- [x] Slice 4: test262 (subset change asked first), follow-ups, baseline, docs (`docs/keywords.md`, README)

## Notes / Result
**What changed**
- `ir::Namespace::Reflect`: `Reflect` is a namespace like `Math`/`JSON` (`object::CLASS_REFLECT` = 9, `[object Reflect]`,
  `console.log` prints `Object [Reflect] {}`); `Reflect.x(…)` is a direct call of `$$Reflect_x`.
- `crates/lower/builtins/reflect.js`: all 13 functions in the prelude. `get`/`set` with a receiver are the spec's
  OrdinaryGet / OrdinarySetWithOwnDescriptor walks over `Object.getOwnPropertyDescriptor`/`getPrototypeOf` (no new
  runtime entry: the walk also covers arrays and wrappers, which `props::super_get`/`super_set` don't); the receiver ===
  target case is a plain `o[k]`. The writers that return `false` (`defineProperty`, `set`, `deleteProperty`) run all
  user code first (ToPropertyKey, ToPropertyDescriptor with node's messages, ArraySetLength's two conversions) and
  catch only the native `Object.defineProperty`/`delete` rejection, so a setter's TypeError propagates.
  `setPrototypeOf`: node's checks (non-extensible, cycles, `Object.prototype` immutable), then `set_proto`. Typed-array
  element keys go straight to the element (F163).
- Prelude intrinsics: `call_list(f, this, args)` (`Intrinsic::CallList`), `construct_list(f, args, newTarget)`
  (`Intrinsic::ConstructList`), `set_proto`, `to_key` (`Intrinsic::ToPropertyKey`), `object_desc` (new
  `Intrinsic::ObjectDesc` → `props::Helper::Desc`).
- Built-in functions as values (`$$…` through `func_value`) are `FnKind::Plain`: `Math.max`, `"".at` etc. are not
  constructors (test262's `not-a-constructor.js` tests via `isConstructor.js`).
- test262: `built-ins/Reflect` added to `SUBSET` and to `scripts/fetch-test262.sh` (the sparse-checkout list is now set
  on every run, so an existing clone gets new directories).

**Commands**: `scripts/diff.sh tests/fixtures/builtins/reflect.js` (new fixture, node is the oracle), whole
`scripts/diff.sh` (117 pass), `cargo test` (all pass; `unsupported.rs` now checks `Proxy` instead of `Reflect`),
`wasm-tools validate` + `wasmtime run -Sp3` of the fixture, `node scripts/test262.mjs --update` on the 239 subset tests
that use `Reflect` (179 → pass) and on `built-ins/Reflect` (135 of 136 run pass; 17 unsupported: `Proxy` 10,
`Function`/`Date` 4, `Object.setPrototypeOf` F164, `Reflect.hasOwnProperty` F167; 1 fail: top-level `this`).
Regression check: every `built-ins/*` directory of the old subset (6,116 tests): 1 fail → pass, 0 regressions.
Subset: 22,146 of 24,973 run pass (88.7%, was 21,831 of 24,820, 88.0%).

**Unsupported / left**: `Proxy` (F152); built-in constructors ignore `newTarget` (`Reflect.construct(Array, [], B)`,
F106); function values named in TypeErrors print `#<Object>` (F165); `new.target` in plain functions (F57); typed-array
elements in the descriptor functions (F163); `Object.setPrototypeOf` (F164).

**Next smallest step**: F164 (`Object.setPrototypeOf` from `$$Reflect_setPrototypeOf`, 34 test262 tests).

## Progress
- 2026-09-27: written as the handoff for a new session after T35 (the user chose `Reflect` over `globalThis`,
  RegExp, Date, BigInt). Not started. State at handoff: `main` = origin at `e3db0a0`; test262 subset 21,831 of
  24,820 run pass (88.0%); whole `diff.sh` 116 pass.
- 2026-09-28: started. Slices 1–3 in one go: `ir::Namespace::Reflect` (`CLASS_REFLECT` = 9, tag, `Object [Reflect]`),
  `builtins/reflect.js` (all 13 functions in the prelude: OrdinaryGet/OrdinarySet walks with a receiver over
  `Object.getOwnPropertyDescriptor`/`getPrototypeOf`; the writers convert key/descriptor/array length first and
  catch only the `Object.defineProperty`/`delete` rejection), prelude intrinsics `call_list`, `construct_list`,
  `set_proto`, `to_key`, `object_desc` (new `Intrinsic::ObjectDesc` → `props::Helper::Desc`). Prelude function
  values are `FnKind::Plain` now (`Math.max` isn't a constructor). `tests/fixtures/builtins/reflect.js` and all 33
  builtins fixtures pass.
- 2026-09-28: typed arrays: `Reflect.get/set` on an element key go straight to the element (the native
  descriptor functions don't see typed-array elements; `ta["0"] = v` makes a dictionary property: F163).
  test262: the 239 in-subset tests that use `Reflect` (source or harness `isConstructor.js`/`deepEqual.js`/…) and
  didn't pass: 179 pass now (`--update`); baseline 22,010 of 24,820 run pass (88.7%, was 88.0%). The rest: `import()`
  24, `globalThis` 13, `isConstructor` of missing statics (`String.raw`, `Promise.try`, `JSON.rawJSON`) 8, `Proxy` 6,
  regex methods 6, `new.target` in functions 4, native `array.length = v` coercion order 2. Whole `diff.sh`: 117 pass.
- 2026-09-28: regression run over all old-subset `built-ins/*` (6,116 tests): 0 regressions, 1 more pass; baseline
  22,146 of 24,973 (88.7%); HISTORY entry written; task done.
