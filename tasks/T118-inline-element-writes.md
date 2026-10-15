# T118: array element writes inline on receivers of unknown type (F422)

Status: done | Depends on: T107, T113, T115 | Local (+ a-03 only after asking)

## Goal
F422 (found in T115): element writes `o[k] = v` with a non-constant key on receivers of unknown type call the
generic `set` (or `set_num` for a number value) every time, while T115 made the matching reads inline (nbody
−12.7%, fannkuchredux −9.9%). The user chose F422 on 2026-10-15, starting with a count run.

## Read first
- `tasks/T115-inline-element-reads.md` (the read side: `elem_get`, `get_other`).
- `crates/backend/src/object.rs` `set` (its array fast path), `set_num`; `crates/backend/src/array.rs` `set_index`
  (its first fast path: an element that's there).

## Plan
- [x] Count run: counters `set.call`, `set.typed_i31`, `set.array_i31`, `set.array_overwrite` (what
  `array_set_index`'s first fast path takes), `set_num.call`, `set_num.nums_overwrite`, `set_num.generic_overwrite`
  (`build --count` only); `scripts/bench.sh -c` on the laptop; report and decide.
- [x] Design (user, 2026-10-15: "yes build it"): `lib.rs` `elem_set`, T115's model for writes.
- [x] Checks: `scripts/diff.sh` on `arrays/` (new `elem-writes.js`), `objects/`, `classes/`, `builtins/`.
- [x] Gate on a-03 against `242c339` (ask); test262 (ask).

## Progress
- 2026-10-15: task file; counters added in `object.rs` `set`/`set_num` and `runtime.rs` `COUNTS` (count builds
  only: `runtime::count` emits nothing otherwise).

- 2026-10-15: count run on the laptop (`scripts/bench.sh -c -o out/count-t118`, all 18 SAME; table in
  `out/count-t118/writes.txt`). A first run counted `set.typed_i31` after the typed-array branch (which returns), so
  it was 0 everywhere; moved before it and rerun. Element writes per program (`set` calls / of them `$array` + i31
  key / the overwrite `array_set_index` takes first; `set_num` calls / number-mode overwrites / generic overwrites):
  fannkuchredux 168.4M / 168.4M / 168.4M; 8.8M / 0 / 8.8M. crypto 3.1M / 2.9M / 2.7M; 33.2M / 0 / 32.9M.
  navier-stokes 1.6M / 1.6M / 1.5k; 17.1M / 16.9M / 51k. spectralnorm 1k; 10.0M / 10.0M / 24. richards 2.5M / 386k /
  380k; 2.1M / 0 / 0. Typed-array writes (`set.typed_i31`): mandelbrot 1.6M, revcomp 1.3M. regexp 243k / 226k
  overwrites; earley-boyer 232k / 111k + 84k generic `set_num`; deltablue 45k. binarytrees, fasta, nbody, pidigits,
  raytrace, splay: none.
- 2026-10-15: richards' 2.1M `set_num` calls that miss are not element writes: `this.v2++` (any `o.k++` / `o.k--`,
  `member_update` in `crates/lower`) lowers to `key = "v2"; $get(o, key) ... $set(o, key, f64->jsval(..))`: the
  constant key goes through a temp, so the read is the generic `get` and the write `set_num` → `set`'s generic path,
  never `get_named`/`set_named`, T110's inline slots or T111's number cells (repro: a scratch `w.js`, `js2wasi3 ir`).
  Outside F422: a follow-up.
- 2026-10-15: built `elem_set` (`lib.rs`): `SetProp(obj, key, value)` with a non-constant key overwrites an
  array element inline under `array_set_index`'s first fast path conditions (an `i31` or integer-valued `f64` key
  below `length`, no number mode, a hinted array only with an `i31`, no element attributes, not sealed/frozen, an
  element there); an `f64` value (T107's `set_num` sites): number mode in place (not a hole, no
  `preventExtensions`/`seal`/`freeze`/read-only `length`, NaN canonicalized), or an integer as an `i31` into a
  generic store; misses call `set`/`set_num`, an `f64` key boxed only then. Two scratch locals per function
  (`(ref null $array)`, `f64`). Counters `elem_write.hit`/`nums`/`miss`.
- 2026-10-15: new fixture `arrays/elem-writes.js` (holes with a prototype setter, frozen/sealed/non-extensible,
  read-only elements and `length`, accessor elements, -0/NaN/2^31/fractional keys and values, plain objects,
  functions, Maps, typed arrays, strings, null, `arguments`). First version printed while `Array.prototype[1]`
  had a setter (node's own `console.log` broke) and used an `Array` subclass (`Sub.from`: plain arrays on the base
  compiler too, F106/F108): both rewritten. `diff.sh`: arrays 9/9, objects 18/18, classes 12/12, builtins 73/73.
