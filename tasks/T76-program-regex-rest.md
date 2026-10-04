# T76: the compiled program, the regex VM and the rest (speed)

Status: done (2026-10-04; handoff written 2026-10-03, updated after T75) | Depends on: T74, T75

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
- [x] The benchmark faster, same checksum; numbers recorded.
- [x] No fixture or test262 regressions.
- [x] Follow-ups updated; the next handoff (back to T73, the TLS client) written.

## Notes / Result
**What changed**:
- `JSON.stringify` (`builtins/json.js`): the common call (no replacer, no `space`) is `$$jsonFast`: state in
  parameters, inline `typeof` tests, array keys made only for `toJSON`, cycles found by a loop over the stack (node's
  message through `$$jsonCircular`), output written into one buffer string (`$$jsonOutBuf`/`$$jsonOutAt`; doubling;
  saved/restored in a `finally` around a nested call) with new intrinsics `str_copy` and `json_quote_into`. The
  general path uses the native `json_quote`. `JSON.parse` uses `json_scan` for string runs and whitespace.
- `RegExp.prototype[Symbol.replace]`: `regexp_plain(rx)` (`object::Helper::RegExpPlain`, against `G_REGEXP_SNAP`,
  a snapshot of `RegExp.prototype`'s `exec`/`flags`/flag-getter entries taken when it's made) lets a `$`-free string
  replacement call `$$reSearch` directly (`$$reReplacePlain`). `$$reSearch` scans for a literal first character
  without per-position calls.
- `array_join`: string elements without calls. `typeof v === "object"`/`"bigint"` inline (F324 part).

