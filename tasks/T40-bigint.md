# T40: BigInt (F153)

Status: todo (handoff written 2026-09-28, not started) | Depends on: T21 (prelude), T25 (built-ins as values), T30 (wrappers)

## Goal
BigInt as in ECMAScript: literals (`123n`, `0x1fn`, …), `typeof` `"bigint"`, the operators (`+ - * / % **`, unary
`-`, `~`, `& | ^ << >>`, `++`/`--`, comparisons with numbers and strings, `==`/`===`), `BigInt(…)`,
`BigInt.asIntN`/`asUintN`, `BigInt.prototype.toString(radix)`/`valueOf`/`toLocaleString`, `String(1n)`/template
literals, `Number(1n)`, printing like node (`123n`), and the errors V8 gives (mixing with numbers, `+1n`, `>>>`,
division by zero, negative exponents, `JSON.stringify`). Programs without BigInt must compile exactly as before.

## What we know (at handoff)
- Today a BigInt literal is rejected in `crates/lower/src/lib.rs` (`Expression::BigIntLiteral` → "unsupported:
  BigInt", "not scheduled"); `BigInt` is in the list of known node globals (rejected as a global). `ir::TypeofTag`
  already has `BigInt` (`"bigint"`). D1 (docs/design.md) planned "GC structs for … bigints".
- test262: **201 subset tests are "unsupported: BigInt"** (152 in `language/expressions`, the rest spread over
  JSON/String/Array/Set/Map/Promise/Date built-ins). `built-ins/BigInt` (~75 files) is **not** in the subset or the
  sparse checkout yet (`scripts/fetch-test262.sh`, `SUBSET` in `scripts/test262.mjs`); add it like T26 added
  `built-ins/Date`.
- Related follow-ups: F116 (`BigInt64Array`/`BigUint64Array`), F124 (WIT `s64`/`u64` are numbers, jco uses BigInt).
- Values: a JS value is `(ref null eq)`; numbers are `i31ref` or `$number` (boxed f64); strings `$string`
  (`(array i16)`), symbols a subtype of it. `$strict_eq` (`crates/backend/src/runtime.rs`) compares numbers by value,
  strings by content, **everything else by identity**: a BigInt must compare by value there, in `==`, in
  SameValue/SameValueZero (`Object.is`, `Map`/`Set` keys: `crates/backend/src/mapset.rs` hashing) and in
  `Array.prototype.includes`/`indexOf`.
- Numeric operators: the lowering infers kinds (`crates/lower`, "Type inference") and emits `f64` arithmetic, with
  ToNumber on JS values. A BigInt operand must never reach that path: ToNumber of a BigInt throws
  ("Cannot convert a BigInt value to a number"), and `a * b` on two values that may be BigInts needs a runtime
  dispatch (ToNumeric on both; both BigInt → BigInt op; one of each → TypeError "Cannot mix BigInt and other
  types, use explicit conversions").
- wasmtime 49 has `wide-arithmetic` (i64 multiply with a 128-bit result) only behind `-W wide-arithmetic`; the
  project uses default features only, so limbs should be 32-bit with `i64` products (or 64-bit limbs with the
  schoolbook split).

## Design questions (decide first, record under D1/D5 in docs/design.md)
1. **Representation**: a `$bigint` GC struct (sign + magnitude limbs, e.g. `(ref $i32s)` little-endian 32-bit
   limbs, normalized: no leading zero limbs, 0 has no limbs and sign +), a new subtype of `eq`. Immutable, so
   `===` is structural. Or a small-value fast path (fits in i64) plus the general case: measure before adding it.
2. **Where the arithmetic lives**: native helpers in the backend (a new `crates/backend/src/bigint.rs`, built with
   wasm-encoder: no WAT, no new deps) vs. prelude JS on `i32_*` intrinsics (like the regex engine). Operators are
   hot and called from the runtime's generic paths (`$js_add`, comparisons, ToNumeric), which suggests native
   helpers for add/sub/mul/compare/shift and prelude JS for the rare ones (toString radix, parse, division if
   long division is simpler there).
3. **Gating** (the key to "programs without BigInt compile as before"): link the BigInt runtime and change the
   operator lowering only when the program can make a BigInt: a literal, `BigInt(…)` or `BigInt` as a value, like
   `cx.names("$date")`/`"$regexp"` gate Date/RegExp support (T26/T27). Without that, today's `f64` lowering and
   runtime stay byte-for-byte the same.
4. **Operator lowering when gated on**: which expressions need the generic path (any operand of kind `Any`) and
   which stay `f64` (both operands known numbers: inference already knows literals and number-typed slots).

## Plan (slices; each ends with fixtures + a targeted test262 run)
1. **Values**: `$bigint`, literals (decimal/hex/octal/binary, from oxc's `BigIntLiteral` value/raw), `typeof`,
   `===`/`!==`/`Object.is`/SameValueZero (Map/Set keys, `includes`), truthiness (`0n` is falsy), `console.log`
   (`123n`, in arrays/objects), `String(x)`/template literals/`+ ""` (ToString), `JSON.stringify` TypeError
   ("Do not know how to serialize a BigInt").
2. **Operators**: ToNumeric-dispatching `+ - * / % **`, unary `-` and `~` (`+1n` throws), `++`/`--`, bitwise
   `& | ^ << >>` (two's-complement semantics on sign-magnitude), `>>>` TypeError, comparisons (BigInt vs number
   by mathematical value, incl. NaN/±Infinity; vs string via StringToBigInt), `==` with numbers/strings/booleans.
   Errors with V8's texts (`RangeError: Division by zero`, `RangeError: Exponent must be non-negative`,
   `TypeError: Cannot mix BigInt and other types, use explicit conversions`).
3. **Built-ins**: `BigInt(v)` (number must be an integer: `RangeError: The number 1.5 cannot be converted to a
   BigInt because it is not an integer`; strings via StringToBigInt; `new BigInt` TypeError), `BigInt.asIntN`/
   `asUintN`, `BigInt.prototype.toString(radix)`/`valueOf`/`toLocaleString` (en-US grouping? check node),
   `Number(1n)`, `parseInt` unaffected, `Object(1n)` wrappers (T30's `$wrapper`), `Symbol.toStringTag`
   `"BigInt"`, `Object.prototype.toString` → `[object BigInt]` (already handled for `typeof` `"bigint"`).
4. **Later / optional**: `BigInt64Array`/`BigUint64Array` (F116), WIT `s64`/`u64` ↔ BigInt like jco (F124),
   `Math` functions throw on BigInt, `Atomics` no.

## Read first
- `docs/design.md` D1 (values, `i31ref`/`$number`, T30 wrappers) and D5 (prelude, intrinsics; T26/T27 notes for the
  gating pattern `cx.names("$…")`).
- `crates/backend/src/runtime.rs`: `strict_eq`, `$js_add`, ToNumber/ToString/ToPrimitive helpers, `typeof_string`,
  the type list (`T_*`).
- `crates/lower/src/lib.rs`: the crate docs on type inference (`Kind`, `Facts`), `Expression::BigIntLiteral`,
  binary/unary operator lowering, `NODE_GLOBALS`.
- `crates/backend/src/inspect.rs` (printing), `crates/backend/src/mapset.rs` (key hashing/equality),
  `crates/backend/src/numstr.rs` (number → string, for the style of native helpers).
- `tasks/T26-date.md` and `tasks/T27-regex.md` Notes / Result (how a new built-in value kind was wired end to end).
- F116, F124, F153 in `tasks/README.md`.

## Tests
Fixtures `tests/fixtures/builtins/bigint-*.js` (node is the oracle: `scripts/diff.sh`), `cargo test`,
`wasm-tools validate` + a wasmtime run. test262: `node scripts/test262.mjs built-ins/BigInt language/expressions`
per slice; the whole subset with `--update` at the end, on `a-03` (~23 min; see the memory note / T26 Progress for
the setup: rsync the repo, `cargo build --release` there, run under `nohup`, rsync `baseline.json` back). Ask the
user before committing the baseline.

## Acceptance
- [ ] Programs without BigInt compile exactly as before (same IR/Wasm for existing fixtures).
- [ ] `bigint-*.js` fixtures match node (literals, operators, comparisons, equality, errors, built-ins, printing).
- [ ] test262: most of the 201 "unsupported: BigInt" subset tests pass; `built-ins/BigInt` added; baseline updated.

## Notes / Result

## Checklist
- [ ] design questions decided and recorded
- [ ] slice 1: values
- [ ] slice 2: operators
- [ ] slice 3: built-ins
- [ ] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-28: handoff written after T26 (the user chose BigInt next). State at handoff: `main` at `b0be98e`, pushed;
  test262 subset 24,738 of 27,420 run pass (90.2%); 201 tests "unsupported: BigInt".
