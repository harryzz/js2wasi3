# T107: arrays that hold only numbers keep unboxed `f64` elements (F391)

Status: done (2026-10-11) | Depends on: T106 | Local

## Goal
JSC's `IndexingType` (Double vs Contiguous), `docs/bun-jsc-analysis.md` #2: an array whose elements are all numbers
keeps them in an `(array (mut f64))`; the first other value converts it, once, to today's `jsval` elements. Today
every non-i31 number stored in an array is a `$number` box: allocated per store, and copied by the copying GC on
every collection while the array lives (navier-stokes keeps six ~17k-element arrays of boxes). Profile
(2026-10-10/11, a-03): numbers (box/unbox) navier-stokes 42%, crypto 32%, spectralnorm 40% before T106; GC
navier-stokes 10%.

## Design
- Types: `$nums = (array (mut f64))` (a new type index, appended); `$array` gets a last field
  `nums: mut (ref null $nums)` (`array::F_NUMS`). **Number mode** = `nums` non-null: elements `0..length` are
  `nums[i]` (capacity `nums.len`), a hole is the NaN bit pattern `HOLE` (never produced by arithmetic; a stored NaN
  is canonicalized first, as JSC's PNaN); `elems` is unused (the empty `$vals` from creation).
- One way: **number mode → generic** (`$to_generic`: box every element into a new `$vals`, holes → null,
  `nums = null`). Every reader of `elems` that isn't number-aware goes through `array::elems(a)` (converts first), so
  correctness doesn't depend on finding every case: accessors/attributes on elements, `delete`, `sort`, `splice`,
  inspection, … convert the array and behave as today.
- Which arrays start in number mode: an empty one (`[]`, `new Array()`), `new Array(n)` (all holes), an array
  literal whose elements are all `Num` (lowering knows). Others start generic.
- Number-aware paths (the hot ones): the index read in `get` (box the `f64`: an i31 for small integers, else a
  `$number`), `own_get` (holes stay holes, no conversion), the index write (`set_index`: a number stays unboxed, grows
  `nums`; anything else converts first), `push`, the iteration step (`iter.rs`), `length` (unchanged field).
- Typed access for numeric contexts (backend peepholes on the IR): `jsval->f64(get(o, k))` → `$get_num(o, k)` (no
  box from a number-mode array), `set(o, k, f64->jsval(x))` → `$set_num(o, k, x)` (no box into a number-mode array).

## Expected / risk
- Gains: no box per stored number, no GC copying of boxed elements, unboxed reads in numeric contexts.
- Cost: a generic read (`x[i] + y[i]` with unknown kinds) of a non-integer from a number-mode array allocates a
  short-lived box (today it returns the stored one). navier-stokes' `lin_solve` reads 4–5 elements per store in
  `js_add` chains: it may lose without a second step (stage 2: `+` chains over element reads computed in `f64` when
  every operand is a number, decided at run time). Measure stage 1 first.

## Read first
- `crates/backend/src/array.rs` (layout, `new`, `own_get`, `ensure`, `set_index`, `resize`, `push`),
  `object.rs` `get`/`set`/`delete`/`has`, `iter.rs` `step`, `inspect.rs` `format_array`, `runtime.rs` `types`;
  the 38 `F_ELEMS` sites (`grep -n F_ELEMS crates/backend/src/*.rs`).

## Plan
- [x] Stage 1a: `$nums`, `F_NUMS`, `HOLE`, `$to_generic`, `array::elems` at every non-aware `F_ELEMS` read; every
  array still generic (nothing starts in number mode): all fixtures and test262 subsets as before.
- [x] Stage 1b: number mode at creation (empty, `new Array(n)`); number-aware `get`/`own_get`/`set_index`/`push`/
  iteration; a fixture (`tests/fixtures/arrays/number-mode.js`: holes, NaN/-0 round trips, a string stored into a
  number array, accessors/`defineProperty`/`delete`/`freeze` on one, `sort`, `splice`, `length` writes, spread,
  `console.log`, prototype elements behind holes).
- [x] Stage 1c: `$get_num` / `$set_num` peepholes; number literals start in number mode.
- [x] Measure (a-03, 18 benchmarks, ask); stage 2 only if the measurement asks for it (it did: the hint, build-time literal states, `ElemNum` for `+=`, the generic fast paths; seven runs).
- [x] test262 full run (ask); docs.

## Acceptance
- Every fixture = node; test262 0 regressions; navier-stokes / crypto / spectralnorm faster; nothing slower beyond
  noise (F399: ±5% layout effects; compare hot IR when a program without number arrays moves).

## Notes / Result
**What changed** (backend + one lowering case):
- Types/fields: `runtime::T_NUMS` (`(array (mut f64))`, 29; `$bigint` 30), `$array` fields `nums` (`array::F_NUMS`,
  null unless in number mode) and `hint` (`array::F_HINT`, i32). `array::HOLE` (a NaN pattern; stored NaNs are
  canonicalized).
- States: generic (`nums` null); hinted (generic, every element a number or a hole so far: `[]`, `new Array(n)`,
  all-i31 literals); number mode (`nums`, `elems` empty). Hinted → number mode on the first non-i31 number stored
  (`ToNums`, checks every element and attributes); any non-number, or any path that doesn't know these states
  (`Elems`: `inspect`, `join`, `delete`, `in`, `sort`, element attributes, …), makes the array generic for good.
- Number-aware paths: `get` (tries `elems` below its size first: unchanged for generic arrays), `get_num` /
  `set_num` (backend peepholes for `jsval->f64(get(o, k))` / `set(o, k, f64->jsval(x))`), `set_index` (an
  overwrite fast path first; one `nums` load; hint logic), `resize`, `push`/`pop`, `own_get`/`ElemAt`, the
  iteration step, `NumsResize`. Literals choose their state at build time (`NewHinted` / `NewGeneric` / checked
  `New`); internal arrays (arguments, rest, key lists, …) are `NewGeneric`.
- Lowering: `o[k] = <number>` keeps its value in an `f64` local (no boxed `Tee`); the statement `o[k] += <number>`
  (`Lowerer::elem_add_stmt`) reads the element first (`ElemNum`, new IR intrinsic, + `F64IsHole`), then the right
  side, then stores (`set_num` or `js_add` + `set`).

**Result** (a-03, seventh run, T106 → T107, 5 interleaved runs, outputs the same): spectralnorm 874 → 443 ms
(−49.3%), navier-stokes 3,508 → 2,856 (−18.6%), fannkuchredux −8.7%, crypto −6.9%, fasta −5.2%, pidigits −1.8%,
nbody −1.2%, richards −1.0%; splay +0.6% (noise); knucleotide +2.3% and deltablue +1.7% left (F400); the rest within
±1% (mandelbrot +2.9% overlapping).

**How it got there** (each step measured on a-03, see Progress): stage 1 alone was mixed (navier −13%, but splay
+6%, raytrace +5%, fannkuch +4%); causes found and fixed one by one: integer arrays in number mode (→ the hint),
calls on generic paths (→ inline fast paths), typed arrays through `get_num`'s extra call (→ inline), generic
`+=` reads boxing (→ `ElemNum`), literal scans (→ build time), internal arrays converting back and forth (→
`NewGeneric`), `pop` (→ number-aware), and the hint as a shared object the copying GC forwarded per array (→ an
i32 field; the field experiment showed a null ref field alone costs ≤ 0.5%).

**Tests.** `tests/fixtures/arrays/number-mode.js` (50 cases: holes, NaN/-0, conversions, attributes, freeze/seal,
sort/splice/length, the hint, `+=` order and getters, pop) = node; every fixture dir passes (the network fixtures
time out only under laptop load, as fast as HEAD alone); `cargo test` ok; full test262 on a-03 after each of the
seven builds: no changes (28,105 of 28,984).

**Not done.** F400 (knucleotide +2.3%, deltablue +1.7%: the remaining cost of the states on generic arrays). F401 (a
generic read of a non-integer from a number-mode array allocates a box: `x[i] + y[i]`, comparisons; a lowering that
keeps such reads `f64` when both operands are numbers at run time would remove it). Object properties still box
(F323).

**Next smallest step.** F401's comparison/`+` case, or F336 (regex literals compiled ahead of time).

## Progress
- 2026-10-11: task created (user: "F391 - next"). Read the array layout (one creation site, `array::new`; 38
  `F_ELEMS` sites in `array.rs`, `object.rs`, `inspect.rs`, `iter.rs`, `props.rs`), `get`'s array fast path (i31
  key, in range, not an accessor) and navier-stokes' `lin_solve` (stores `(…) * invC`, an `f64`; reads in `js_add`
  chains). Design and the risk above.
