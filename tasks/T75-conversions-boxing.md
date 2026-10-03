# T75: conversions, operators and number boxing (speed)

Status: todo (handoff written 2026-10-03, after T74) | Depends on: T74 (benchmark, profiling setup)

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
- [ ] The benchmark faster, same checksum; numbers recorded.
- [ ] No fixture or test262 regressions.
- [ ] Follow-ups and the T76 handoff updated.

## Notes / Result
(to be written)

## Progress
- 2026-10-03: handoff written after T74 (`main` at `e12241e`).