**Results** (interleaved, min of 5, checksum 485686016 on every run): exposes ×200 1,950 → **1,285 ms (−34%)**, find
533 → 466 ms (−13%) (the T75 commit vs this task). Micro (`tasks/T76-files/micro76.sh`, ms, before → after; node):
stringify 255 → 82 (12–20), indent 60 → 40 (3–4), stringify + parse 354 → 158, replace 202 → 98 (12), parse only
447 → 389 (88). After T76 the benchmark profile is GC + wasmtime 33%, property access 21%, conversions 15%
(of which much is the benchmark's own checksum loop: ~20% of the run, F326), JSON 15.6% inclusive.

**Tests**: JSON matrices (escapes of all pairs of 25 boundary units, wrappers, `toJSON` keys and nesting, throwing
`toJSON`, cycles, BigInt, getter order, big outputs; parse errors and values), regex matrices (replace forms ×
13 regexes × 7 strings, `lastIndex`, sticky, patched instances and prototype; search/match/split/test with
surrogates, `u`, `i`, `y`, lookbehind) and `join`/`typeof` all equal node. `cargo test`; `diff.sh` on every fixture
directory: all pass (`node/fetch.js` failed under load and passes alone, F320). test262 full subset on a-03:
28,075 of 28,984 run pass (96.9%), 0 regressions.

**Not done**: code size (F328), the general JSON path and per-key quoting (F327), the benchmark's checksum loop
(F326), node's `global`-based replace/flags behaviour (F325). Next: T73 (TLS client), as planned.

## Progress
- 2026-10-03: handoff written with T74's profile.
- 2026-10-03: started; tools unchanged. Benchmark base for T76 = the T75 commit (building in the scratchpad).
  Old (s5) profile split: `JSON.stringify` is 31.7% of the run inclusive (`$$jsonQuote` 5.0 self, `get_named` from
  `$$jsonWrite`/`$$jsonValue` 2.5, `array_join` 1.5, `typeof_string` 1.1); `memmove` 5.3% is the GC's copying.
  `tasks/T76-files/micro76.js` + `micro76.sh` (stringify of discovery-like payloads, with indent, parse). Baseline
  (min of 5, ms, node / js2wasi3): stringify 20/378, indent 4/86, parse 41/473.
- 2026-10-03: JSON.stringify fast path for the common call (no replacer function or list, no gap): `$$jsonFast`
  with the state in parameters (no `state.x` reads per value), inline `typeof` tests, array keys made only for
  `toJSON`, cycles found by a loop over the stack (same message via `$$jsonCircular`). `typeof v === "object"` and
  `"bigint"` are inline too now (F324 part; `bigint` is `false` without a `$bigint` type). A JSON matrix (escapes,
  wrappers, toJSON with keys, holes/functions/symbols, cycles in objects/arrays/deep, BigInt with and without
  toJSON, getter order) equals node, with and without indentation.
- 2026-10-03: micro (each compiler measured first, min of 7): stringify base 255 → fast path 159 ms, parse (incl. a
  stringify) 354 → 236, indent 60 → 51; node 12/26/3 in that batch (this laptop's speed drifts ~2× between
  batches: compare within one).
- 2026-10-03: `$$jsonQuote` native: `__intrinsic_json_quote` (IR `Intrinsic::JsonQuote`, backend helper
  `runtime::json_quote`: one pass for the length, `array.copy` when nothing is escaped, else a writing pass).
  Escapes of all pairs of 25 boundary units (controls, `"`, `\`, surrogates paired and lone) as values and keys equal
  node. micro: stringify ~159 → 120 ms, parse 236 → 198, indent 51 → 43.
- 2026-10-03: benchmark, T76 base (T75 commit) vs JSON fast path + native quote (j2), interleaved, min of 5, all 10
  runs checksum 485686016: exposes ×200 2,046 → 1,574 ms (−23%), find 495 / 507 (unchanged). 37.05 MB.
- 2026-10-03: `array_join`: a string element is its own part (no accessor check, `is_nullish`, `to_string` calls).
- 2026-10-03: the j2 profile: `JSON.stringify` 22% inclusive, GC 33.7% (allocation: a string per piece and per
  quote, the pieces array, the join copy). Then the fast path writes into one buffer: prelude globals
  `$$jsonOutBuf`/`$$jsonOutAt` (saved and restored around a nested call from `toJSON`, in a `finally`), `$$jsonPut`,
  `$$jsonPutUnit`, `$$jsonPutQuoted` with new intrinsics `str_copy` (`array.copy` of a string into a buffer) and
  `json_quote_into` (escapes written in place; runs without escapes copied with `array.copy`); the result is one
  `sub` copy. Nested/throwing `toJSON`, 5,000-object outputs, surrogates, the earlier JSON matrices equal node.
  (A first version inverted the `"`/`\` escape select: caught by the escape matrix.) micro (each first, min of 7):
  stringify 116 → 82 ms, parse 200 → 156, indent 43 → 41.
- 2026-10-03: benchmark base / j2 / j3 (builder + join fast path), interleaved, min of 5, 15 runs checksum
  485686016: exposes ×200 1,946 / 1,546 / 1,392 ms (−28%), find 511 / 501 / 507. Noisy batch (single runs up to
  +50%).
- 2026-10-03: j3 profile: `getLabelFromName` (zigbee-herdsman-converters' `name.replace(/_/g, " ")` in every
  expose constructor) 9.1% inclusive: `String.prototype.replace` 7.3 (the `flags` getter reading 8 flag getters
  2.9, `$$reExecAny`/`$$reExec` result arrays 2.6). Then a sound fast path, no write hooks:
  `__intrinsic_regexp_plain(rx)` (`object::Helper::RegExpPlain`) is true for a RegExp whose prototype is
  `RegExp.prototype`, whose own entries are just `lastIndex` (attributes as made, a number) and its private
  state, and when `RegExp.prototype`'s `exec`, `flags` and 8 flag getters are as built: entry index, key
  identity, attributes and value (an accessor's getter) compared with `G_REGEXP_SNAP`, a snapshot taken when the
  prototype is made (`Helper::RegExpSnapshot`; accessors are updated in place, hence the getter). Then
  `$$reReplacePlain` (string replacement without `$`): `$$reSearch` directly, the match start from `code[8]`,
  the same `lastIndex` writes. Replace matrix (13 regexes × 7 strings × 6 forms, `lastIndex` before/after,
  sticky, a `valueOf` lastIndex, own `exec`, read-only lastIndex, frozen, subclass, patched `global` getter and
  `exec`, restored) equals node. Found two existing differences from node, not on the fast path (F325).
  micro replace 202 → 107 ms.
- 2026-10-03: `$$reSearch`: a literal first character (no `i`, not a surrogate) is found with a plain unit scan
  (no `$$reRead`/`$$reOne` calls per position). A search/match/split/test/replace matrix (surrogates, `u`, `i`, `y`,
  lookbehind, U+2028, U+FFFF) equals node. micro replace 107 → 98 ms (node 12).
- 2026-10-03: benchmark base / j3 / r2 (regex fast paths), interleaved, min of 5, 15 runs checksum 485686016:
  exposes ×200 1,950 / 1,377 / 1,285 ms (−34% for T76 so far), find 533 / 507 / 466 ms (−13%). F325 recorded.
- 2026-10-03: r2 profile: 19.6% of the run is the benchmark's own checksum loop (`sum = (sum * 31 +
  text.charCodeAt(k)) | 0` in `async main`: `sum` is a `jsval`, so `numeric`/boxing/`charCodeAt` through
  `call_value`); a measurement artifact, not tuned for. The "compiled program" bucket has no item over 1.2% besides
  `json_quote_into` (3.4%, native already).
- 2026-10-03: JSON.parse (in scope; zigbee2mqtt parses every MQTT message): `$$jsonParseString` 38% and
  `$$jsonSkipSpace` 14% of parse time. A native scanner `__intrinsic_json_scan(src, i, mode)` (mode 0: past JSON
  whitespace; mode 1: to the next `"`, `\` or control unit) replaces both JS loops. A parse matrix (escapes,
  `\u` errors, control characters, unterminated, trailing commas, empty, `__proto__`, repeated keys, long
  strings, 200-element arrays, -0, 1e400, NBSP) equals node, messages included. Parse-only micro (interleaved,
  min of 5): 447 → 389 ms (node 88); the rest is object building (`define`, `grow`, `reindex`: F321).
- 2026-10-04: `cargo test` passes; `diff.sh` on all fixture directories passes (`node/fetch.js` only alone,
  F320). test262 full subset on a-03 (1,200 s): 28,075 of 28,984 run pass (96.9%), 0 regressions, the same 2
  changes as after T75. design.md (D1 typeof note, D5 JSON/RegExp notes), HISTORY.md, feasibility speed note,
  follow-ups F325–F328, Notes / Result; done. Next: T73.
- 2026-10-04: committed and pushed (user's request).
