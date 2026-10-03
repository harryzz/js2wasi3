# T76: the compiled program, the regex VM and the rest (speed)

Status: todo (handoff written 2026-10-03, updated after T75) | Depends on: T74, T75

## Goal
The remainder of zigbee2mqtt's start-up CPU that is ours, after T74 (property access) and T75 (conversions and
boxing): the compiled program itself (~18% in T74's benchmark profile), JSON (~5%: `$$jsonQuote`, `$$jsonWrite`,
`$$jsonObject`, `array_join`), the other prelude code (~3%), the regex VM (~1% in the benchmark, 4% in T69's
zigbee2mqtt profile: `$$reRun`), and object building that T74 left (`grow`, `reindex`, `alloc_object`).
Same results everywhere.

## What we know (at handoff)
- Benchmark and profiling: `tasks/T74-files/` (see T74 and T75). The production trial's zigbee2mqtt profile
  (2026-10-03): regex VM `$$reRun` 1.8%, `to_upper_case` via `$$caseMap`.
- Ideas, unmeasured: `JSON.stringify` builds pieces in an array joined at the end (`builtins/json.js`): write into
  a growable `$i32s`/string buffer instead; `$$jsonQuote` scans per character in JS: a native "needs escaping?"
  scan first; object literals with constant keys could allocate their arrays at their final size (no `grow`);
  regex: literal prefixes / first-character sets before the VM, a compiled program cache by source (exists, T27).
- After T75 (2026-10-03; profile of the benchmark build, exposes ×200 ~1.94 s): GC + wasmtime type checks 30.1%,
  property access + object building 23.4% (`get_named` 5.1: callers `$$jsonWrite` 1.6, `$$jsonValue` 0.9),
  compiled program + other 19.6%, conversions/operators/boxing 15.1% (`box_number` 4.4, F323; `to_string` 3.1:
  `$$thisString` before T75's fix, `js_add`, `array_join`; `typeof` 1.3 via `typeof_string` from JSON, F324;
  `concat` 1.3: `js_add` and `$$jsonQuote`), JSON 7.5%, other prelude 3.1%, regex 1.1%. Compare builds interleaved
  (T75's Progress: the second of two back-to-back measurements runs ~20% slower on this laptop).
- Code size (memory, the production trial): 62 MB of wasm for zigbee2mqtt became 334 MB of machine code, 210 MB
  resident: worth a look here too (helpers inlined per site? the lowering's per-function prologues?).

## Plan
1. Profile the benchmark after T75; split the "compiled program" bucket by function (zigbee-herdsman-converters'
   own code vs generated helpers like `$code` trampolines and argument arrays).
2. Pick the largest items, measure each change.
3. Tests: `cargo test`, `diff.sh`, test262 subset at the end.

## Acceptance
- [ ] The benchmark faster, same checksum; numbers recorded.
- [ ] No fixture or test262 regressions.
- [ ] Follow-ups updated; the next handoff (back to T73, the TLS client) written.

## Notes / Result
(to be written)

## Progress
- 2026-10-03: handoff written with T74's profile.