- 2026-10-15: count run again (`out/count-t118b`, all SAME): inline hits fannkuchredux 177.2M (27 misses), crypto
  35.5M (472k misses), navier-stokes 16.9M number mode + 52k, spectralnorm 10.0M number mode, richards 380k (2.1M
  misses = F428's `this.v2++`), regexp 226k, earley-boyer 195k; typed arrays miss (mandelbrot 1.6M, revcomp 1.3M).
  Laptop gate (`out/gate-t118-laptop.log`) running.
- 2026-10-15: laptop gate vs `242c339` (`-r 3`, six programs, `out/gate-t118-laptop.log`): fannkuchredux −21.9%
  (instructions −25.1%), spectralnorm −14.9% (−13.2%), crypto −7.1% (−4.6%), navier-stokes −2.6% (−2.7%), richards
  same, regexp +2.5% at −0.05% instructions (placement). Micro: `o[k] = v` (key `"a"`, every receiver misses) +4…+6
  ns, but unrelated cases (`typeof`, `instanceof`, `==`) moved ±2…5 ns too: laptop noise; the a-03 gate decides.
- 2026-10-15: a-03 (user: "yes run gate and test262 on a-03"): synced at the working tree, `scripts/gate.sh -o
  out/gate-t118 242c339` into `~/gate-t118-a03.log`, then `node scripts/test262.mjs` into `~/t262-t118.log`; both
  copied to `out/` when done.
- 2026-10-15: a-03 gate vs `242c339` (`out/gate-t118-a03.log`): fannkuchredux −25.5% (instructions −25.1%),
  spectralnorm −9.6% (−13.2%), navier-stokes −5.2% (−2.7%), crypto −3.3% (−4.6%); **mandelbrot +11.5% (instructions
  +2.9%)**, pidigits +3.1% (instructions +0.00%: placement); micro `o[k] = v` [array] and [Map] +2 ns (key `"a"`:
  misses), the rest same or faster. The gate rule (nothing slower) fails on mandelbrot.
- 2026-10-15: test262 on a-03 didn't run: my command lacked the `subset` argument (`No tests given`); restarted
  with `node scripts/test262.mjs subset`.
- 2026-10-15: mandelbrot on the laptop: 1,771.6M → 1,822.8M instructions (+51M, `perf stat`). Its 1.6M writes are
  typed-array writes (`byte_acc[0] <<= 1`, `|= 1`, `__out[__n++] = …`, all `Uint8Array`): they miss and pay the
  inline check (a `br_on_cast_fail`), which accounts for only a few instructions each, not ~31. A sampled profile
  (`perf record` with `--profile=perfmap`) was too noisy at this size (typed_set differs by 15% between the two
  builds doing the same work): the cause isn't found yet.
- 2026-10-15: diagnostic (user: "yes run the diagnostic"; temporary `T118_DIAG` switch in `lib.rs`, reverted
  after): mandelbrot instructions on the laptop: base 1,771.65M; new 1,822.77M; inline writes off but the two new
  scratch locals kept 1,771.65M (the locals cost nothing); without the number-value path 1,771.27M; without the
  generic path 1,818.90M. So the number-value path's misses cost it: 1.52M typed-array writes (`set_num` sites),
  ~31 instructions each. A scratch micro (`Uint8Array`, `b[0] <<= 1`, `b[0] |= 1`, 1.5M writes): +24.5M, ~16 per
  write: the miss sequence itself (three `local.set`s, the `br_on_cast_fail` on an `eqref`: null, `i31`, type
  loads and compare; the call's arguments reloaded), more in mandelbrot's larger `main`. A typed write was already
  slow before: `set_num` (its `$array` test) → box → `set` → its typed test → `to_number` → `typed_set`.
- 2026-10-15: typed-array fix (user: "yes build the typed-array fix"): a number-value miss tests `$typed` next and,
  for an index below its length (not a BigInt array), calls `typed_set` with the `f64` directly (`set`'s typed path
  without `set_num`, the box and `to_number`); counter `elem_write.typed`. Fixture gets typed-array cases (every
  kind, out of range, negative keys, NaN, clamping, a view with an offset, a resizable buffer, BigInt64Array's
  TypeError). Two node differences found there are the same on `242c339` (keys `1.5` and `-0` on typed arrays):
  F429, those keys left out. `diff.sh`: arrays 9/9, objects 18/18, builtins 73/73, node 41/42 (`fetch.js` timed out
  under the parallel load; alone it passes in 30 s).
- 2026-10-15: mandelbrot on the laptop: 1,771.65M (base) → 1,615.70M instructions (−8.8%; the gated version was
  +2.9%). Count run (`out/count-t118c`, all SAME): mandelbrot 1.52M `elem_write.typed`, revcomp 21k (its 1.25M
  Buffer writes carry a `jsval` value: the generic path, a miss, as before). The a-03 test262 run in progress tests
  the version before this fix.
- 2026-10-15: test262 on a-03 of the version before the typed fix (`out/t262-t118-prefix.log`): no changes since
  the baseline, 31,461 tests in 1,514 s.
- 2026-10-15: a-03 again with the typed fix (user: "yes run gate and test262 on a-03"): `scripts/gate.sh -o
  out/gate-t118b 242c339` into `~/gate-t118b-a03.log`, then `node scripts/test262.mjs subset` into
  `~/t262-t118b.log`; both copied to `out/` when done.
- 2026-10-15: a-03 gate with the typed fix (`out/gate-t118b-a03.log`, vs `242c339`): fannkuchredux −26.1%
  (instructions −25.1%), spectralnorm −12.1% (−13.2%), crypto −2.6% (−4.6%), navier-stokes same (it was −5.2% in
  the first gate); **mandelbrot +6.3% at −8.8% instructions**, pidigits +4.6% at −0.00% (no element writes:
  placement); micro `o[k] = v` [array]/[Map] +3 ns (string key `"a"`: misses), regex cases ±, the rest same or
  faster.
- 2026-10-15: mandelbrot on the laptop (`perf stat`): base 1,771.6M instructions in 684.3M cycles (IPC 2.59),
  T118 1,615.7M in 735.8M cycles (IPC 2.20), branch misses equal (798k vs 784k); times 181–190 vs 190–199 ms. Fewer
  instructions, more cycles: the hot `for (i < iter)` loop of pure `f64` math in the same function as the writes
  runs slower per instruction (placement or register allocation of `main`), not more work.
- 2026-10-15: test262 on a-03 with the typed fix: no changes since the baseline, 31,461 tests in 1,521 s
  (`out/t262-t118b.log`).
- 2026-10-15: user picked "cut the miss cost": `elem_set` now tests the key (an `i31`, or an integer-valued `f64`)
  before the `$array` cast, so a string key misses without the cast. `arrays/` fixtures 9/9 pass (incl.
  `elem-writes.js`); `wasm-tools validate` OK. Laptop times of `object-kinds.js` are noise (`loop` and `typeof`,
  untouched, also +2…+3 ns), so instructions (`perf stat`, deterministic) per case of `object-kinds.js` vs base
  242c339: only `o[k] = v` changes, +140M over its 20M writes = 7 instructions per string-key miss (of ~670 in
  the `set` call); every other case ±0.01M. A loop of `arr[k] = 1` with `k = "a"` alone: +3 per miss (the key test).
- 2026-10-15: user picked misses without repeated tests (the T115 rule, as `get_other`): `set(cx, arrays, typed)`
  gives `set_other` (no array path) and `set_generic` (no typed and no array path; both need an `i31` key).
  `elem_set` misses: an array with an integer key → `set`/`set_num`; not an array, integer key → `set_other`
  (a number value: the inline typed path, its misses `set_num`); a key that isn't an integer, or a number value
  into neither an array nor a typed array → `set_generic` (value boxed). Counters `elem_write.miss_other`,
  `elem_write.miss_generic`. (A `cargo fmt` reformatted the crate by mistake; reverted, changes reapplied in the
  project's style.) Fixtures `arrays` 9/9, `objects` 18/18, `builtins` 73/73 (1 skip as before); validate OK.
  Instructions vs 242c339: `arr[k] = 1` loop (`k = "a"`, 10M) 7,098M → 6,528M (−57 per miss); the `o[k] = v`
  case of `object-kinds.js` 16,349M → 15,259M (−54 per miss over 20M).
- 2026-10-15: third a-03 gate vs 242c339 started (`~/gate-t118c.log`), then full test262 (`~/t262-t118c.log`).
- 2026-10-15: third a-03 gate (`out/gate-t118c-a03.log`): fannkuchredux −24.5% (instr −25.1%), spectralnorm −11.3%
  (−13.2%), crypto −3.6% (−4.5%), nbody −2.2% (0.00%); mandelbrot +5.5% at −8.77% instructions (as in the second
  gate: fewer instructions, more cycles); the rest same. Micro: `o[k] = v` −7 ns on all four kinds (was +3 on array
  and Map), `k in o` [array]/[Map] −2…−3; `bigint a + b` +4 in all three gates at equal instructions
  (`primitives.js` 107,358.63M → 107,358.64M on the laptop: placement); `v == 1 [null]` +2 and three regex
  cases ± (2k-op cases, 1 ms clock steps) flip between gates: noise. test262: no changes, 31,461 tests in
  1,517 s (`out/t262-t118c.log`).

## Notes / Result
- **What changed.** `SetProp(obj, key, value)` with a non-constant key (`lib.rs` `elem_set`; the `f64`-value form
  replaces the old `set_num` peephole) overwrites an `$array` element inline, as `array_set_index`'s first fast
  path: the key first (an `i31`, or an integer-valued `f64` unboxed, `-0` is 0), then `ref.test $array`, below
  `length`; number mode (an `f64` value): no `preventExtensions`/`seal`/`freeze`/read-only `length`, not a hole,
  a NaN canonicalized, the `f64` stored; generic store: no number mode, a hinted array only takes an `i31`, no
  element attributes, not sealed/frozen, an element that's there (an `f64` value stored as an `i31` when it fits).
  An `f64` value into a typed array at an index below its length (not BigInt) calls `typed_set` directly. Misses
  never repeat the inline tests (the T115 rule): an array with an integer key calls `set`/`set_num`; another
  receiver with an integer key calls the new `set_other` (`set` without its array path); a key that isn't an
  integer, or a number value into neither an array nor a typed array, calls the new `set_generic` (`set` without
  its typed and array paths, which both need an `i31` key). An `f64` key is boxed only for the call. Two scratch
  locals per function (`(ref null $array)`, `f64`). Counters (`build --count`): `elem_write.*`, `set.*`, `set_num.*`.
- **Gates:** the first (a-03) had mandelbrot +11.5% (instructions +2.9%: its typed-array writes missed the
  inline array path into `set_num`, ~31 instructions each): fixed by the inline typed path (−8.8% instructions).
  The second had `o[k] = v` with a string key +3 ns on array/Map (the miss paid the inline test): fixed by the
  key test first and `set_other`/`set_generic` (−54…−57 instructions per string-key miss vs `242c339`).
- **Result (a-03, `out/gate-t118c-a03.log`, vs `242c339`):** fannkuchredux 6,518 → 4,924 ms (−24.5%, instructions
  −25.1%), spectralnorm 435 → 386 ms (−11.3%, −13.2%), octane-crypto 4,917 → 4,738 ms (−3.6%, −4.5%), nbody −2.2%
  (0.00%); the rest the same; outputs = node. **mandelbrot 256 → 270 ms (+5.5%) at −8.8% instructions**: more
  cycles for fewer instructions (laptop: IPC 2.59 → 2.20, branch misses equal; its pure-`f64` inner loop in the
  same function runs slower per instruction): accepted as placement by the user (2026-10-15). Micro: `o[k] = v`
  −7 ns on all four kinds, `k in o` array/Map −2…−3; `bigint a + b` +4 ns at equal instructions (placement);
  `v == 1 [null]` and three 2k-op regex cases flip between gates (noise).
- **test262** (a-03, `out/t262-t118c.log`, 1,517 s): no changes since the baseline (28,105 pass, 97.0%); also
  `out/t262-t118-prefix.log`, `out/t262-t118b.log` for the earlier versions.
- **Commands:** `scripts/bench.sh -c` (count runs `out/count-t118*`); `scripts/diff.sh` on
  `tests/fixtures/{arrays,objects,classes,builtins}` (new `arrays/elem-writes.js`); `wasm-tools validate`;
  `scripts/gate.sh -o out/gate-t118 242c339` (laptop once, a-03 three times); `node scripts/test262.mjs subset`
  (a-03, three times); `perf stat -e instructions:u` on single cases of `object-kinds.js`/`primitives.js`.
- **Not done (follow-ups):** `o.k++`/`o.k--` take the generic `get`/`set` (F428); typed-array keys `1.5`/`-0`
  become ordinary properties (F429, pre-existing); typed-array/Buffer reads still call (F420); an `Array`
  subclass fixture case left out (F106/F108).
- **Next smallest step:** choose among F427, F404, F420, F428, F429 (`tasks/README.md`).

