# T49: `Symbol` as a value (F218, part of F106)

Status: done (2026-09-29) | Depends on: T15 (symbols), T25 (constructor values,
well-known symbols, `builtins/symbol.js`), T45 (gating new built-in globals), T47/T48 (test262 harness adaptation)

## Goal
`Symbol` usable as a value, as in node 24.21.0: `typeof Symbol` (`"function"`), `const S = Symbol; S("x")`,
`x.constructor === Symbol`, `Symbol.name`/`length` (0), `new Symbol()` → `TypeError: Symbol is not a constructor`,
`Symbol.prototype` and its `constructor`, the statics on the value (`Symbol.for`/`keyFor`, the well-known symbols
`Symbol.iterator` … as non-writable, non-configurable data properties), `class X extends Symbol {}` evaluating (and
`new X()` throwing), `Reflect.construct(Symbol, [])` throwing. Programs that don't use `Symbol` as a value compile
to the same bytes.

## What we know (at handoff)
- **Today**: `Symbol(…)` calls lower to `Intrinsic::NewSymbol` (`crates/lower/src/lib.rs`, `builtin_ident(...) ==
  Some(Builtin::Symbol)`), `Symbol.iterator` & co to `Intrinsic::WellKnownSymbol(k)`, `Symbol.prototype` to
  `Intrinsic::Prototype`. `Symbol.for`/`keyFor` and the prototype methods are prelude code in
  `crates/lower/builtins/symbol.js`. Any other use of the name (`typeof Symbol`, `Symbol` as an argument) is
  "unsupported: global `Symbol`". `Builtin::Symbol` is in `Builtin::ALL` (its prototype global exists already).
- **Constructor values** (T25): `CTOR_VALUES` in `crates/lower/src/builtins.rs`, `func::Helper::BuiltinCtor(b)` in
  `crates/backend/src/func.rs` (call entry = the prelude function's adapter, construct entry; `BigInt` is the model:
  a call-only constructor whose construct entry is `Helper::NotConstructor`). Statics come from the prelude
  (`$$Symbol_for`, `$$Symbol_keyFor`); the well-known symbols would be new native statics (constants: read-only,
  fixed, hidden), made with `runtime::Helper::WellKnownSymbol(k)`.
- A call entry needs a prelude function `$$Symbol(description)` (`__intrinsic_…` for NewSymbol, ToString of the
  description unless `undefined`); check how `$$BigInt` / `$$DateCall` are wired as the call code.
- **test262 in the subset** (baseline `unsupported`/`fail` whose body uses `Symbol` other than `Symbol.x`/`Symbol(`):
  7 tests (`Array/fromAsync/asyncitems-uses-intrinsic-iterator-symbols.js`, 2 `class/subclass/builtin-objects/
  Symbol/*`, 3 RegExp `Symbol.matchAll`/`from-regexp-like` fails, `async-generator/default-proto.js`; check each cause,
  some may be other gaps). Plus the 13 tests including `testTypedArray.js` (its `typeof Symbol !== "undefined"`):
  11 `Array/prototype/*/callbackfn-resize-arraybuffer.js`, 2 `class/subclass/builtin-objects/TypedArray/*`.
  `built-ins/Symbol` isn't in the sparse clone/subset: consider adding it (`scripts/fetch-test262.sh` sparse paths,
  `SUBSET` in `scripts/test262.mjs`) as a later slice.

## Design questions (decide first, record in docs/design.md D5)
1. Gating: the constructor value is linked only when the code uses `Symbol` as a value (like the other
   `CTOR_VALUES`); confirm `typeof Symbol` alone can fold to `"function"` without linking anything.
2. The well-known symbols as properties of the value: all 14 (`WELL_KNOWN_SYMBOLS` in `crates/ir/src/lib.rs`) or
   only the ones named in the code (T25 links statics by name; unnamed ones then are missing from
   `Object.getOwnPropertyNames(Symbol)`).
3. `Symbol.prototype[Symbol.toPrimitive]` and `[Symbol.toStringTag]` if missing (check `Object.prototype.toString
   .call(Object(Symbol()))`).

## Plan
1. `$$Symbol` + `Symbol` in `CTOR_VALUES` + construct entry throwing; `typeof Symbol`; statics; fixture
   `tests/fixtures/builtins/symbol-value.js` (node is the oracle).
2. test262: the 20 tests above (targeted `--update`), then decide on adding `built-ins/Symbol`.
3. Docs (design.md D5, keywords.md `Symbol` row, HISTORY.md), follow-ups (F106/F218 ticks).

## Read first
- `docs/design.md` D5 (T25 slice 4: constructor values; T25 slice 5: well-known symbols).
- `crates/lower/builtins/symbol.js`; `func::builtin_ctor` in `crates/backend/src/func.rs`; `CTOR_VALUES`/`NEW_ONLY`
  in `crates/lower/src/builtins.rs`; the `Symbol` cases in `crates/lower/src/lib.rs` (grep `Builtin::Symbol`).
- F106, F218 in `tasks/README.md`.

## Tests
Fixture via `scripts/diff.sh`, `cargo test`, `wasm-tools validate` + wasmtime. Gating: old vs new build of every
fixture byte-identical except those using `Symbol` as a value (scratchpad `cmp.sh` from T42–T48: build
`old-js2wasi3` from the stashed tree). test262: targeted `--update`; ask before a full run on `a-03`.

## Acceptance
- [x] `Symbol` as a value behaves like node's (fixture).
- [x] The 13 `testTypedArray.js` tests and the 7 others pass or have a named follow-up; 0 regressions; baseline
      updated.
