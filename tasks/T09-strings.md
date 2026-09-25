# T09: Strings and the string boundary

Status: done | Depends on: T08

## Goal
JS strings as `(array i16)`: literals (from data or `array.new_fixed`), `.length`, concatenation with `+`,
`===`, number→string (shortest round-trip, like `String(1.5)`), and WIT `string` params/results.

## In scope
- The Canonical ABI string lift/lower through linear memory: `cabi_realloc`, UTF-8 ↔ UTF-16 conversion, and lone surrogates → U+FFFD (document this).
- The number→string algorithm must match JS exactly. Test it against node for a fixed list of 200 values.

## Out of scope
String methods (later, self-hosted in T21), ropes.

## Acceptance
- `export function greet(name) { return "hi " + name; }` → `wasmtime run --invoke 'greet("x")'` prints `"hi x"`.

## Progress
- 2026-09-25: started (T08 committed as 8bfb4e7).
- 2026-09-25: **decision (user): WIT param types from JSDoc + usage.** `/** @param {string} name */` (or `{number}`) wins;
  otherwise an exported param used as a string (a `+` or `==`/`===` operand next to a known string, a template-literal
  interpolation, `.length` read) is `string`; anything else stays `f64`. Result: `string` if every return value is a string
  (or `@returns {string}`), else `f64` as before.
- 2026-09-25: plan.
  - **IR**: `Expr::Str(Vec<u16>)` (jsval literal); `Expr::Intrinsic { op, args }` for runtime operations with a fixed signature
    (`JsAdd`, `Concat`, `ToString`, `NumberToString`, `Length`, `TypeofString`); `ir::Export.abi` = boundary types (`F64` / `String`).
  - **Backend runtime**: `$string (array (mut i16))` joins the rec group. Literals are immutable globals built with `array.new_fixed`
    (no data segment needed, which settles F3). Helpers: `str_eq`, `concat`, `to_string`, `js_add`, `length`, `typeof_string`, plus string
    cases in `to_boolean`/`typeof`/`strict_eq`/`loose_eq`. `to_number` of a string traps for now (string → number parsing is a follow-up);
    the lowering reports it at compile time when the operand is known to be a string.
  - **Number → string**: Ryu (d2s, shortest round trip, round-half-even) hand-written in Wasm (`numstr.rs`), tables (5^i splits) computed
    in Rust with a small bignum and loaded from a passive data segment with `array.new_data` (so no linear memory needed), then JS
    Number::toString formatting (plain / decimal point / `0.000ddd` / exponent).
  - **ABI** (`backend/src/abi.rs`): memory + bump `cabi_realloc` only when an export takes or returns a string. An adapter per such export:
    UTF-8 (ptr, len) → `$string`; reset the bump pointer after the args are decoded; string result → UTF-8 (lone surrogates → U+FFFD)
    + an 8-byte return area. Canonical ABI string encoding: UTF-8.
  - **Lowering**: value kinds `Num | Bool | Str | Any`; slot evidence becomes a lattice (absent ≈ Num, Str, Any) with the same fixpoint.
    `+`: Num+Num → f64 add; a Str operand → `Concat` with `ToString` of the other; otherwise `JsAdd`.
    `.length`, template literals, `typeof` as a value (F17).
  - **Component**: WIT `string`.
- 2026-09-25: Ryu prototyped in Python first (`d2d` + JS formatting): 0 mismatches on 250k values vs Python's shortest repr and on
  30k random doubles vs node. Then written as Wasm (`backend/src/numstr.rs`); tables from a small bignum, spot-checked against Ryu's header.
- 2026-09-25: runtime strings (`$string` in the rec group, string globals, helpers), `abi.rs` (memory, bump `cabi_realloc`, UTF-8 ↔ UTF-16,
  adapters), IR `Str`/`Intrinsic`/`Export.sig`, lowering kinds + JSDoc + usage hints, component `string`.
