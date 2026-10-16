# T121: `%TypedArray%`, typed `[[Set]]` with another receiver, integrity on typed arrays (F430–F433)

Status: done | Depends on: T20, T97, T120 | Local (+ a-03 only after asking)

## Goal
The typed-array gaps left by T120 (user, 2026-10-16: "analyse them and finish in one task"):
- F430: `Object.getPrototypeOf(Int8Array)` is `Function.prototype`; ECMA-262 23.2.1 `%TypedArray%` (name
  `TypedArray`, length 0, throws "Abstract class TypedArray not directly constructable" when called or
  constructed) owns `from`, `of`, `prototype` (the shared `typed_array_prototype`, whose `constructor` it is) and a
  `Symbol.species` getter; every typed-array constructor's [[Prototype]] is it.
- F431: typed `[[Set]]` (10.4.5.5) with a receiver other than the typed array: an invalid index returns true
  without writing (`Object.create(t)[5] = v` creates a property today; `Reflect.set(t, 7, v, o)` writes `o[7]`).
- F432 / F433: `Reflect.defineProperty` on a non-extensible typed array, `Reflect.set` with a symbol key: a probe
  (2026-10-16) matches node after T120; to confirm with test262.
- Found by the probe: `Object.freeze`/`Object.seal` of a typed array with elements throws node's TypeError
  ("Cannot freeze array buffer views with elements"); `isFrozen`/`isSealed` follow.

## Read first
- ECMA-262 23.2.1–23.2.2, 10.4.5.5, 20.1.2.6 (`Object.freeze`), `SetIntegrityLevel`.
- `crates/backend/src/typed.rs`, the typed constructors' setup, `object.rs` `Builtin::Typed`, `props.rs` `Integrity`.

## Plan
- [x] Probe `scratchpad/t121.js` vs node; test262 `TypedArrayConstructors/internals` before (`out/t262-t120-internals.log`).
- [x] F430, F431, integrity; F432/F433 confirmed.
- [x] Fixture `builtins/typed-array-intrinsic.js`; `scripts/diff.sh` on typed fixtures; internals tests again.
- [x] Full test262 on a-03 (ask); gate only if hot paths change.

