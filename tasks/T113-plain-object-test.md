# T113: a plain-object test that never calls `is_subtype` (root R1)

Status: done (accepted by the user with the trade-offs below) | Depends on: T112 | Local (+ a-03 only after asking)

## Goal
Root R1 of `docs/root-causes.md`: `$object` isn't final, so wasmtime compiles `ref.test`/`ref.cast (ref $object)`
inline only for an exact `$object`; any other value (a string, a number box, a Map, an array, a function…) calls
the `is_subtype` libcall. T112 measured what that costs T110's inline sites (`tests/bench/micro/property-paths.js`,
T109 → T110): a non-object receiver +18 ns, a Map method call +31 ns, and the counts show knucleotide's 17.5M
subtype misses (+7.2%). Every later inline object path (T110, F359, T111) hits the same wall.

The goal is a test "is this a plain object?" that costs one inline compare on the hit and never calls
`is_subtype` on a miss, so a miss costs no more than the old path (T112's rule).

## Idea (to measure, not decided)
- Plain objects become a **final** type `$plain` (a subtype of `$object` with no new fields): `ref.test (ref
  $plain)` is one inline type-id compare, for every value.
- The catch (why T109 avoided a subtype): generic code casts to `(ref $object)`; for a `$plain` that is no longer
  an exact `$object`, so those casts would call `is_subtype`. 187 sites cast or test `$object` directly
  (`as_obj`, `test_object`, `ref.cast`/`ref.test` of `runtime::object()`), 16 go through `object_or`. Each must
  test `$plain` first (one compare), then the other final kinds, then `$object`.
- Alternative to compare: keep `$object` exact for plain objects and make the inline sites test with
  `br_on_cast` to the final kinds first only on the miss path (T112's laptop trial: 3 tests before the cast
  halved T110's gain: measure where the tests go, not only how many).

## Plan
- [ ] Count (`build --count`) of receiver kinds at the sites: not done (the probe and the gates decided it).
- [x] Micro-benchmarks: `tests/bench/micro/object-kinds.js` (generic paths × plain/array/function/Map) and
  `primitives.js` (`==`, `typeof`, BigInt on every primitive kind; added after the first gates missed them).
  Not covered: spread, `for-in`, `JSON.stringify`.
- [x] Prototype (working tree, not a branch); three gates on a-03 against T112's commit (`db256bc`).
- [x] T110's inline sites on top (F409): they were already in `db256bc`; they now cast to `$plain`.

## Acceptance
- `scripts/gate.sh`: no program slower (2% on minimums), no micro case slower (1 ns and 3%); T110's misses no
  dearer than T109's paths; test262 no changes (a-03, ask).

## Notes / Result
- What changed: plain objects are a final `$plain` (sub of `$object`, `runtime::T_PLAIN`; `object::alloc` and
  `alloc_sized`); `OBJECT_KINDS` lists the 11 final object types. `JsTest::cast_object` (a `br_on_cast` chain,
  `$plain` first) replaces the 79 direct `ref.cast (ref $object)`; `JsTest::is_object` (`test_object`) is
  `$plain`, then the primitive exits (`struct`, `$number`, `$oddball`), then the other 10 kinds; `object_or` loops
  over `OBJECT_KINDS` after its primitive exits. Block types `T_CAST_OBJECT`, `T_IS_OBJECT`, `T_NOT_OBJECT`
  (`T_BIGINT`/`TYPES` 34). T110's inline sites `br_on_cast_fail` to `$plain`: a Map/array/function receiver goes
  straight to `get_named`/`set_named`, no `is_subtype` (F409's cause). No `ref.test`/`ref.cast` to `$object` that
  can call `is_subtype` is left at these sites.
- Tooling: `scripts/bench.sh -b`/`scripts/gate.sh` get an `instr` column (instructions retired, `perf stat`; a-03
  needs `PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf`); probe `probes/casts.wat` (item 11).
- Results (a-03, `out/gate-t113c-a03.log`, vs T112 `db256bc`): deltablue −8.6%, richards −8.1%, earley-boyer
  −6.3%, raytrace −5.9%, knucleotide −5.2%, fannkuchredux −4.9%, nbody −4.5%, binarytrees −4.2%, crypto −4.1%,
  fasta −2.9%, 6 the same; pidigits +5.3% and mandelbrot +3.2% at equal instructions (+0.01%, +0.00%: code
  placement; pidigits' hot loops leave the uop cache, `idq.dsb_uops` 1.97e9 → 0.42e9). Instructions fell in 14
  of 18 (up to −7%). Micro: generic object paths −2…−17 ns; primitives +2…+6 ns in `==`/`typeof` (the `$plain`
  check before their exits, F411); Map/array own-`x` inline sites +5.5…+6.5 (they now take the call, F413);
  `getPrototypeOf` of a function +2 (F412). test262 (a-03): no changes (28,105, 97.0%).
- Lessons (in `docs/design.md`, "Speed work"): a shared test changed for every value kind needs a micro case for
  every kind (the first prototype dropped the primitive exits: richards +2.7% from `x == null`); two forms of one
  test must agree; a slowdown at equal instructions is placement, not work.
- Commands: `scripts/gate.sh -r 5 db256bc` (a-03, three times), `scripts/bench.sh -r 5 -b … pidigits revcomp`,
  `perf stat`/`perf record` on a-03, `node scripts/test262.mjs subset` (a-03), `scripts/diff.sh` on every fixture
  directory.
- Next smallest step: decide T110 (F409's cause is gone: close it or resume it) and T111 (F406).

## Progress
- 2026-10-12: proposed by T112 (step 6) from `docs/root-causes.md`; user agreed ("Yes 1 and 2").
- 2026-10-12: probe `probes/casts.wat` (also `probes/run.sh` item 11): 50M `ref.test` each, laptop, best of 3
  (`wasmtime run --invoke` of a precompiled module, startup included): final target `$plain`: hit 99 ms, miss on a
  subtype 97, on another struct 97, on an i31 99: **hit and miss cost the same, inline**. Non-final target `$obj`:
  exact hit 132 ms, a subtype 478, another struct 474 (**+~7.5 ns per test: the `is_subtype` call**), an i31 109.
  So a final `$plain` type gives the test T113 wants; the open question is the cost it moves onto the generic
  `(ref $object)` casts (a `$plain` is no longer an exact `$object` there).
- 2026-10-12: facts for the design: wasmtime 49 has no exact reference types (no custom-descriptors in `-W
  help`), so `ref.test (ref exact $object)` isn't available. All 10 `$object` subtypes are already final
  (`runtime.rs` type list: function, array, promise, generator, timer, map, mapiter, buffer, typed, wrapper).
  Plain objects are allocated in 2 places (`object.rs` `struct_new(T_OBJECT)` ×2: `alloc`, `alloc_sized`).
  Design note (from the probe): make plain objects a final `$plain` (sub of `$object`, no new fields; the 2
  allocation sites). Then every object is one of 11 final types, and every "is it an object?" site can classify
  with inline compares only, `$plain` first (one compare for the common case), never `is_subtype`:
  - inline sites (T110): `ref.test (ref $plain)`: same cost hit or miss (probe);
  - generic sites (187 direct `$object` casts/tests + 16 `object_or`): a chain `$plain`, then the other finals by
    frequency, typed as `(ref $object)` (all are subtypes, so fields read as before). Today a non-plain object at
    those 171 non-`object_or` sites calls `is_subtype` (~7.5 ns); after, a few inline compares. Plain objects: one
    compare, as today's exact-`$object` cast.
  - risk: a missed site still casting to `$object` gets `is_subtype` for every plain object (slower, still
    correct): the counting build can find them (a counter on `is_subtype`-prone casts is not possible from Wasm,
    but the micro-benchmarks of the generic paths show it).
  Next: micro-benchmarks of the generic paths (plain/array/function/Map receivers), then the prototype.
- 2026-10-12: prototype: `runtime::T_PLAIN` (final sub of `$object`, the type list's 31st entry; `T_BIGINT`/`TYPES`
  moved to 33), `OBJECT_KINDS` (11 final object types, `$plain` first), block types `T_CAST_OBJECT`
  (`(eqref) -> (ref $object)`) and `T_IS_OBJECT` (`(eqref) -> i32`), `JsTest::cast_object` / `is_object` (a
  `br_on_cast` per kind; a block with a parameter, since a block doesn't see the values pushed before it: the
  first try failed validation). `test_object`, `as_obj`, `object_or` and the 79 direct `ref.cast`/`ref.test` of
  `$object` use them; `alloc`/`alloc_sized` make `$plain`; T110's inline sites `br_on_cast_fail` to `$plain`.
  Fixtures: objects 16, classes 12, arrays 8, commonjs 15, modules 14, builtins 73 (1 skipped), node 42 (one
  flaky failure on the first run), async 15, generators 4, syntax 10, closures 11, exceptions 14, globals 5,
  promises 14, using 3, top-level files: all pass. `unsupported/bad-names.js` "fails" under `diff.sh` with and
  without the change (it's a build-error fixture). Gate vs T112 (`db256bc`) running (`out/gate-t113.log`).
- 2026-10-12: gate on the laptop (`scripts/gate.sh -r 5 db256bc`, `out/gate-t113.log`), prototype vs T112.
  Programs (minimums): faster: crypto −6.3%, knucleotide −5.7%, deltablue −5.7%, raytrace −4.3%, earley-boyer
  −4.3%, splay −3.1%, binarytrees −3.0%, nbody −3.0%, revcomp −2.5%, mandelbrot −2.3%; same: fannkuchredux,
  fasta, regexredux, spectralnorm, navier-stokes, regexp; **slower: pidigits +7.0%, richards +2.3%**. All outputs
  = node. Micro: faster: `in`, `hasOwnProperty`, `instanceof`, `Object.keys` on Maps, a string `.x` −7.5, a Map
  `get` call −10; **slower**: `Object.getPrototypeOf` of an array/function/Map +8…+12, computed-key reads/writes
  on array/function/Map +3…+8, `typeof` a function +6, inline read/write of a Map's or array's own `x` +8…+12
  (expected: a subtype no longer takes the inline path), inline write hit +6.0 and read hit +2.5 (but the same
  T112 code measured 12.0 / 14.5 for these earlier today, so the base's 6.5 is laptop noise: the micro noise on
  the laptop is several ns). Gate verdict: fails (pidigits, richards, several micro cases).
- 2026-10-12: user: "yes run it on a-03". Synced the working tree to a-03 (rsync), built there, started
  `scripts/gate.sh -r 5 db256bc` on a-03 (log copied to `out/gate-t113-a03.log`).
- 2026-10-12: gate on a-03 (`scripts/gate.sh -r 5 db256bc`; log copied to `out/gate-t113-a03.log`). Programs
  (minimums): faster: knucleotide −6.0%, raytrace −5.7%, nbody −5.0%, crypto −4.9%, earley-boyer −4.6%,
  deltablue −4.0%, binarytrees −3.7%, fasta −2.1%; same: the other 8; **slower: pidigits +5.6%** (runs 998–1024
  vs 1054–1091 ms, no overlap), **richards +2.7%** (3262–3301 vs 3350–3393, no overlap): both real. All outputs =
  node. Micro: almost every generic case faster (`in`, `hasOwnProperty`, `Object.keys`, `instanceof`, `typeof`
  −3…−18 ns; Map `get` −13; string `.x` −11.5; T110 misses −3…−5); laptop's computed-key and `typeof` slowdowns
  were noise. **Slower**: `getPrototypeOf` of a function/Map +3/+2; inline read hit +3.0, write hit +1.5; inline
  read/write of a Map's/array's own `x` +5.5…+7.5 (expected). Gate verdict: fails (pidigits, richards, inline
  hits, getPrototypeOf). Not investigated yet (waiting for the user).
- 2026-10-12: investigation (no code changed; scratch builds of both compilers, `wasmtime objdump`):
  - inline read/write hit (+3.0/+1.5 ns on a-03): the hit loop's Wasm differs only in the `br_on_cast_fail`
    target (`$plain` instead of `$object`); its machine code got *shorter* (the `is_subtype` fallback is gone, nothing
    else changed), and `js_add`'s number path (called twice per iteration) compiles to the same instructions. No
    code cause on the hit path.
  - pidigits (+5.6%): all 37 hot BigInt helpers compile to the same instructions (`bigint_div` differs only in
    padding; the Wasm only in BigInt's type index, 30 → 33). What moved: every hot function starts 32 bytes off
    (e.g. `bigint_mul` at 0 mod 64 before, 32 now; `bigint_div` 32 → 0). Likely a code-placement effect, not the
    change's work.
  - richards (+2.7%): 100 functions differ, mostly the generic helpers with the longer `$object` chain
    (`get_named`, `set_named`, `js_add`, `js_less`…). On a plain receiver the new chain is 1 compare vs the base's 4
    (function, array, Map, then exact `$object`), so the chain doesn't explain it either; not resolved.
  - test of the placement idea: `wasmtime compile -C cranelift-log2_min_function_alignment=6` aligns every
    function to 64 bytes in both builds. pidigits 2000 on the laptop, 7 alternating runs: too noisy to decide (first
    runs 845–995 ms, the rest 1080–1230). Needs a-03 (not run; asking).
- 2026-10-12: alignment test on a-03 (user: "ok run"; `~/t113-align.sh`, log in `out/t113-align-a03.log`): the gate's
  builds, each compiled default and with `-C cranelift-log2_min_function_alignment=6`, 5 alternating runs.
  pidigits: default +6.6%, aligned **+4.0%** (978–998 vs 1017–1026 ms, no overlap); richards: default +3.2%,
  aligned **+2.7%** (no overlap). So function placement explains at most part of pidigits and none of richards:
  the placement idea above was wrong as the main cause. Micro: the inline read hit's +2.5 is the subtracted loop
  (raw: base 14.0 + 20.5 = 34.5, new 16.5 + 18.5 = 35.0 ns; aligned 34.0 vs 36.0); the write hit raw is equal
  (32.5 vs 32.5; aligned 32.0 vs 31.5). `getPrototypeOf` of a function +4/+5 holds in both (a function is the
  chain's 3rd kind). Causes of pidigits and richards still open; next would be a `perf` profile on a-03 (ask).
- 2026-10-12: `perf` profiles on a-03 (user: "yes"; `~/t113-prof.sh`, `perf record -F 4999` + `wasmtime run
  --profile=perfmap`, the gate's default builds; tops in `out/*.top`):
  - richards: **all of it is `loose_eq`** (`==`): 1473 → 2645 samples (+1172; the whole run +~1000). Cause: the
    new `is_object` (used by `test_object`) dropped the old test's early exits: before, a value that isn't a
    struct (string, i31, BigInt) took one abstract `ref.test struct`, a number box or oddball (`null`/`undefined`)
    two exact tests, function/array two more, then `$object`. Now every non-object runs all 11 `br_on_cast`
    misses; `loose_eq` tests both operands first, so `x == null` costs 22 failed compares. A defect of the
    prototype, not of the `$plain` idea.
  - pidigits: the extra time is in `bigint_div` (917 → 1137 samples) and `bigint_mul` (2578 → 2726), whose
    code is identical; `libc` (+53). BigInts are arrays, so every `is_object` on one also runs the 11 misses now,
    but that is a few ns per operation against ~400 ns operations: not explained yet.
  - `getPrototypeOf` of a function (+4…+5 ns): a function is 3rd in `OBJECT_KINDS`.
  Not fixed (waiting for the user).
- 2026-10-12: user: "ok fix it". `JsTest::is_object` now leaves early for primitives as `object_or` does:
  `br_on_cast_fail` to the abstract `struct` (small integer, string, symbol, BigInt), then `$number`, `$oddball`,
  then the 11 kinds (new block type `T_NOT_OBJECT` `(eqref) -> eqref`; `T_BIGINT`/`TYPES` → 34). `cast_object`
  unchanged (only ever given objects: it traps otherwise). New micro-benchmark `tests/bench/micro/primitives.js`
  (`==`, `typeof`, BigInt `+`/`*` on null, undefined, boolean, small int, double, string, symbol, BigInt, plain).
  Validated; `==` on primitives = node. Fixtures running.
- 2026-10-12: fixtures with the fix: all pass. node: `fetch.js` failed under load (a 108 s timeout; my loop
  ran every directory twice, so 3 node failures in the first pass, only `fetch.js` in the second) and passes
  alone; `unsupported/bad-names.js` as before (a build-error fixture). Next: the gate on a-03 (ask).
- 2026-10-12: gate on a-03 with the fix (user: "yes run gate on a-03"; `out/gate-t113b-a03.log`) vs T112.
  Programs: faster: richards −7.9% (was +2.7%: fixed), deltablue −8.4%, raytrace −6.0%, knucleotide −5.6%,
  earley-boyer −4.8%, crypto −4.1%, fasta −2.2%; **slower: pidigits +5.5%** (unchanged: not the BigInt object test),
  **revcomp +3.5%** (runs overlap: 519–548 vs 537–558; +1.0% in the previous gate). Micro `primitives`: all same or
  faster except `v == 1` on a small int +3 and on a symbol +2; BigInt `+`/`*` same. Other micro slower: inline
  read/write of a Map's/array's own `x` +6…+7 (expected), `getPrototypeOf` of a function +3, `o[k] = v` on a
  function +4 / Map +2, read hit +2.0, **literal `{x, y}` +7.5** (+1.5 before the fix). Gate verdict: fails.
- 2026-10-12: investigation of the second gate (user: "yes look into it"; no code changed):
  - literal `{x, y}` +7.5 ns: `layout_define` (once per property) does `test_object` then `cast_object`. T112:
    one exact `$object` compare each; the prototype before the fix: `$plain` first (1 + 1); with the fix the
    primitive exits (`struct`, `$number`, `$oddball`) run *before* `$plain`: 3 more checks per property for the
    commonest value. Better order: `$plain` first, then the primitive exits, then the other kinds (not done).
  - pidigits +5.5%: `perf stat` on a-03 (gate builds, arg 2000, 2 runs each): instructions retired equal
    (10.0305e9 vs 10.0308e9), cycles +6.2%, cache/TLB misses and branch misses equal. Front end: uops from the
    decoded-uop cache (`idq.dsb_uops`) 1.97e9 → 0.42e9, from the legacy decoders (`idq.mite_uops`) 6.6e9 → 8.2e9.
    So the same instructions, but the hot BigInt loops no longer run from the uop cache: a code-placement effect
    (32-byte windows / DSB sets; Cascade Lake also has the JCC-erratum rule), not work T113 added. Aligning
    function starts to 64 bytes (earlier test) recovered only part, consistent with placement inside the cache
    rather than function starts. pidigits can move ±5% on any change that shifts its code.
- 2026-10-12: user: "yes" (both). (1) `is_object` order: `$plain` first (from `eqref`, one compare), then the
  primitive exits (`struct`, `$number`, `$oddball`), then the other 10 kinds. `object_or` unchanged (primitive
  exits first, then `$plain`: as in T112's order). Validated; `==` = node; fixtures objects 16, classes 12,
  builtins 73, exceptions 14, closures 11: all pass. (2) `scripts/bench.sh -b` (so `scripts/gate.sh`): an `instr`
  column, instructions retired base → new from one more run of each under `perf stat -e instructions:u` when perf
  can count (PATH or `PERF=`; a-03 needs `PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf`); also in gate.tsv.
  Laptop check (`-r 1`, pidigits and fasta): pidigits instr +0.01%, fasta −0.11%. Next: the gate on a-03 (ask).
- 2026-10-12: user: "run only pidigits and revcomp". a-03, `bench.sh -r 5 -b` (base db256bc), with `instr`:
  pidigits +4.8% (994–1011 vs 1042–1063 ms), instr **+0.01%**: same work, placement (as the `perf stat` above);
  revcomp **+0.4% same** (530–543 vs 532–552), instr −0.11%: the +3.5% of the previous gate was noise.
- 2026-10-12: gate on a-03 with `$plain` first (user: "yes run gate on a-03"; `out/gate-t113c-a03.log`, with
  `instr`). Programs: faster: deltablue −8.6%, richards −8.1%, earley-boyer −6.3%, raytrace −5.9%, knucleotide
  −5.2%, fannkuchredux −4.9%, nbody −4.5%, binarytrees −4.2%, crypto −4.1%, fasta −2.9%; same: 6; slower only
  at equal instructions (placement): pidigits +5.3% (instr +0.01%), mandelbrot +3.2% (instr +0.00%).
  Instructions fell in 14 of 18 (up to −7%). Micro: literal `{x, y}` back to same (+0.5); object kinds faster
  except `getPrototypeOf` of a function +2; **primitives now +2…+6 ns** in `==` and `typeof` (each primitive
  pays the `$plain` check before its exits: the trade-off of the order); inline subtype reads/writes +5.5…+6.5
  (expected), read hit +2.0.
- 2026-10-12: user: "yes run test262 on a-03": started `node scripts/test262.mjs subset` (no `--update`) into `~/t262-t113.log`.
- 2026-10-12: test262 on a-03 (`~/t262-t113.log`, 1,424 s, -j 36): **no changes since the baseline**, 28,105 pass
  (97.0%). User accepted T113 ("ok agree"). Task closed: notes above, README, HISTORY, design.md, root-causes.md.