- [x] Programs not using `Symbol` as a value compile to the same bytes.

## Notes / Result
- **What changed**: `Symbol` is in `CTOR_VALUES` (`crates/lower/src/builtins.rs`); its call entry is `$$Symbol`
  (`builtins/symbol.js`, length 0), its construct entry `func::Helper::NotConstructor` (like `BigInt`). The value's own
  well-known symbols are those the code names (`ir::Module::symbol_statics`, installed in `func::builtin_ctor`:
  read-only, hidden, fixed). `Symbol.x` for a non-well-known `x` (`name`, `length`, `call`, …) reads the value.
  `Symbol(x);` as a statement and `new Symbol()` no longer "unsupported"; `Symbol(…)` calls don't link `$$Symbol`.
  `Symbol.prototype[Symbol.toPrimitive]` added (linked by naming `toPrimitive`). `Symbol.for` uses ToString
  (a symbol key throws). Found and fixed: a program naming `dispose` without a `DisposableStack` failed to build
  (the stacks' prototype methods were linked by name while their prototypes don't exist; T45 bug).
- **Design**: recorded in `docs/design.md` D5 (T49 paragraph): constructor value gated like the others (`typeof Symbol`
  links it, no folding); statics by name; `@@toPrimitive` added.
- **Gating**: every fixture builds to the same bytes except `symbol-value.js` (new), the 3 naming `toPrimitive`
  (`timers-more`, `date-convert`, `symbols`: `Symbol.prototype` gets `@@toPrimitive`) and the 2 `using` fixtures
  (they name `dispose`: the unreachable stack methods are gone); all pass `diff.sh`.
- **test262**: `built-ins/Symbol` joined the subset (`scripts/fetch-test262.sh`, `SUBSET`): 69 of 79 run pass.
  The 13 `testTypedArray.js` tests and the 2 `class/subclass/builtin-objects/Symbol` tests pass; the other 5 of the
  handoff's 20 have other causes: `Array.fromAsync/asyncitems-uses-intrinsic-iterator-symbols.js` replaces the global
  `Symbol` (F201), 2 `Symbol.matchAll` species tests (F172), `RegExp/from-regexp-like-flag-override.js` (F223),
  `async-generator/default-proto.js` (F222). Symbol's 10 left: `description` of `Symbol("")` (3, F110), species
  getters (2, F172), `Proxy` (F152), ordinary ToPrimitive of wrappers (2, F220), `$$describe` trap (F221),
  `symbol.js` checks the global `this` (a module's is `undefined`). Subset 27,243 of 28,591 run pass (95.3%),
  0 regressions (targeted `--update`, confirmed by a full run on `a-03`).
- **Commands**: `scripts/diff.sh tests/fixtures/builtins/symbol-value.js` (+ the changed fixtures), `cargo test`,
  `wasm-tools validate`, `node scripts/test262.mjs --update built-ins/Symbol <26 tests>`, and a check of
  `built-ins/{,Async}DisposableStack`, `SuppressedError`, `statements/{using,await-using,class/subclass}`,
  `Array/fromAsync` (no changes).
- **Next smallest step**: F220 (ToPrimitive of wrappers through the real `valueOf`/`toString` lookups) or F110
  (`Symbol("")`'s description needs a flag on `$symbol`).

## Checklist
- [x] design questions decided and recorded
- [x] constructor value, statics, `typeof`
- [x] fixture, gating check, test262 + baseline
- [x] docs (HISTORY.md, keywords.md, design.md), follow-ups

## Progress
- 2026-09-29: handoff written after T48 (T48 suggested F218). State at handoff: `main` at `653bb27`, pushed; test262
  subset 27,159 of 28,512 run pass (95.3%).
- 2026-09-29: started. Decisions: (1) `Symbol` is a `CTOR_VALUES` entry, linked only when used as a value
  (`typeof Symbol` is a use: it links the small constructor, no folding); `Symbol(…)` calls stay
  `Intrinsic::NewSymbol` and no longer root `$$Symbol`. (2) Only the well-known symbols the code names are own
  properties of the value (`ir::Module::symbol_statics`, like T25's statics by name). (3) Added
  `Symbol.prototype[Symbol.toPrimitive]` (`$$Symbol_prototype_$toPrimitive`, linked by naming `toPrimitive`);
  `@@toStringTag` was already there. Construct entry: `Helper::NotConstructor` like `BigInt`.
- 2026-09-29: fixture `tests/fixtures/builtins/symbol-value.js` passes `diff.sh`.
- 2026-09-29: gating check (scratchpad `cmp.sh`, old = HEAD in a worktree): only `symbol-value.js` and the 3 fixtures
  naming `toPrimitive` (`timers-more`, `date-convert`, `symbols`: `Symbol.prototype` gains `@@toPrimitive`, gated like
  Date's) change; all 4 pass `diff.sh`. (First try edited the shared `$$thisSymbol` and changed ~40 fixtures: the
  `@@toPrimitive` check is now its own.) `cargo test` green (the `unsupported.rs` case for `const s = Symbol` now lowers).
- 2026-09-29: `built-ins/Symbol` added to the subset: first run 61/79; fixed `Symbol(x);` statements, `new Symbol`,
  `Symbol.for` ToString, and the `dispose`-without-stacks build failure (pre-existing): 69/79. Targeted `--update`:
  0 regressions, 15 unsupported → pass. Docs, follow-ups F220–F223; task done.
- 2026-09-29: full subset run on `a-03`: 27,243 of 28,591 run pass (95.3%), baseline unchanged (one flaky timeout, F185).