- 2026-10-11: stage 1a done: `runtime::T_NUMS` (29; `$bigint` is 30, `TYPES` 30), `$array` field `nums`
  (`array::F_NUMS` = 10), `array::HOLE`, `Helper::Elems` (`array_elems`: leaves number mode, boxing every element
  into a new `$vals`, holes null); all 20 `F_ELEMS` reads in `array.rs` (9 functions), `object.rs` (6), `inspect.rs`
  (3), `iter.rs`, `props.rs` call it (`elems_h`); `set_attrs` and `ensure` get it passed. Nothing is in number mode
  yet. Release build clean; hello validates and runs; `scripts/diff.sh` on `arrays/`, `builtins/`, `objects/`,
  `classes/`, `syntax/`, `generators/`: 119 pass.
- 2026-10-11: stage 1b (first part): `G_EMPTY_NUMS` (an immutable empty `$nums`); `array::new` puts an empty array
  in number mode; `Helper::NumsResize` (grow doubling, holes = `HOLE`, `length`), `Helper::ElemAt` (boxed / null for a
  hole), `array::num_value` (an i31 or `$number` → `f64`, NaN canonicalized); number-aware `set_index` (both [[Set]]
  and define: a number, no NO_EXTEND/SEALED/FROZEN/LENGTH_READONLY, a hole only without `G_INDEX_PROTO`),
  `resize`, `own_get`, `get`'s index fast path (holes go the general way), the iteration step. Builds, validates; a
  probe (push, holes, NaN, `-0`, `length` both ways, a string stored, for-of over holes) = node. `scripts/diff.sh` on
  `arrays/`, `builtins/`, `objects/`, `classes/`, `syntax/`, `generators/`, `async/`, `exceptions/`, `closures/`,
  `node/`: all pass (`node/net.js` timed out once at 100 s, then passed twice in 24 s: network timing, HEAD 27 s).
