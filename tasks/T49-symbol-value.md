# T49: `Symbol` as a value (F218, part of F106)

Status: todo (handoff written 2026-09-29, not started) | Depends on: T15 (symbols), T25 (constructor values,
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
- [ ] `Symbol` as a value behaves like node's (fixture).
- [ ] The 13 `testTypedArray.js` tests and the 7 others pass or have a named follow-up; 0 regressions; baseline
      updated.
- [ ] Programs not using `Symbol` as a value compile to the same bytes.

## Notes / Result
(to be written)

## Checklist
- [ ] design questions decided and recorded
- [ ] constructor value, statics, `typeof`
- [ ] fixture, gating check, test262 + baseline
- [ ] docs (HISTORY.md, keywords.md, design.md), follow-ups

## Progress
- 2026-09-29: handoff written after T48 (T48 suggested F218). State at handoff: `main` at `653bb27`, pushed; test262
  subset 27,159 of 28,512 run pass (95.3%).
