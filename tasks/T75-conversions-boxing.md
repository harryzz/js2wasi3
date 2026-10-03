# T75: conversions, operators and number boxing (speed)

Status: done (2026-10-03; handoff written after T74) | Depends on: T74 (benchmark, profiling setup)

## Goal
Cut the next part of zigbee2mqtt's CPU that is ours. The user's order (2026-10-03, after the production trial:
`docs/zigbee2mqtt-feasibility.md`, "Production trial"): T74 property access (done), **this**, then T76 (the compiled
program, JSON, the regex VM, the rest). wasmtime's GC (~26% of the benchmark) is out of scope: other WasmGC + WASI
P3 runtimes exist, wasmtime is the reference. Same results everywhere (node is the oracle); a program's output
must not change.

## What we know (at handoff)
- **Where the time is** (T74's benchmark profile after T74, `tasks/T74-files/perf-cats.py`): conversions,
  operators and boxing **~28%**: `strict_eq` 5.8%, `box_number` 4.6%, `is_number` 3.4% (1.8% inside `strict_eq`),
  `to_string` 2.9%, `to_number` 2.7%, `to_boolean` 1.8%, `js_add` 1.7%, `js_less` 1.2%, `numeric` 1.1%, `concat` 1.1%,
  `typeof` 0.6%, `to_numeric` 0.5%, `is_nullish` 0.4%. The rest: GC + wasmtime type checks ~26%, property access +
  object building ~20% (F321), the compiled program ~18%, JSON ~5%, other prelude ~3%, regex ~1%.
- **Callers**: `strict_eq` from the prelude: `$$thisString` (`v === undefined || v === null`), JSON's
  `$$jsonValue`/`$$jsonObject`/`$$jsonWrite`/`$$jsonWritable`/`$$jsonUnwrap`, `$$toInteger`, `$$reFlag`;
  `box_number` from `numeric`, `js_add`, `$$append` (`+a.length`), `$$toInteger`, `charCodeAt`; `to_string` from
  `to_property_key` (number keys of non-array objects: `o[i]`) and `array_join`; `is_number` mostly inside `strict_eq`.
- **How `===` reaches the runtime**: the lowering (`crates/lower/src/lib.rs`, `B::StrictEquality` → `BinOp::Eq`,
  `equality(...)`) gives `jsval.eq(a, b)` in the IR (`js2wasi3 ir file.js` prints it: `v === undefined` is
  `jsval.eq(v.0, undefined)`, `s === "x"` is `jsval.eq(s.1, "x")`); the backend (`crates/backend/src/lib.rs` ~line
  1587) calls `runtime::Helper::StrictEq` for two `jsval`s. A constant operand is visible there (an oddball constant,
  `Expr::Str`), as T74 used `Expr::Str` keys for `GetNamed`: oddballs and symbols compare by identity (`ref.eq`);
  a string constant: identity, else a string test, length, then `string_eq`; numbers keep `strict_eq`.
- **Numbers**: integers in [-2^30, 2^30) except -0 are `i31ref`, others a boxed `$number` (`runtime.rs` `box_number`:
  trunc_sat, range and -0 checks, else `struct.new`). Variables are `f64` unless a non-number is stored (D1, "Type
  inference" in `crates/lower`); every property read is a `jsval`, so `o.x + 1` is `js_add` on `jsval`s and results
  stored back into objects/arrays are boxed. Ideas: an i31 + i31 fast path inline at `js_add`/`js_less` sites (no
  call, no unbox/box when both are i31 and the sum stays in range); `+a.length` in `$$append` already gives an i31
  from `GetLength` (T74): check its `to_number`/box round trip.
- **`to_property_key` of numbers**: `o[i]` on a non-array object converts `i` to a string every time (`to_string`);
  a small cache of number → key string (0…1023), or reading arrays/typed arrays before the conversion (T74's `get`
  already has an i31 array fast path).

## How to work (from T74)
- **Benchmark** (zigbee-herdsman-converters' definitions and exposes for the production trial's 52 device types,
  `tasks/T74-files/bench.cjs` + `devices.json` (no addresses) + `bench.sh`): it needs T62's zigbee2mqtt tree with its
  patches: `~/js2wasi3-prod-run/zp/patched` (outside the repo, kept; to recreate: `tasks/T62-files/setup.sh` into a
  directory `$S` with `t62/` and `t66/` copies of the task directories, then `T62_LEVEL=2 T63_GLOBALS=0
  $S/t66/patch.sh`). Commands:
  `Z2M=~/js2wasi3-prod-run/zp/patched JS2WASI3=$PWD/target/release/js2wasi3 tasks/T74-files/bench.sh build` (~2 min
  build + ~1.5 min `wasmtime compile`), then `Z2M=… RUNS=5 tasks/T74-files/bench.sh run`; `bench.sh node` for node.
  Every run prints the checksum (node's: `485686016`, 52 of 52 definitions) and per-phase times on stderr.
- **Baseline after T74** (i7-8565U, wasmtime 49.0.2, `-O gc-heap-initial-size=268435456`): exposes ×200
  **2,529–2,732 ms**, find 595–700 ms, load ~50 ms; node: exposes 62–73 ms, find 334–363 ms.
- **Micro**: `tasks/T74-files/micro.sh <compiler>…` (node, then each compiler; `RUNS=9` for the minimum of 9; it
  checks each build's output against node's). After T74: builders 76 ms, literals 59, prototype methods 49, big
  objects 29, stringify 111. Add T75 cases (equality against constants, `+`/`<` on small integers, `o[i]` on plain
  objects, `JSON.stringify` of numbers) to `micro.js` or a new `micro75.js`.
- **Noise**: ~30% run to run on this laptop. Compare minimums of several runs, keep the machine idle (no `diff.sh`
  or test262 running at the same time), keep a copy of the previous compiler to compare against
  (`cp target/release/js2wasi3 <scratchpad>/js2wasi3-base` before changing code).
- **Profile**: `perf record -q -e cpu-clock -F 999 -g -o p.data wasmtime run -O gc-heap-initial-size=268435456 -Sp3
  -S http -S inherit-network --profile=perfmap --allow-precompiled --dir .::/ <bench.cwasm>` (from the `zp/patched`
  directory), then `perf script -i p.data -F ip,sym > ps.txt; tasks/T74-files/perf-cats.py ps.txt strict_eq
  box_number` (categories, top items, immediate callers).
- **Tests**: `cargo test`; `scripts/diff.sh` on the fixtures a change touches (the whole suite takes ~1 h here and
  3 `http` fixtures fail under its parallel load, F320: rerun failures alone); `wasm-tools validate` + one wasmtime
  run. **test262 runs on `a-03`, and ask the user before any long run** (rsync the tree, `cargo build --release`
  there, `nohup node scripts/test262.mjs <dirs> > ~/t262-<label>.log`, poll `pgrep -u harry -x MainThread`; T74's
  11,050 property tests took 306 s). For T75: `language/expressions` (equality, addition, relational, typeof, …)
  and `built-ins/JSON`, `built-ins/Number`, `built-ins/String`.

## Plan
1. Micro cases for each operator kind; profile the benchmark with callers.
2. `===`/`!==` with a constant operand inline in the backend (oddballs, strings), measured.
3. Small-integer fast paths for `+`, `-`, `<`, `<=` on `jsval`s (i31 × i31), and fewer box/unbox round trips.
4. `to_property_key` of small integers without a new string each time.
5. Each change: micro + benchmark numbers in the Progress log; then `cargo test`, `diff.sh` on touched fixtures,
   test262 on `a-03`, docs (design.md D1/D5 note, `tests/test262/HISTORY.md`), follow-ups, T76 handoff update.

## Acceptance
- [x] The benchmark faster, same checksum; numbers recorded.
- [x] No fixture or test262 regressions.
- [x] Follow-ups and the T76 handoff updated.

## Notes / Result
**What changed** (backend only, plus two prelude functions):
- `===`/`!==` with a constant operand inline (`runtime::strict_eq_const`; `lib.rs` `eq_const`): `undefined`/`null`/
  `true`/`false` by `ref.eq`, a string constant by identity then string test + length + `string_eq`, a number as
  i31 or `$number`. A `jsval` scratch local per function (`scratch_js`).
- `strict_eq`: `ref.eq` first (false only for a NaN `$number`), numbers compared inline, other different refs unequal
  without further tests (BigInts by value).
- i31 fast paths: `js_add` (i32 add, i31 or a new `$number`), numbers before the object/string tests, two strings
  concatenated without `to_string`; `js_less` (i32 compare); `numeric` (programs with BigInts) for `-`, `&`, `|`, `^`,
  `<<`, `>>`, `++`, `--`, and numbers skip ToNumeric and the BigInt tests.
- `f64->jsval(jsval->f64(v))` keeps a number as it is (`runtime::as_number_value`; `+a.length` in `$$append`).
- `typeof v === "number"|"string"|"symbol"|"function"|"undefined"|"boolean"` inline (`runtime::typeof_is`);
  `typeof` tests numbers without a call; `to_boolean` tests oddballs, i31s, numbers, strings, then BigInts.
- `number_to_string`: integers below 2^53 written directly (`decimal_length` + `write_digits`); `to_property_key` of
  an i31 in [0, 1024): a key made once (`G_NUM_KEYS`, a new runtime global: every program's bytes change).
- Prelude: `$$toInteger` uses a new `f64` local; `$$thisString` returns a string first.

**Results** (i7-8565U, wasmtime 49.0.2, interleaved runs, min of 5, checksum 485686016 every run): exposes ×200
2,438 → 1,941 ms (**−20%**), find 549 → 515 ms; module 36.7 → 37.0 MB. In the profile, T75's bucket went from ~28%
(T74's handoff) to 15.1%: `strict_eq` 5.8 → 0.5%, `to_boolean` 2.2 → 0.7%, `is_number` 3.4 → 0.2%; `box_number` 4.4%
remains (non-integer results, F323). Micro (`tasks/T75-files/micro75.sh`, min of 9, ms, base → after): eqconst 74 →
52, intops 145 → 132, numstr 182 → 100, objindex 281 → 69, stringify 345 → 256, truthy 56 → 50; node: 2–8 ms. The
micro's intops case is 69% property reads (F321). Caution: with two compilers, `micro75.sh` measures the second
~20% slower whichever it is; compare in both orders.

**Tests**: equality, operator, numeric (with BigInts), truthiness, typeof, `+` and number-to-string matrices equal
node (scratchpad programs); `cargo test`; `diff.sh` on every fixture directory: all pass (`async/timers-more`,
`node/child-process-shell`, `node/fetch` failed under parallel load and pass alone: F250/F320); test262 full subset
on a-03: 28,075 of 28,984 run pass (96.9%), 0 regressions, 2 more passes (`tests/test262/HISTORY.md`).

**Not done / next**: boxing of non-integer numbers (F323), `typeof` as a value and `typeof … === "object"` (F324),
property access (F321). The next smallest step is T76: the compiled program and JSON (`$$jsonQuote`'s `js_add`/
`concat`, `array_join`'s `to_string`).

## Progress
- 2026-10-03: handoff written after T74 (`main` at `e12241e`).
- 2026-10-03: started; tools unchanged. `tasks/T75-files/micro75.js` + `micro75.sh` (minimum of RUNS; output checked
  against node). Baseline (min of 5, ms, node / js2wasi3): eqconst 5/73, intops 5/144, numstr 7/183, objindex 2/282,
  stringify 8/329, truthy 4/55. Base compiler copied to the scratchpad.
- 2026-10-03: step 2: `===`/`!==` with a constant operand (`undefined`/`null`/`true`/`false`, a string, a number)
  inline in the backend (`runtime::strict_eq_const`: oddballs `ref.eq`; strings identity, then string test +
  length + `string_eq`; numbers i31/`$number` compare); a `jsval` scratch local per function. micro (min of 5):
  eqconst 74 → 51 ms, stringify 345 → 292, others unchanged.
- 2026-10-03: `strict_eq` rewritten: `ref.eq` first (NaN check), numbers inline (no `is_number`/`to_number` calls),
  different refs of other kinds are unequal without more tests; checked on a 21×21 value matrix (+ BigInts) vs node.
- 2026-10-03: step 3: i31 × i31 fast paths at the top of `js_add` (i32 add, i31 or boxed) and `js_less` (i32
  compare), number × number in `js_add` before the object/string tests; operator matrix equal to node. intops 144 →
  132 ms only: the micro's profile is 69% property reads (`get_named`/`find_own_named` missing the own-property IC
  across 1,000 objects: F321's territory), operators 5.5%.
- 2026-10-03: step 4 (first part): the profile of `o[i]` showed `ryu_d2d` at 21% (not in perf-cats' T75 category):
  `number_to_string` now writes integers below 2^53 directly (`decimal_length` + `write_digits`). numstr 182 → 97 ms,
  objindex 280 → 141, stringify 297 → 264; number-to-string forms (`String`, `+""`, template, `join`, keys) equal node.
- 2026-10-03: step 4: `to_property_key` of an i31 in [0, 1024) returns a cached key (`G_NUM_KEYS`, a `$keys` array made
  on first use, entries filled on first use). micro (min of 9): eqconst 52, intops 133, numstr 97, objindex 68
  (base 281), stringify 291, truthy 54.
- 2026-10-03: benchmark after steps 2–4 (interleaved runs, min of 5, same checksum 485686016): exposes ×200 base
  2,438 → 2,036 ms (−16%), find 575 → 499 ms, load ~50. Build 128 s, `wasmtime compile` 89 s, 37.0 MB (base 36.7 MB).
- 2026-10-03: profile after steps 2–4: T75 bucket 19% of the run (box_number 5.0: `numeric` 1.6, `js_add` 0.8,
  compiled code 0.8, `$$toInteger` 0.4, `charCodeAt` 0.4; to_string 2.5; to_number 2.2; to_boolean 2.2; typeof 1.3;
  strict_eq 0.4, was 5.8). Then: `numeric` (the BigInt-aware operators of programs with BigInts) skips ToNumeric
  and the BigInt tests for numbers, with i31 fast paths for `-`, `&`, `|`, `^`, `<<`, `>>`, `++`, `--`;
  `$$toInteger` uses a new `f64` local (no box + `strict_eq` + unbox); `f64->jsval(jsval->f64(v))` (`+a.length`
  in `$$append`) keeps a number as it is (`runtime::as_number_value`); `to_boolean` tests oddballs, i31s, numbers,
  strings first and BigInts last; `typeof v === "number"|"string"|"symbol"|"function"|"undefined"|"boolean"`
  inline (`runtime::typeof_is`). Numeric (with BigInts), truthiness and typeof matrices equal node.
- 2026-10-03: note on `micro75.sh` with two compilers: the second one measured is ~20% slower whichever it is
  (machine state after the first batch); compare in both orders, or interleave like the benchmark runs below.
- 2026-10-03: benchmark, base / after steps 2–4 (s4) / after the numeric, toInteger, `+v`, to_boolean, typeof changes
  (s5), interleaved, min of 5, all 15 runs checksum 485686016: exposes ×200 2,438 / 2,074 / 1,941 ms (−20%),
  find 549 / 541 / 515 ms.
- 2026-10-03: profile of s5: T75 bucket 15.1% (box_number 4.4: `numeric` 1.9 = non-integer results, `js_add` 1.0;
  to_string 3.1: `$$thisString` 0.9, `js_add` 0.8, `array_join` 0.7; typeof 1.3 = `typeof_string` from JSON's
  `const t = typeof value`). Then: `$$thisString` returns a string first (inline typeof test), `js_add` concatenates
  two strings without `to_string`, `typeof` tests numbers inline. `+` on mixed values, string methods on any
  `this`, typeof equal node.
- 2026-10-03: `cargo test` passes. `diff.sh` on arrays, builtins, classes, closures, exceptions, generators, globals,
  objects, promises, syntax, async and the top-level fixtures: 165 pass; `async/timers-more.js` failed under the
  parallel load (timer order, F250) and passes alone. node, commonjs, modules, using, wasi: all pass
  (`node/child-process-shell.js`, `node/fetch.js` failed under load and pass alone; `unsupported/` holds rejection
  tests). test262 full subset started on a-03 (user approved).
- 2026-10-03: test262 full subset on a-03 (31,461 tests, 1,171 s, -j 36; the host was busy: load ~30 with none of
  our processes; locally module size +0.75% and `wasmtime compile` time unchanged): 28,075 of 28,984 run pass
  (96.9%), 0 regressions, 2 changes (timeout → pass `Array/prototype/map/15.4.4.19-8-c-ii-1`, fail → pass
  `for-await-of/async-gen-decl-dstr-array-elem-iter-rtrn-close-null`); baseline not updated. design.md D1 note,
  HISTORY.md, follow-ups F323–F324, T76 handoff; done.
- 2026-10-03: docs: zigbee2mqtt-feasibility.md speed note (T74/T75 benchmark numbers); keywords.md unchanged (no feature change). Committed and pushed (user's request).
