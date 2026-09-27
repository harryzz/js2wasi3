# T35: arrays, the rest of F108 (generic native methods, `isConcatSpreadable`, species, `fromAsync`)

Status: done | Depends on: T34

## Goal
Close F108's array items, measured on test262 `built-ins/Array` (493 not passing after T34): the native
`push`/`pop`/`join`/`toString` on non-arrays (40), `concat` and `Symbol.isConcatSpreadable` (44), species
(ArraySpeciesCreate: `constructor[Symbol.species]` for `map`/`filter`/`slice`/`splice`/`concat`/`flat`/`flatMap`, 60),
`Array.fromAsync` (93). Iterator helpers (node 22+) stay in F108: `built-ins/Iterator` isn't in the subset.

## Plan (slices)
1. **Generic native methods**: the prelude's `$$Array_prototype_{push,pop,join,toString}` (spec algorithms) are not
   installed; the lowering lists them in `ir::Module::array_generic` and the native methods call them (`$code`
   adapters, same signature) when `this` isn't an array. Arrays keep the native fast path.
2. **`concat`**: IsConcatSpreadable (`Symbol.isConcatSpreadable`, then IsArray), `length` limits.
3. **Species**: ArraySpeciesCreate in the prelude (`constructor`, `Symbol.species`, IsConstructor, `new C(n)`).
4. **`Array.fromAsync`**: an async function in the prelude (async iterables, then sync iterables/array-likes with
   awaited values, `mapfn`, a constructor `this`).
5. **test262**: `built-ins/Array` with `--update`; follow-ups; baseline.

## Read first
F108 in `tasks/README.md`; T21's Notes (prelude arrays), T34's; `crates/lower/builtins/array.js`,
`crates/backend/src/array.rs` (`this_array`), `crates/lower/src/lib.rs` (`declare_builtin_methods`).

## Tests
A fixture per slice in `tests/fixtures/builtins/` (`scripts/diff.sh`), `tests/fixtures/arrays`, `cargo test`,
`wasm-tools validate` + one wasmtime run.

## Checklist
- [x] Slice 1: generic `push`/`pop`/`join`/`toString`
- [x] Slice 2: `concat`, `Symbol.isConcatSpreadable`
- [x] Slice 3: species
- [x] Slice 4: `Array.fromAsync`
- [x] Slice 5: test262, follow-ups, baseline

## Notes / Result
**What changed**
- Generic native methods: the prelude's `$$Array_prototype_{push,pop,join,toString}` are the spec algorithms; the
  lowering doesn't install them (`builtins::ARRAY_NATIVE`) but lists them in `ir::Module::array_generic`, and
  `array::this_array` calls that `$code` when `this` isn't an array. Arrays keep the native code.
- `concat`: IsConcatSpreadable (`$$spreadable`), array-likes, the 2**53-1 limit.
- Species: `$$speciesCreate` (ArraySpeciesCreate) for `map`, `filter`, `slice`, `splice` (also sets the removed
  array's `length`), `concat`, `flat`, `flatMap`; `$$put` (CreateDataPropertyOrThrow: `defineProperty` for non-array
  results). Two new prelude intrinsics: `__intrinsic_is_constructor(v)` (`Intrinsic::IsConstructor`) and
  `__intrinsic_named("x")`, a compile-time constant: does the user's code name property `x`. Built-in constructors
  have no `Symbol.species` here, so without `species` in the code the species path is skipped, which also avoids
  reading `Array.prototype.constructor` (a trapping sentinel unless the code names `constructor`).
- `Array.fromAsync` (prelude async function over `for await`; array-likes await each value; `mapfn` gets two
  arguments and its result is awaited) and `Array.from` with a constructor `this` (`$$fromCreate`, `$$call2`).

**Tests**: fixtures `tests/fixtures/builtins/array-{generic,concat,species,from-async}.js` (= node); whole
`diff.sh`: 116 pass; `cargo test`, clippy; validate + wasmtime run.

**test262**: `built-ins/Array` and `language/statements/for-of` (3,833 tests, 1,244 s at -j 4, `--update`): 105 more
pass (100 fail, 3 timeout, 2 unsupported → pass), 1 unsupported → fail, 0 regressions. `built-ins/Array` 2642 of 3030
(87.2%, was 83.7%); subset 21,831 of 24,820 run (88.0%). `Array.fromAsync`'s tests mostly use `globalThis` (88,
F151); the 3 that run pass.

**Left** (F108, narrowed): subclassing `Array` (`class X extends Array`: the prelude constructor ignores
`new.target`, so there is no species through inheritance, F106), iterator helpers and other node 22–24 additions.
Also left in `built-ins/Array`: indices ≥ 2^26 (F160), `Reflect`/`Proxy`/`globalThis` (F151, F152).

**Next smallest step**: no task proposed.

## Progress
- 2026-09-27: task written for F108 at the user's request (scope: its array items; counts from the baseline).
- 2026-09-27: slice 1 implemented: prelude generic push/pop/join/toString (`$$genericThis`: V8's "Cannot convert
  undefined or null to object"; push's 2**53-1 message), `ir::Module::array_generic` + `builtins::ARRAY_NATIVE`,
  `array::this_array` calls the fallback. Probe = node.
- 2026-09-27: slice 2: `concat` with IsConcatSpreadable (`$$spreadable`), array-likes, 2**53-1 check; fixtures builtins/array-generic.js, array-concat.js pass.
- 2026-09-27: slice 3: species: `$$speciesCreate` (ArraySpeciesCreate), `$$put` (CreateDataPropertyOrThrow), new intrinsics `is_constructor` (IR IsConstructor, func.rs) and `named` (compile-time: does the user's code name the property; species is skipped without `species`, so plain programs never read `constructor`, whose sentinel would trap). map/filter/slice/splice(+removed.length)/concat/flat/flatMap. Fixture builtins/array-species.js; 48 fixtures pass.
- 2026-09-27: slice 4: `Array.fromAsync` (prelude async function over `for await`; array-likes await each value; mapfn awaited; V8's messages), `Array.from` uses a constructor `this` (`$$fromCreate`), `$$call2` (mapfn gets two arguments). Fixture builtins/array-from-async.js; 54 fixtures pass.
- 2026-09-27: slice 5: test262 +105, 0 regressions; diff.sh 116 pass. Task done.
