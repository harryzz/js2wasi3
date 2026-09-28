# T37: `Object.setPrototypeOf` (F164)

Status: done (2026-09-28) | Depends on: T36 (`$$Reflect_setPrototypeOf`, `set_proto`)

## Goal
`Object.setPrototypeOf(o, proto)` works like node's. Today it is rejected at build time ("unsupported:
`Object.setPrototypeOf`"; test262: 34 in the subset, and `keywords.md` wrongly listed it as done until T36).

## What node does (node 24.21.0, checked)
- `undefined`/`null` target: `TypeError: Object.setPrototypeOf called on null or undefined`.
- `proto` neither an object nor `null`: `TypeError: Object prototype may only be an Object or null: <v>`.
- A primitive target (after both checks): returned unchanged.
- [[SetPrototypeOf]] false: `#<Object> is not extensible` (`[object Array]`, `#<K>`; a function: its source),
  `Cyclic __proto__ value`, `Immutable prototype object 'Object.prototype' cannot have their prototype set`.
- Returns the target; `length` 2.

## Plan
1. Share `Reflect.setPrototypeOf`'s steps in `builtins/reflect.js` as one function returning why it failed; `$$Object_setPrototypeOf`
   in `builtins/object.js` throws node's TypeErrors from it.
2. Fixture `tests/fixtures/builtins/set-prototype-of.js`; test262: rerun the subset tests that mention
   `Object.setPrototypeOf` and aren't passing, with `--update`.
3. F167 (`Reflect.hasOwnProperty`, `Math.hasOwnProperty`: an `Object.prototype` method read from a namespace) if it
   is small; otherwise leave it.

## Read first
- T36's Notes / Result; `crates/lower/builtins/reflect.js`, `object.js`.

## Tests
The fixture (`scripts/diff.sh`), `cargo test -p js2wasi3-lower`, `wasm-tools validate` + one wasmtime run, the
targeted test262 rerun.

## Checklist
- [x] `Object.setPrototypeOf`
- [x] fixture + test262
- [x] F167
- [x] docs, follow-ups, README status

## Notes / Result
**What changed**
- `builtins/reflect.js`: `$$setPrototypeOf(o, proto)` (status 0 done, 1 not extensible, 2 cycle, 3 `Object.prototype`)
  and `$$checkProto`, shared by `Reflect.setPrototypeOf` and the new `$$Object_setPrototypeOf` in `builtins/object.js`
  (node's TypeErrors; primitives returned unchanged).
- F167: `builtins::inherited_prop(global, prop)`: `Function.prototype`'s names on constructor values (as before) and
  `Object.prototype`'s methods on namespaces (`OBJECT_PROTO_PROPS`) are read from the value instead of being rejected
  as missing statics (`Reflect.hasOwnProperty("x")`, `Math.constructor`); `Math.x` reads other than constants use it too.

**Commands**: fixture `tests/fixtures/builtins/set-prototype-of.js` (and all builtins fixtures, then the whole
`scripts/diff.sh`: 118 pass), `cargo test -p js2wasi3-lower`, `wasm-tools validate` + wasmtime run, test262 `--update` on
the 35 `setPrototypeOf` tests (19 → pass) and the namespace-method one (→ pass), regression run
`built-ins/{Math,JSON,Reflect}` (no changes). Subset: 22,166 of 24,973 run pass (88.8%, was 22,146).

**Unsupported / left**: `super.x` with a `null` or changed home prototype (F168); a function named in the
"is not extensible" TypeError prints `#<Object>` (F165); `console.log(Object.prototype)` prints `{}` where node prints
`[Object: null prototype] {}` (inspect detail, not recorded separately).

**Next smallest step**: F168 (super references, 5 tests), or pick the next task.

## Progress
- 2026-09-28: written and started (the user chose F164 after T36). node's messages checked (above).
- 2026-09-28: `$$setPrototypeOf` (status 0–3) shared by `Reflect.setPrototypeOf` and the new `$$Object_setPrototypeOf` (object.js); fixture `builtins/set-prototype-of.js` and `reflect.js` pass; validate OK.
- 2026-09-28: test262: 35 non-passing subset tests mentioning `setPrototypeOf`: 19 → pass, 5 → fail (super references with a null/changed home prototype: F168), 11 other causes; baseline 22,165 of 24,973 (88.8%).
- 2026-09-28: F167: `builtins::inherited_prop` (FUNCTION_PROPS, plus `Object.prototype` methods for namespaces) lets `Reflect.hasOwnProperty(…)`, `Math.constructor` go through the namespace value; fixture extended; builtins fixtures 34 pass; lower tests pass.