## Progress
- 2026-10-16: task created. Probe vs node: F430 missing (constructors' [[Prototype]] is `Function.prototype`);
  F431: `child[0] = 5` through a typed prototype already right (T120), `child[5] = 5` creates a property (spec:
  nothing; node 24 reaches a setter further up, test262 `key-is-canonical-invalid-index-prototype-chain-set` follows
  the spec: the spec decides), `Reflect.set(t, 7, v, o)` writes `o[7]` (spec and node: no); F432/F433 = node;
  freeze/seal of a typed array with elements don't throw.
- 2026-10-16: F430: prelude `$$TypedArray` (throws "Abstract class TypedArray not directly constructable"),
  `$$TypedArray_from`/`_of` (kind from `this` by the new `TypedOp::CtorKind`, `__intrinsic_typed_ctor_kind`: `this`
  or the first typed constructor on its chain, compared with the constructor globals; else "X is not a
  constructor"), `$$TypedArray_$species` (a `$name` static is a getter at `Symbol.name`, func.rs `builtin_ctor`).
  Lowering: any typed constructor as a value adds `TypedArray` to `ctor_values`; per-kind `from`/`of` aren't own
  statics of the values any more (direct `Int8Array.from(…)` calls still go to `$$Int8Array_from`).
  `builtin_ctor`: name `TypedArray`, length 0, and each typed constructor's [[Prototype]] is it.
- 2026-10-16: F431: `Reflect.set` (prelude) walks the chain with typed [[Set]]: the receiver itself →
  TypedArraySetElement; a valid element on the chain → a data property on the receiver; an invalid index → true,
  nothing. `Reflect.get` stops at a typed array's numeric key. `props::set`'s chain walk: a typed array on the chain
  with a numeric key: valid → a new own property of the receiver (no setter search), invalid → nothing. (node 24
  reaches a setter further up in both; the spec and test262 decide.) Integrity: `Object.freeze`/`seal` of a typed
  array with elements: not extensible, then node's TypeError; `isFrozen` false with elements (`isSealed` unchanged:
  node says true for a non-extensible one with elements, the spec false).
- 2026-10-16: probe = node except `Int8Array.from === TA.from` (a member read of a direct static gives the per-kind
  function: follow-up) and the two spec-vs-node chain writes. internals (`out/t262-t121-internals.log`): 160 pass
  (was 107), 0 newly failing; the 52 left need `$262` (detach), `SharedArrayBuffer` or `Proxy`; F432/F433's tests
  pass. Fixture `builtins/typed-array-intrinsic.js` = node. Fixtures `builtins` 75 (1 skip), `objects` 19, `arrays` 9,
  `classes` 12, `closures` 11 (1 skip), `syntax` 10 (1 skip), `node` 42 (`fetch.js` timed out under load, passes
  alone in 34 s).
- 2026-10-16: user: ok. a-03 gate vs `c7af6e6` (`~/gate-t121.log`), then full test262 (`~/t262-t121.log`): started.
- 2026-10-16: wasmtime 49.0.3 (2026-10-14: crash fixes in CM async, streams, exception tags, recursive types) and
  50.0.0-rc.1 (GC field/array alias regions, #14351/#14353) checked; user: upgrade the pin to 49.0.3 as a small task
  after T121 (not during the a-03 run); measure 50.0.0 when final.
- 2026-10-16: a-03 (`out/gate-t121-a03.log`, vs `c7af6e6`): every program at equal instructions (±0.01%);
  mandelbrot +2.2%, revcomp −3.2% (placement); micro 3 slower (`v == null [symbol]` +2, `typeof v [boolean]` +2, a
  2k-op regex case), 1 faster: the noise level of identical code (T119). test262 (`out/t262-t121.log`): no changes,
  31,461 tests in 1,534 s. Task done.

## Notes / Result
- **What changed.**
  - F430: `%TypedArray%` (prelude `$$TypedArray`, `$$TypedArray_from`/`_of`/`_$species`; `TypedOp::CtorKind`
    `__intrinsic_typed_ctor_kind`): name `TypedArray`, length 0, throws node's "Abstract class TypedArray not
    directly constructable", owns `prototype` (`%TypedArray%.prototype`, whose `constructor` it is), `from`/`of`
    (kind from `this`, a subclass's chain included) and a `Symbol.species` getter (a `$name` static is a getter at
    `Symbol.name`); every typed constructor's [[Prototype]] is it (`func.rs` `builtin_ctor`). Per-kind `from`/`of`
    aren't own statics of the constructor values any more; direct `Int8Array.from(…)` calls still compile to
    `$$Int8Array_from`.
  - F431: typed [[Set]] with another receiver (10.4.5.5): `Reflect.set`'s chain walk (prelude) and `props::set`'s
    (native): a valid element on the chain is a writable data property (defined on the receiver, no setter
    search), an invalid index does nothing; `Reflect.get` stops at a typed array's numeric key.
  - F432/F433: fixed by T120 (their test262 tests pass).
  - Integrity: `Object.freeze`/`seal` of a typed array with elements: not extensible, then node's TypeError;
    `isFrozen` false with elements.
- **Result:** test262 `built-ins/TypedArrayConstructors/internals` (laptop, not in the subset) 107 → 160 of 240
  (T120 started at 71), 0 newly failing; the 52 left need `$262` (detach), `SharedArrayBuffer` or `Proxy`. Full
  test262 (a-03): no changes. Fixture `builtins/typed-array-intrinsic.js` = node. Gate (a-03): no instruction
  changes, placement and noise only.
- **Differences from node, deliberate:** writes through a typed array on another object's prototype chain follow
  the spec (node 24 reaches a setter further up; test262 checks the spec); `Object.isSealed` of a non-extensible
  typed array with elements stays true (node; the spec says false).
- **Commands:** probe `scratchpad/t121.js`; `scripts/diff.sh` on `tests/fixtures/{builtins,objects,arrays,classes,
  closures,syntax,node}`; `node scripts/test262.mjs --depth 5 --list built-ins/TypedArrayConstructors/internals`;
  `scripts/gate.sh -o out/gate-t121 c7af6e6` and `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** F434 `Int8Array.from` read as a value isn't `%TypedArray%.from`; F435 `isSealed`
  (spec vs node).
- **Next smallest step:** the wasmtime 49.0.3 pin (user, 2026-10-16), then choose among F420, F427, F404.

