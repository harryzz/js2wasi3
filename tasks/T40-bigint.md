# T40: BigInt (F153)

Status: done | Depends on: T21 (prelude), T25 (built-ins as values), T30 (wrappers)

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

## Decisions (2026-09-28)
1. **Representation**: `$bigint`, a final `(array (mut i32))`: element 0 the sign (0, or 1 for negative), then the
   magnitude in 32-bit limbs, little-endian, normalized (no zero top limb; `0n` is `[0]`). Never mutated once made, so
   `===` compares contents (like strings). It joins the runtime `rec` group (index 28, `T_BIGINT`) **only when the
   module can make a bigint**; otherwise the type section and every index are unchanged. Literals are immutable
   globals built with `array.new_fixed` (like string constants). No small-value fast path for now.
2. **Arithmetic is native** (`crates/backend/src/bigint.rs`, wasm-encoder): magnitude add/sub/compare, schoolbook
   multiply with `i64` products, Knuth's division D, shifts, `& | ^ ~` through two's complement, → `f64` (correctly
   rounded), from an integral `f64`, → string in a radix (repeated division by one limb). The prelude
   (`builtins/bigint.js`) has `BigInt(…)`, StringToBigInt, `asIntN`/`asUintN` and the prototype methods.
3. **Gating**: `ir::Module::bigint`, set when the user's code has a BigInt literal or names the global `BigInt`. The
   backend adds the bigint cases to the runtime helpers (`typeof`, ToNumber, ToString, `===`, `==`, `+`, `<`,
   ToBoolean, printing, Map/Set keys) and the type only then.
4. **Operators when gated**: `- * / % ** & | ^ << >> >>>`, unary `-`/`~` and `++`/`--` with an operand of kind `Any`
   become ToNumeric on each operand, then either the bigint operation (`Intrinsic::BigIntBinary`, which throws the
   mix TypeError unless both are bigints) or the existing `f64` operation on the unboxed numbers; the result is `Any`.
   Relational operators with an `Any` operand use `JsLess`. Operands known to be numbers keep the `f64` path.
   V8's texts: node 24 says `RangeError: undefined must be positive` for a negative exponent (a V8 quirk; copied).

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
- [x] Programs without BigInt compile exactly as before (same IR/Wasm for existing fixtures).
- [x] `bigint-*.js` fixtures match node (literals, operators, comparisons, equality, errors, built-ins, printing).
- [x] test262: most of the 201 "unsupported: BigInt" subset tests pass; `built-ins/BigInt` added; baseline updated.

## Notes / Result
**What changed**
- IR (`crates/ir`): `Module::bigint` (the gate) and `Module::string_to_bigint`; intrinsics `BigIntConst` (a literal's
  decimal digits, `-` for a negated literal), `ToNumeric`, `Numeric(NumericOp)` (binary operators but `+`, unary
  `-`/`~`, `++`/`--`), `BigIntFromNumber`/`BigIntToNumber`/`BigIntToString` (prelude); `Builtin::BigInt`, outside
  `Builtin::ALL`.
- Backend: `crates/backend/src/bigint.rs` (see the Decisions); `$bigint` is type 28, added to the `rec` group only
  when gated (`runtime::types(…, bigint)`, `type_base` + 1); `G_BIGINT_PROTO`/`G_BIGINT_CTOR` after the runtime
  globals only when gated (`runtime::proto_global`/`ctor_global` replace `G_PROTOS + b.index()` for any `b`);
  constants are globals next to the string constants (`ConstGlobal`). Gated cases in `typeof`, ToNumber (throws),
  ToBoolean, ToString, `===`, `==`, `+`, `<`/`<=`/…, `console.log` (`1n`, `[BigInt: 1n]`), Map/Set hashing and
  SameValueZero, primitive property reads (`BigInt.prototype`), ToObject, the wrapper tag, `toStringTag`. `BigInt`
  as a value: its construct entry throws "BigInt is not a constructor" (`func::Helper::NotConstructor`).