- 2026-10-11: fixture `tests/fixtures/arrays/number-mode.js` (31 cases: holes behind a prototype element, NaN/-0/
  Infinity, a string or object stored in, delete, a getter, read-only, frozen/sealed/non-extensible, sort, splice,
  slice/concat, spread, length both ways, map/filter/reduce, join/indexOf/includes, JSON/keys/String, `new Array(n)`,
  pop/shift, forEach, destructuring) = node at once.
- 2026-10-11: stage 1c: `object::Helper::GetNum` / `SetNum` and backend peepholes (`jsval->f64(get(o, k))`,
  `set(o, k, f64->jsval(x))`, non-constant keys); the lowering's `o[k] = <number>` keeps its value in an `f64`
  local (it used to `Tee` the boxed value) so the store sees `f64->jsval(x)`. `array::new`: an all-number literal
  starts in number mode (its boxed `elems` dropped). `new Array(n)` is `[]` + `length = n` in the prelude: number
  mode already (holes). Laptop (3 runs each, noisy): navier-stokes 5.1–6.2 → 4.0–4.2 s, outputs the same.
  `scripts/diff.sh` on every fixture dir and the top-level fixtures: all pass (`unsupported/bad-names.js` is an
  expected build error, the same on HEAD); `cargo test -p js2wasi3-backend -p js2wasi3-lower` ok.
