# T125: ToNumber of a string inside the runtime never traps (F85)

Status: done | Depends on: T21, T120 | Local (+ a-03 only after asking)

## Goal
F85 (found in T21): the runtime's ToNumber (`runtime::to_number`) calls the prelude's StringToNumber for a string,
which the lowering links only when the IR itself converts a JS value to a number (`Convert JsVal→F64`, loose `==`,
`JsLess`; T120 added modules with typed arrays). Without it the string branch is `unreachable`: a trap, e.g.
`setTimeout(f, "10")`. User chose F85 on 2026-10-16.

## Plan
- [x] Probe the F85 cases; find every runtime caller of `to_number`; measure what linking StringToNumber costs.
- [x] Decide with the user: link it always, or only for the constructs whose runtime can convert a string.
- [x] Fixture; `scripts/diff.sh`; full test262 on a-03 (ask).

## Progress
- 2026-10-16: task created (user: "ok lets F85"). Probe (`scratchpad/f85`): `a.length = "2"`,
  `f.apply(null, {length: "2"})`, `slice("1")`/`charAt("1")`/`at("-1")`/`repeat("3")` already = node (those
  modules link StringToNumber for other reasons); `setTimeout(f, "10")` traps in `to_number` (called by
  `setTimeout`; the module has no StringToNumber). The runtime's `to_number` has ~28 callers in 12 backend files
  (timers, apply's array-likes, array `length`, defineProperty, typed arrays, BigInt mixing, the boundary, …), and
  even hello-world contains `to_number` (`to_string`, `array_set_length`). All 18 benchmark programs already link
  StringToNumber. Its cost in a small program: hello-world 69,821 → 81,462 bytes (+11.6 KB, +17%).
- 2026-10-16: user chose "always link" (option 1), and to note the paths for a later targeted version (F437).
  `crates/lower/src/lib.rs`: the facts start with `string_to_number: true` (StringToNumber in the first round, no
  extra round); `may_convert_string` kept for F437. The F85 cases run: `setTimeout(f, "10")` → `tick`; small
  modules +12.2 KB (c.js 71,888 → 84,098; hello-world 69,094 → 81,259). IR snapshots `add.ir`/`fib.ir`/`loops.ir`
  regenerated (`UPDATE_SNAPSHOTS=1`): only the 22 StringToNumber functions appended (`$$parseDecimal`,
  `$$decimalToDouble`, `$$big*`, …), no line of the programs' IR changed. `cargo test --release --no-fail-fast`:
  all pass. Fixture `builtins/runtime-to-number.js` = node. Fixtures `builtins` 76 (1 skip), `objects` 19, `arrays` 9,
  `classes` 12, `closures` 11, `syntax` 10, `async` 15, `promises` 14, `exceptions` 14, `generators` 4, `globals` 5,
  `modules` 14, `commonjs` 15, `node` 42, `semantics`/`values`/`strings` pass. Laptop gate vs `5c8ec94`
  (`out/gate-t125-laptop.log`): all 18 programs and the 4 micro modules byte-identical (they linked it already).
- 2026-10-16: the runtime's ToNumber callers (for F437; `file:line function`): `iter.rs:435 step`;
  `runtime.rs:2099 loose_eq`, `:2323 to_string`, `:3113 js_add`, `:3513 js_less`; `func.rs:1101
  list_from_array_like` (`apply`/`Reflect` array-likes), `:1152 bind`; `abi.rs:387`, `:1035 writable_end`, `:2067
  adapter` (the boundary); `object.rs:1678 get_num`, `:3727 typed_put`, `:3818 set` (typed path), `:5565
  object_to_number`; `array.rs:1150 set_length`; `mapset.rs:380 same`; `bigint.rs:1788 loose_eq`, `:1829 cmp_any`,
  `:1872 to_numeric`, `:1904 numeric`; `timers.rs:299 set`, `:486 clear`; `lib.rs:2359/2406/2434` (the emitter's
  numeric reads); `props.rs:835 define_property`; `wit.rs:580/773 lower_mem`.
- 2026-10-16: user: yes. Full test262 on a-03 started (`~/t262-t125.log`).
- 2026-10-16: test262 (a-03, `out/t262-t125.log`, 1,467 s): no changes since the baseline (28,105 pass). Task done.

## Notes / Result
- **What changed:** the lowering links the prelude's StringToNumber in every module (`Facts { string_to_number:
  true }` from the first round), so the runtime's ToNumber of a string never traps (`setTimeout(f, "10")` did).
  Cost: +11.6–12.2 KB in a module that never converted a string; every benchmark module is byte-identical (they
  linked it already). `Lowerer::may_convert_string` stays as the start of F437 (link only when needed: the IR's
  conversions plus the ~28 runtime paths listed in Progress).
- **Result:** fixture `builtins/runtime-to-number.js` = node; `cargo test` (IR snapshots regenerated: only the
  StringToNumber functions appended); fixture directories pass; laptop gate: all identical; full test262 no changes.
- **Next smallest step:** choose: F106/F108 (`extends Error`/`Array`), F407 (raytrace), F436 when wasmtime 50.0.0 is
  final.