- Lowering: literals; in a BigInt module (`may_be_bigint`: gated and the operand's kind is `Any`) arithmetic,
  bitwise, shift and unary operators, `++`/`--` (variables and properties) and compound assignments go through
  `Intrinsic::Numeric`, relational operators through `JsLess`; operands known to be numbers keep the `f64` code.
  `if (__intrinsic_bigint_mode()) { … }` in the prelude: the block in a BigInt module, nothing otherwise.
- Prelude: `builtins/bigint.js` (`$$StringToBigInt`, `$$BigInt`, `asIntN`/`asUintN`, `toString`/`valueOf`/
  `toLocaleString`); `Number(…)` and `JSON.stringify` of a BigInt wrapper in `bigint_mode` blocks; `Object.is`
  (`object.js`, SameValue: it didn't exist). The prelude scan (`builtins::Prelude::bigint`) decides the gate and
  links `BigInt.prototype`'s methods only then (every program naming `toString` linked them before the gate).
- Fixtures: `tests/fixtures/builtins/bigint-{values,operators,builtins,ctor}.js`.

**Commands run**: `scripts/diff.sh tests/fixtures/builtins/bigint-*.js` (4 pass); randomized scratch programs vs
node (20,000 divisions/remainders, 400 rows of every operator, 3,000 `Number(bigint)` roundings: identical); old
vs new compiler (`HEAD` built in a scratch worktree) on every fixture: identical core modules except 3 the old
compiler itself builds nondeterministically (F186); `cargo test -p js2wasi3-{ir,lower,backend}`; test262:
`built-ins/BigInt` and every test whose output can change, with `--update` (see HISTORY.md): subset 25,018 of
27,496 run pass (91.0%, was 90.2%), 0 regressions.

**Unsupported / left**: BigInt typed arrays (F116), WIT `s64`/`u64` ↔ BigInt (F124), `toLocaleString` with
locales (F182), speed (F188), algorithms (F189), exact size limits (F190).

**Next smallest step**: F116's `BigInt64Array`/`BigUint64Array` (the element conversions are `BigInt.asIntN(64, …)`
and `bigint_to_f64`-like limb reads), or F124 at the boundary.

## Checklist
- [x] design questions decided and recorded
- [x] slice 1: values
- [x] slice 2: operators
- [x] slice 3: built-ins
- [x] test262 + baseline, follow-ups, docs (HISTORY.md, keywords.md, design.md, README)

## Progress
- 2026-09-28: handoff written after T26 (the user chose BigInt next). State at handoff: `main` at `b0be98e`, pushed;
  test262 subset 24,738 of 27,420 run pass (90.2%); 201 tests "unsupported: BigInt".
- 2026-09-28: started. Read design.md, runtime.rs (`strict_eq`, `js_add`, `js_less`, ToNumber/ToString), the
  lowering of binary/unary operators. Decided the four design questions (see Decisions). node's error texts probed.
- 2026-09-28: slice 1+2 core in place: `$bigint` type (gated), `crates/backend/src/bigint.rs` (norm, compare,
  add/sub, mul, Knuth D, pow, shifts, two's-complement `& | ^`, f64 ↔, to string, hash, ToNumeric, `Numeric(op)`,
  loose `==`, IsLessThan), runtime cases (typeof, ToNumber throws, ToBoolean, ToString, `===`, `==`, `+`, `<`,
  inspect, Map/Set keys), literals as globals, lowering of operators/`++`/compound assignment through
  `Intrinsic::Numeric` when an operand is `Any`, `builtins/bigint.js` (`$$StringToBigInt`). A smoke program
  (literals, printing, every operator, comparisons) matches node.
- 2026-09-28: randomized differential check (20,000 divisions/remainders up to 7 limbs, 400 rows of every
  operator) identical to node. Output unchanged for programs without BigInt: all 149 buildable fixtures'
  core modules are byte-identical to `HEAD`'s, except 3 that the old compiler itself builds
  nondeterministically (generators/basics, arrays/destructuring, builtins/map-set: 2–6 variants over 25 runs;
  the new outputs are among them) → follow-up. `Object.is` didn't exist: added (SameValue, `object.js`).
  Fixture `builtins/bigint-values.js` passes.
- 2026-09-28: slice 3: `Builtin::BigInt` (outside `Builtin::ALL`, whose order is the global layout: its
  prototype/constructor are 2 gated globals after the runtime's, `runtime::proto_global`/`ctor_global`),
  `BigInt.prototype` with the prelude's `toString`/`valueOf`/`toLocaleString` (linked only when the code can make
  a BigInt: every program naming `toString` linked them before the gate), primitive property reads, ToObject /
  `[BigInt: 1n]` wrappers, `BigInt` as a value (no construct entry: `new BigInt` is node's TypeError), `BigInt(…)`,
  `asIntN`/`asUintN`, `Number(1n)` through a compile-time `if (__intrinsic_bigint_mode()) { … }` block in the
  prelude (dropped from the IR without BigInts). Fixed `bigint_cmp` and `bigint_to_f64` validation errors.
  Fixtures bigint-{values,operators,builtins,ctor}.js pass (~1 s each serially); byte-identical check still holds.
  3,000 `Number(bigint)` rounding cases (ties, sticky bits) match node.
- 2026-09-28: test262: `built-ins/BigInt` added to the subset and sparse checkout (77 files): 75 of 76 run pass
  (`prop-desc.js` needs the global object). Fixes from it: `BigInt` has a construct entry that throws (test262's
  `isConstructor`), `toString`'s `length` is 0, `BigInt.prototype.valueOf`/`toString` always linked in a BigInt
  module (ToPrimitive of a wrapper hit the sentinel trap), `JSON.stringify` unwraps BigInt wrappers (in a
  `bigint_mode` block). Of the 267 baseline-"unsupported" subset tests mentioning BigInt: 199 pass, 62 still
  unsupported for other reasons (44 `Function`, 6 `globalThis`, 6 BigInt64Array/BigUint64Array, 4 dynamic
  `import`, `JSON.rawJSON`, `Proxy`), 4 time out: `Promise.{all,allSettled,any,race}/resolve-throws-iterator-
  return-is-not-callable.js`, a pre-existing hang (a throwing `C.resolve` never closes the iterator; reproduced
  without BigInts) → follow-up. `-5n` is a negative constant. A library using BigInts internally works.
- 2026-09-28: done. Docs: design.md (D1, D5 notes), keywords.md (`BigInt` row), README, HISTORY.md, follow-ups
  F186–F190 (F153 ticked). Baseline updated locally (targeted run); not committed yet.