- 2026-10-11: a-03 (user: both runs now): `~/js2wasi3-t106` (HEAD) vs `~/js2wasi3-t107` on the 18 benchmarks (`~/t107-bench.sh`, `~/t107-bench.log`), then the full test262 (`~/t262-t107.log`), one detached script (`~/t107-all.sh`).
- 2026-10-11: a-03 stage 1 (T106 vs T107 first build, 5 interleaved runs, median ms, every output the same):
  | program | T106 ms | T107 ms | change |
  |---|---|---|---|
  | mandelbrot | 247 | 254 | +2.8% |
  | spectralnorm | 873 | 870 | -0.3% |
  | fannkuchredux | 8,252 | 8,559 | +3.7% |
  | nbody | 5,229 | 5,332 | +2.0% |
  | binarytrees | 9,202 | 9,251 | +0.5% |
  | fasta | 902 | 853 | -5.4% |
  | knucleotide | 5,117 | 5,199 | +1.6% |
  | pidigits | 991 | 979 | -1.2% |
  | regexredux | 3,039 | 3,057 | +0.6% |
  | revcomp | 511 | 522 | +2.2% |
  | octane-richards | 4,602 | 4,574 | -0.6% |
  | octane-deltablue | 3,725 | 3,825 | +2.7% |
  | octane-raytrace | 7,337 | 7,685 | +4.7% |
  | octane-navier-stokes | 3,569 | 3,110 | -12.9% |
  | octane-crypto | 5,781 | 5,314 | -8.1% |
  | octane-splay | 3,522 | 3,737 | +6.1% |
  | octane-earley-boyer | 10,280 | 10,325 | +0.4% |
  | octane-regexp | 22,159 | 22,834 | +3.0% |
  navier-stokes −12.9%, crypto −8.1%, fasta −5.4%; but splay +6.1%, raytrace +4.7%, fannkuchredux +3.7%, regexp
  +3.0%, deltablue +2.7%, nbody +2.0% (no overlap). Likely: every generic `elems` read became a call
  (`array_elems`; `set_index` made up to three per write), the iteration step called `elem_at` per element, and
  reads of number-mode elements called `box_number`. Fixed: `set_index` and `resize` leave number mode once after
  their fast path and read `elems` directly (`ensure` too: only `set_index` calls it); the iteration step reads
  `elems` inline for generic arrays; `runtime::box_inline` (the box helper's body as an emitter) in `get`'s
  number-mode read. A try to give an emptied conversion room for 4 broke `inspect_props` (it takes the length of
  an `own_symbols` result's `elems` as its count): conversion now makes exactly `length` slots. Fixtures again
  all pass (`arrays/`, `builtins/`, `objects/`, `classes/`, `syntax/`, `generators/`, `async/`, `exceptions/`,
  `closures/`). The full test262 now running on a-03 is the first build's.
- 2026-10-11: test262 of the first build (a-03, `~/t262-t107.log`, 1,282 s): no changes since the baseline (28,105).
- 2026-10-11: rerun on a-03 with the fixed build (user: both, now): `~/t107b-bench.log`, then `~/t262-t107b.log`.
- 2026-10-11: a-03 rerun with the fixes (`~/t107b-bench.log`): the same picture (navier-stokes −12.8%, crypto
  −8.6%, fasta −6.4%; splay +7.2%, mandelbrot +7.8%, raytrace +5.6%, fannkuchredux +4.8%, regexp +3.4%, nbody +2.9%,
  deltablue +2.7%). So not the calls: (1) integer arrays (fannkuchredux) in number mode convert on every read and
  write what was a free i31; (2) arrays of objects started in number mode and converted on their first element;
  (3) typed arrays (mandelbrot's `Uint8Array`) took `get_num`/`set_num`'s extra call layer. Changed the policy
  (JSC's Int32 vs Double kinds, roughly): a third state, the **hint** (`nums` = `runtime::G_NUM_HINT`, a distinct
  empty `$nums`; replaces `G_EMPTY_NUMS`; no class bit is free: bits 13+ are the Map identity hash): empty arrays
  and all-i31 literals are generic + hinted; `set_index` keeps the hint for an i31, converts to number mode for
  another number (`Helper::ToNums`: checks every element, no attributes; else drops the hint), drops it for
  anything else; `Elems` drops it; `resize` keeps it (holes are fine); `array::is_num_mode`. Literals with a
  non-i31 number start in number mode. `get_num` converts `get`'s result inline (i31 / `$number`), `set_num` boxes
  inline. Fixture: 7 hint cases added (= node); fixture dirs as above all pass. The second test262 run (on the
  previous build) is running on a-03 (`~/t262-t107b.log`).
- 2026-10-11: local check of the hint build: fannkuchredux +20% (9.1 → ~11 s). Microbenchmarks (12-element int
  array, 3M rounds) located it: writes of ints +25–30% (`set_index`'s head tested the state three times;
  `set_num` added a call layer before `set`), reads +9%. Fixed: `set_index` starts with an overwrite fast path
  (index below `length`, generic or hinted-with-an-i31, no element attributes, not sealed/frozen, element there);
  `set_num` and `get_num` handle generic/hinted arrays inline (no `get`/`set` call); number mode keeps `elems` empty
  (`to_nums` empties it), so `get` and `get_num` try `elems[j]` below the store's size first (exactly HEAD's read
  for generic arrays) and test the mode only after. Micros now at or below HEAD (writes 1.8–2.1 vs 2.2–2.4 s, reads
  1.25–1.34 vs 1.28–1.48 s, number writes 2.8–2.9 vs 3.0–3.7 s; laptop). Fixture dirs all pass. The laptop is too
  loaded now for the programs (±30% between runs of one program): a-03 decides.
- 2026-10-11: test262 of the second build (`~/t262-t107b.log`): no changes. Third a-03 run with the hint + fast-path build (user: both, now): `~/t107c-bench.log`, then `~/t262-t107c.log`.
- 2026-10-11: third a-03 run (`~/t107c-bench.log`, the hint + fast-path build): navier-stokes −15.7%,
  fannkuchredux −11.8%, crypto −7.2%, fasta −4.1%; spectralnorm +9.5%, splay +6.3%, knucleotide +3.1%, raytrace
  +2.4%, deltablue +1.8%; outputs the same. Causes found: (a) spectralnorm's `Au[i] += eval_A(i,j)*u[j]` reads
  `Au[i]` generically for `js_add`: from a number-mode array that's a new box per read (3 allocations per step,
  was 2); (b) splay builds `[0, …, 9]` per payload and `new` scanned it at run time. Changes: array literals pick
  their state at build time (`array::Helper::NewHinted` for all-i31 constants or `[]`, `NewGeneric` when an element
  isn't known to be a number, `New` with its check only for other all-number literals); the statement
  `o[k] += <number>` (computed key) is lowered by `Lowerer::elem_add_stmt`: `tf = ElemNum(o, k)` (new IR intrinsic:
  the element of a number-mode array unboxed, else the hole's bits), `old = get(o, k)` only when `F64IsHole(tf)`
  (new intrinsic), then the right side once, then `set(o, k, js_add(old, x))` or `set(o, k, f64->jsval(tf + x))`
  (the backend's `set_num`): the read stays before the right side, and number-mode arrays never box. Fixture: 9 `+=`
  cases added (a right side that changes or converts the array, a getter/setter element, a key object's
  `toString`, holes, strings, a plain object, a `Float64Array`) = node.
- 2026-10-11: fixture dirs (`arrays/`, `builtins/`, `objects/`, `classes/`, `syntax/`, `generators/`, `async/`,
  `exceptions/`, `closures/`, `node/`) pass; `node/fetch.js`, `fs-streams.js`, `http-server.js` hit the 100 s timeout
  in the full run (laptop load ~6) and pass alone, as fast as HEAD (fetch 45 vs 52 s, fs-streams 34 vs 37 s).
  `cargo test` (backend, lower, ir) ok.
- 2026-10-11: test262 of the third build (`~/t262-t107c.log`): no changes. Fourth a-03 run (user: both, now): `~/t107d-bench.log`, then `~/t262-t107d.log`.
- 2026-10-11: fourth a-03 run (`~/t107d-bench.log`), T106 vs T107, 5 interleaved runs, median ms, outputs the same:
  | program | T106 ms | T107 ms | change |
  |---|---|---|---|
  | mandelbrot | 247 | 257 | +4.0% |
  | spectralnorm | 881 | 474 | -46.2% |
  | fannkuchredux | 8,362 | 7,535 | -9.9% |
  | nbody | 5,284 | 5,164 | -2.3% |
  | binarytrees | 9,290 | 9,312 | +0.2% |
  | fasta | 916 | 881 | -3.8% |
  | knucleotide | 5,185 | 5,315 | +2.5% |
  | pidigits | 1,000 | 1,001 | +0.1% |
  | regexredux | 3,072 | 3,096 | +0.8% |
  | revcomp | 515 | 522 | +1.4% |
  | octane-richards | 4,655 | 4,599 | -1.2% |
  | octane-deltablue | 3,761 | 3,849 | +2.3% |
  | octane-raytrace | 7,424 | 7,575 | +2.0% |
  | octane-navier-stokes | 3,577 | 3,125 | -12.6% |
  | octane-crypto | 5,869 | 5,409 | -7.8% |
  | octane-splay | 3,544 | 3,728 | +5.2% |
  | octane-earley-boyer | 10,364 | 10,391 | +0.3% |
  | octane-regexp | 22,505 | 22,372 | -0.6% |
  spectralnorm −46.2%, navier-stokes −12.6%, fannkuchredux −9.9%, crypto −7.8%, fasta −3.8%, nbody −2.3%. Left:
  splay +5.2%, knucleotide +2.5%, deltablue +2.3%, raytrace +2.0% (no overlap); mandelbrot +4.0% and revcomp +1.4%
  overlap. Suspect for the object-heavy ones: `$array` is 8 bytes bigger (`nums`), and splay is 78% GC (copying).
  Next: measure HEAD + an unused `$array` field against HEAD on those four, to know whether merging `nums` with
  `eattrs` (one `eqref` field: number mode never has element attributes) is worth it.
- 2026-10-11: test262 of the fourth build (`~/t262-t107d.log`): no changes; baseline copied back (identical). Field experiment started on a-03 (user: ok, wait for results): `~/t107-field.sh` (HEAD, HEAD + one unused `$array` field, T107; splay, knucleotide, deltablue, raytrace; 5 interleaved runs).
- 2026-10-11: field experiment (`~/t107-field.log`, median ms): splay 3,555 / 3,572 / 3,730 (HEAD / +field / T107), knucleotide 5,095 / 5,112 / 5,234, deltablue 3,707 / 3,712 / 3,776, raytrace 7,295 / 7,312 / 7,443: the field costs ≤ 0.5%; the rest (2–5%) is T107's code on generic arrays. Next: perf of HEAD vs T107 on splay and raytrace.
- 2026-10-11: perf of HEAD vs T107 (`~/t107-perf.log`): raytrace `new_array` 0.94 → 2.34% and `array_elems` 0.84%
  (new): its `initialize.apply(this, arguments)` argument lists (numbers) went through `new_array`'s check, into
  number mode, and straight back out (`array_elems`). splay: no array function grew; GC +~1% (the field, ≤ 0.5%),
  and `exportKeys` puts random keys (non-integers) into a number-mode array that the checks then read generically
  (`keys[i] >= keys[i + 1]`: a new box per read): the known cost of generic reads from number mode (follow-up).
  Change: every internal array (rest parameters, `arguments` lists, key/symbol lists, iteration entries, errors,
  `Array.prototype`) is created by `NewGeneric` (no check, never number mode); the checked `New` is left to the
  emitter's all-number literals. Fixture dirs (9) pass.
- 2026-10-11: fifth a-03 run (user: both, now): `~/t107e-bench.log`, then `~/t262-t107e.log`.
- 2026-10-11: fifth a-03 run (`~/t107e-bench.log`), T106 vs T107, median ms, outputs the same: spectralnorm
  867 → 480 (−44.6%), navier-stokes −13.3%, fannkuchredux −9.9%, crypto −8.2%, fasta −5.2%, nbody −2.3%, raytrace
  −0.6% (fixed), regexp −1.0%, richards −1.1%; left: splay +5.3%, deltablue +2.1%, knucleotide +2.0% (no overlap);
  the others within ±1%. deltablue (`OrderedCollection`: `new Array()`, push/pop of objects): `pop` called
  `array_elems` per call and `set_index`'s append path tested the state three times. Changes: `proto_pop` has a
  number-mode path (the last element boxed, `length` − 1; not sealed/frozen, not a hole) and reads `elems` directly
  for a generic array; `set_index` loads `nums` once and skips all mode logic when it's null (generic). Fixture: 3
  `pop` cases (sealed, number mode, a hole behind a prototype element) = node; fixture dirs pass.
- 2026-10-11: test262 of the fifth build (`~/t262-t107e.log`): no changes; baseline copied back (identical).
- 2026-10-11: sixth a-03 run (user: both, now): `~/t107f-bench.log`, then `~/t262-t107f.log`.
- 2026-10-11: sixth a-03 run (`~/t107f-bench.log`), T106 vs T107, median ms, outputs the same:
  | program | T106 ms | T107 ms | change |
  |---|---|---|---|
  | mandelbrot | 257 | 247 | -3.9% |
  | spectralnorm | 881 | 473 | -46.3% |
  | fannkuchredux | 8,231 | 7,529 | -8.5% |
  | nbody | 5,197 | 5,050 | -2.8% |
  | binarytrees | 9,190 | 9,233 | +0.5% |
  | fasta | 904 | 867 | -4.1% |
  | knucleotide | 5,105 | 5,226 | +2.4% |
  | pidigits | 1,004 | 997 | -0.7% |
  | regexredux | 3,018 | 3,003 | -0.5% |
  | revcomp | 515 | 504 | -2.1% |
  | octane-richards | 4,576 | 4,522 | -1.2% |
  | octane-deltablue | 3,724 | 3,770 | +1.2% |
  | octane-raytrace | 7,301 | 7,265 | -0.5% |
  | octane-navier-stokes | 3,544 | 3,058 | -13.7% |
  | octane-crypto | 5,770 | 5,326 | -7.7% |
  | octane-splay | 3,523 | 3,706 | +5.2% |
  | octane-earley-boyer | 10,197 | 10,252 | +0.5% |
  | octane-regexp | 22,074 | 21,891 | -0.8% |
  deltablue +1.2% (runs overlap now), knucleotide +2.4%, splay +5.2%. splay: its payloads `[0, …, 9]` (hundreds of
  thousands live) are hinted, and the hint was a reference to the shared `G_NUM_HINT` object: the copying collector
  forwards it for every hinted array in every collection (`forward` is 24% of splay). The field experiment's extra
  field was null, so it didn't show that. Change: the hint is an `i32` field (`array::F_HINT`, never traced);
  `nums` is non-null only in number mode again; `G_NUM_HINT` is gone; `is_num_mode` is a null test. Fixture dirs pass.
- 2026-10-11: test262 of the sixth build (`~/t262-t107f.log`): no changes; baseline copied back (identical).
- 2026-10-11: seventh a-03 run (user: both, now): `~/t107g-bench.log`, then `~/t262-t107g.log`.
- 2026-10-11: seventh a-03 run (`~/t107g-bench.log`), T106 vs T107, 5 interleaved runs, median ms, outputs the same:
  | program | T106 ms | T107 ms | change |
  |---|---|---|---|
  | mandelbrot | 244 | 251 | +2.9% |
  | spectralnorm | 874 | 443 | -49.3% |
  | fannkuchredux | 8,223 | 7,508 | -8.7% |
  | nbody | 5,200 | 5,137 | -1.2% |
  | binarytrees | 9,108 | 9,162 | +0.6% |
  | fasta | 911 | 864 | -5.2% |
  | knucleotide | 5,082 | 5,199 | +2.3% |
  | pidigits | 988 | 970 | -1.8% |
  | regexredux | 3,013 | 3,022 | +0.3% |
  | revcomp | 504 | 508 | +0.8% |
  | octane-richards | 4,579 | 4,532 | -1.0% |
  | octane-deltablue | 3,696 | 3,758 | +1.7% |
  | octane-raytrace | 7,278 | 7,258 | -0.3% |
  | octane-navier-stokes | 3,508 | 2,856 | -18.6% |
  | octane-crypto | 5,767 | 5,368 | -6.9% |
  | octane-splay | 3,513 | 3,535 | +0.6% |
  | octane-earley-boyer | 10,321 | 10,292 | -0.3% |
  | octane-regexp | 22,196 | 22,042 | -0.7% |
  splay fixed (+0.6%, overlapping). spectralnorm −49.3%, navier-stokes −18.6%, fannkuchredux −8.7%, crypto −6.9%,
  fasta −5.2%. Left: knucleotide +2.3% and deltablue +1.7% (no overlap); mandelbrot +2.9% overlaps. Recorded as a
  follow-up rather than chased further.
- 2026-10-11: the other fixture dirs (`commonjs/`, `globals/`, `library/`, `modules/`, `promises/`, `using/`, `wasi/`, `node/`, top-level) pass; `node/fetch.js`/`fs-streams.js` timed out under load (~6) and pass alone (56 / 41 s; HEAD 52 / 37 s earlier). `cargo test` (backend, lower, ir) ok.
- 2026-10-11: docs: `design.md` (D1, T107), `keywords.md` (`Array`), `performance.md` (numbers, next steps), test262 `HISTORY.md`, `tasks/README.md` (T107 done, F391 ticked, F400, F401), `CLAUDE.md`.
- 2026-10-11: test262 of the seventh (final) build (`~/t262-t107g.log`): no changes; baseline copied back (identical). Done.
