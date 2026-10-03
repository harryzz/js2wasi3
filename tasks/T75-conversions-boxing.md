# T75: conversions, operators and number boxing (speed)

Status: todo (handoff written 2026-10-03) | Depends on: T74 (benchmark, profiling setup)

## Goal
The next part of zigbee2mqtt's start-up CPU that is ours, after T74's property access (the user's order,
2026-10-03: T74, this, T76). In T74's benchmark profile (after T74) the runtime's conversions, operators and
boxing are **~28%** of the CPU, the biggest bucket: `strict_eq` 5.1% (+ `is_number` 1.8% inside it), `box_number`
4.1%, `to_string` 3.9% (half of it `to_property_key` turning number keys into strings), `to_number` 2.7%,
`to_boolean` 1.8%, `js_add`, `js_less`, `typeof`, `numeric`, `concat`. Same results everywhere.

## What we know (at handoff)
- Benchmark: `tasks/T74-files/bench.sh` (zigbee-herdsman-converters' definitions and exposes for 52 device types;
  node's checksum), `micro.js`/`micro.sh` (min of N runs, wasmtime with the 256 MiB heap). After T74 (2026-10-03,
  i7-8565U, wasmtime 49.0.2): exposes ×200 2,529–2,732 ms (node 62–73), find 595–700 ms (node 334–363).
- Profile script: `perf record -e cpu-clock -F 999 -g … wasmtime run --profile=perfmap …`, then `perf script -F
  ip,sym` and `tasks/T74-files/perf-cats.py ps.txt [function…]` (categories, top items, callers).
- Callers (T74 profile): `strict_eq` from the prelude (`$$thisString`'s `v === undefined || v === null`, JSON's
  `$$jsonValue`/`$$jsonWrite`/`$$jsonWritable`), so comparisons with a constant oddball (`undefined`, `null`, `true`,
  `false`) or a string constant can be `ref.eq` / an inline compare instead of a call; `box_number` from `numeric`,
  `js_add`, `$$append` (`+a.length`), `$$toInteger`; `to_string` from `to_property_key` (number keys of non-arrays)
  and `array_join`.
- Type inference (D1, `crates/lower`, "Type inference"): variables are `f64` unless a non-number is stored; boxing
  happens where an `f64` meets a `jsval` (calls through values, properties, arrays). Every property read gives a
  `jsval`, so `o.x + 1` is `js_add` on `jsval`s.

## Plan
1. Profile the benchmark and micro with call counts per operator site kind (constant vs value operands).
2. `===`/`!==` with a constant operand: inline (`ref.eq` for oddballs and symbols; string constants: identity,
   then length + `string_eq`; numbers: `f64.eq` when the other side is known to be a number).
3. Fewer boxes: an i31 fast path where results are small integers (`box_number` already makes i31s for them?
   check), `js_add`/`js_less` fast paths for two i31s inline at the site.
4. `to_property_key` of an i31 key on a plain object: a small cache of number → string (0…1023), or avoid
   converting where the object is an array.
5. Tests: `cargo test`, `diff.sh` on touched fixtures, test262 `language/expressions` subset at the end.

## Acceptance
- [ ] The benchmark faster, same checksum; numbers recorded.
- [ ] No fixture or test262 regressions.
- [ ] Follow-ups and T76 handoff updated.

## Notes / Result
(to be written)

## Progress
- 2026-10-03: handoff written with T74's profile.
