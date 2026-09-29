# T48: `BigInt64Array` / `BigUint64Array` (F217, part of F116)

Status: done (2026-09-29) | Depends on: T20 (typed arrays), T40 (BigInt), T47 (resizable buffers, runner harness adaptation)

## Goal
`BigInt64Array` and `BigUint64Array` as node 24.21.0 has them: constructors (length, buffer, typed array of the same
content type, iterable, array-like), `from`/`of`, `BYTES_PER_ELEMENT`, element reads (BigInts) and writes (ToBigInt:
`Cannot convert 1 to a BigInt`), every `%TypedArray%.prototype` method with BigInt values, content-type mismatches
(`Cannot mix BigInt and other types, use explicit conversions`), printing (`BigInt64Array(2) [ 1n, 2n ]`), over
resizable buffers too. Then drop their entries from the test262 runner's `HARNESS_ADAPT`.
Programs that don't name them compile to the same bytes.

## Design
- Kinds 9 (`BigInt64Array`) and 10 (`BigUint64Array`), 8-byte elements; `TYPED_NAMES`/`TYPED_SIZES` get 11 entries.
- Gated by `ir::Module::big_typed` (the user's code names either global; implies `Module::bigint`). Like T40/T45:
  `Builtin::Typed(9|10)` are outside `Builtin::ALL`; their prototype and constructor globals come after the stacks'
  (`runtime::runtime_globals`: each group brings the ones before it). Every loop over the kinds in the backend goes
  to 11 only then.
- Element reads build a `$bigint` from the 8 bytes; writes from user code call the prelude's `$$toBigInt`
  (`Module::to_bigint`) then store the two's complement's low 64 bits.
- Prelude: `$$BigInt64Array`/`$$BigUint64Array` (+ `from`/`of`); methods that convert with `+value` (`fill`, `with`)
  and mix kinds (`set`, constructors from a typed array) branch in `if (__intrinsic_big_typed_mode()) { … }` blocks.

## Tests
Fixture `tests/fixtures/builtins/bigint-typed-arrays.js` (node is the oracle); `cargo test`; gating (`cmp.sh`);
test262: the 89 `resizable-arraybuffer` tests (3 name `BigInt64Array`) with the adaptation entries removed, plus
the tests naming `BigInt64Array`/`BigUint64Array`, `--update`.

## Acceptance
- [x] The fixture matches node.
- [x] `HARNESS_ADAPT` no longer hides the BigInt arrays; the resizable-buffer tests pass or have a follow-up.
- [x] Programs not naming them compile to the same bytes.

## Notes / Result
**What changed**: see the Progress entry "implemented" and docs/design.md's T48 note. Printing needed nothing new
(elements are BigInts, the formatter already groups them like node).

**Commands run**: `scripts/diff.sh tests/fixtures/builtins/bigint-typed-arrays.js` (pass); `cargo test` (pass);
`wasm-tools validate` + `wasmtime run -Sp3`; scratchpad `cmp.sh` over all fixtures (identical except the new one);
test262 targeted `--update` over 96 tests: 7 unsupported → pass, 0 regressions, subset 27,159 of 28,512 run (95.3%).

**Unsupported / follow-ups**: WIT `list<s64>`/`list<u64>` as BigInt arrays stays F124; `Float16Array` stays in F217;
`typeof Symbol` (F218) blocks the last 11 `resizable-arraybuffer` tests.

**Next smallest step**: F218 (`Symbol` as a value) unblocks those 11 and the 2 `class/subclass-builtins/TypedArray`
tests.

## Checklist
- [x] IR/backend kinds 9–10, gating, element access, printing
- [x] prelude constructors and methods
- [x] fixture, gating check, test262 + baseline, docs, follow-ups

## Progress
- 2026-09-29: started after T47 (the user chose F217's BigInt arrays). Probed node: writes use ToBigInt (`Cannot
  convert 1 to a BigInt`, `SyntaxError: Cannot convert x to a BigInt`); a typed array of the other content type as a
  constructor or `set` source: `TypeError: Cannot mix BigInt and other types, use explicit conversions`; printing
  groups `n` values in columns like numbers.
- 2026-09-29: implemented: IR kinds 9–10 (`TYPED_NAMES`/`TYPED_SIZES` 11 entries, `TYPED_KINDS`, `TYPED_BIGINT64`,
  `Builtin::parse`), `Module::{big_typed, to_bigint}`; backend globals `G_BIG_TYPED_PROTOS/CTORS` after the stacks'
  (`runtime_globals` 4th flag), `Cx::{big_typed, typed_kinds, to_bigint}`, `typed::Helper::Get` builds a `$bigint`
  (sign + |value|, `Norm`), `Helper::SetBig` (`ToTc(b, 2)`), `object.rs` element writes call `$$toBigInt` for kinds
  ≥ 9; prelude constructors/`from`/`of`, `$$typedSameContent` (node's "Cannot mix BigInt…"), `fill`/`with` branches,
  all in `__intrinsic_big_typed_mode()` blocks (new gate). Fixture `tests/fixtures/builtins/bigint-typed-arrays.js`
  passes `diff.sh`. Runner: the BigInt entries of `HARNESS_ADAPT` removed (`MyBigInt64Array` written out); the 89
  `resizable-arraybuffer` tests: 78 pass, 11 blocked by `typeof Symbol` (F218).