- 2026-09-25: bug: the first kind observed for a slot joined with the implicit "number" and became `Any` (so `greet` returned f64).
  Fixed with proper fixpoint iteration: each round records the kinds it *sees* (from bottom); next facts = old ⊔ seen; stop when unchanged.
- 2026-09-25: **acceptance**: `greet(name) { return "hi " + name; }` → WIT `greet: func(name: string) -> string`,
  `wasmtime run --invoke 'greet("x")'` prints `"hi x"`. Lower tests updated (string → number rejection, JSDoc type rejection).
- 2026-09-25: `numstr.txt` (200 values, node's `String(x)`) → backend test `number_to_string_matches_node` passes. One-off check of
  5000 more random doubles vs node: all equal (and a planted wrong expectation was caught).
- 2026-09-25: `strings.js` vs node: `joinWith`'s `n` became `string` (template interpolation, per the rule) → fixture annotated
  `@param {number} n`. **Bug**: `unicode()` returned f64: round 1 read `s` as a number (not known yet), so `s + s.length` was `Any`,
  and facts only grow. Fixed with a `Bottom` kind ("nothing known yet") and monotone kind rules, so the rounds reach the least
  fixpoint (`Num` is now recorded explicitly). All 10 `strings.js` exports match node.
- 2026-09-25: exports with > 16 flat params are rejected (the Canonical ABI would pass them in memory, F21). fmt + clippy clean, all tests ok.

## Notes / Result

**Done.** JS strings on WasmGC, `+`/template literals/`.length`/`typeof`, exact number → string, and WIT `string` at the boundary.

What changed:
- `crates/ir`: `Expr::Str`, `Expr::Intrinsic` (`JsAdd`, `Concat`, `ToString`, `NumberToString`, `Length`, `TypeofString`),
  `Export.sig: AbiSig` (boundary types `F64`/`String`, checked by the validator).
- `crates/backend`: `runtime.rs` (`$string`, `$i64s` in the rec group; string globals; `string_eq`, `concat`, `to_string`, `js_add`, `length`,
  `typeof_string`; string cases in `to_boolean`/`typeof`/`strict_eq`/`loose_eq`), `numstr.rs` (Ryu + JS formatting), `abi.rs` (memory, bump
  `cabi_realloc`, UTF-8 ↔ UTF-16, lone surrogates → U+FFFD, adapters). GC types, memory and data only appear when used.
- `crates/lower`: kind lattice `Bottom < Num | Bool | Str < Any` with a least-fixpoint iteration; string and template literals, `+` (static
  concat / f64 add / runtime `js_add`), `.length`, `typeof` as a value (F17), boundary types from JSDoc + usage, converting wrappers.
- `crates/frontend`: `Program::jsdoc(offset)`. `crates/component`: WIT `string`. CLI: signatures from `Export.sig`.
- Docs: `docs/wit-names.md` "Parameter and result types", design D1/D4 notes.
- Tests: backend `number_to_string_matches_node` (200 values), `numstr::tests::tables_match_ryu`; CLI `greet_takes_and_returns_a_string`,
  `strings_match_node` (10 exports), `values_match_node`; lower tests updated. Fixtures `numstr.txt`, `strings.js/.out`,
  `unsupported/string-to-number.js` (was `string.js`).

Commands run: `cargo test -q` (all ok), `cargo clippy -q --all-targets` (clean), `wasm-tools validate`,
`wasmtime run --invoke 'greet("x")' greet.wasm` → `"hi x"`.

Unsupported / limits:
- String → number (`"5" * 2`, `"1" == 1`) and `<` on strings: rejected when the operand is known to be a string, a trap at run time
  otherwise (F20, T21).
- More than 16 flat boundary params (F21). Only `f64`/`string` at the boundary (F13).
- Long string literals cost ~3 bytes per code unit as `array.new_fixed` globals (F19).

Next smallest step: T10 (command world + `console.log`): the string runtime and `to_string` are in place, so `console.log(x)` is
`to_string` + UTF-8 encode (`abi.rs` already has it) + a P3 stdout stream write.
