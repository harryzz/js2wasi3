# T108: element reads in `+` and comparisons without a box (F401)

Status: done | Depends on: T107 | Local

## Goal
T107 keeps arrays of numbers unboxed (`nums`, number mode). A read used as a number is already unboxed
(`jsval->f64(get(o, k))` → `get_num`, `o[k] += <number>` → `ElemNum`). But a read whose value goes to a generic
operation boxes it: `get` returns a `jsval`, so a non-integer element becomes a new `$number` on every read (before
T107 the stored box was returned, no allocation). Cases:
- `x[i] + y[i]`, `lastX + x[++k]` (`js_add` of two `jsval`s): navier-stokes `lin_solve`
  (`(x0[c] + a*(lastX + x[++c] + x[++l] + x[++n])) * invC`), spectralnorm-like sums;
- comparisons `a[i] < a[j]`, `keys[i] >= keys[i + 1]` (`js_less`…): splay's key checks, sorts;
- `f(a[i])` / `o.p = a[i]` (the box escapes: unavoidable, out of scope).

This is what T107 accepted (`tasks/T107-number-arrays.md`, Notes; `docs/design.md` T107 paragraph).

## Idea (to check first, not decided)
In the lowering, for a `+` or a comparison whose operand(s) are computed element reads (`o[k]`, non-constant key,
not `super`/private), read them with `ElemNum` (T107's intrinsic: the `f64` of a number-mode element, or the hole's
bits = "not one") in source order; if every such operand gave a number (and the other operand is an `f64` or also a
number), compute in `f64`; else fall back to the generic `get` + `js_add`/`js_less` for that operand **without
reading it twice** (`ElemNum` never runs JS code, so reading it with `get` after a failed `ElemNum` is the same
single observable read, as `Lowerer::elem_add_stmt` does). Order of evaluation must stay JS's: left operand fully
(including its `get`) before the right one.
- Result type: a comparison is a `bool` either way (no box at all). A `+` is `f64` on the fast path but a `jsval`
  in general (strings): it only pays when the consumer is numeric (`* invC`, a comparison, `set_num`), so emit the
  two-path form only there, or produce `f64` with a generic branch that converts (`ToNumber(js_add(…))` is exact
  for a numeric consumer).
- The chain `lastX + x[a] + x[b] + x[c]` (left-assoc) under `a * (…)`: the whole chain under one numeric consumer
  can be one fast path (all operands numbers → `f64` adds; else the generic chain from the first non-number on).

## Read first
- `tasks/T107-number-arrays.md` (Design, Notes, the `+=` step), `docs/design.md` (D1, T107 paragraph).
- `crates/lower/src/lib.rs`: `binary` (~4699), `elem_add_stmt` (~7061, the pattern to follow), `combine` (~7441).
- `crates/backend/src/object.rs`: `get` (~2920), `get_num` (~1370), `elem_num` (~1533); `crates/backend/src/lib.rs`:
  the `ElemNum`/`F64IsHole` emission and the `get_num`/`set_num` peepholes (search `T107`).
- `crates/ir/src/lib.rs`: `Intrinsic::ElemNum`, `F64IsHole`.

## Plan
- [x] Measure first (a-03, ask): perf of navier-stokes, splay, crypto, nbody with T107: how much is `box_number` /
  allocation from generic element reads now; IR of the hot loops (`js2wasi3 ir`) to list the exact shapes. If
  < ~3% anywhere, record that in F401 and stop.
- [x] (dropped after measuring: ≤ 1.1% anywhere, F402) Comparisons first (smallest, no result-type question): `o[k] <op> x` / `x <op> o[k]` / both, for
  `< <= > >=` (and `=== !==` if they show up).
- [x] `+` under a numeric consumer (the navier-stokes chain).
- [x] Fixture `tests/fixtures/arrays/elem-reads.js`: order of evaluation (a right operand that changes the array,
  getters on elements counted, `valueOf` on an object element, string elements, holes behind a prototype element,
  `NaN`/`-0`, typed arrays, plain objects with numeric keys) = node.
- [x] `scripts/diff.sh` on the touched dirs; test262 (full run on a-03, ask); a-03 benchmarks (ask): nothing
  slower beyond noise (F399: ±5% layout effects; compare hot IR when a program without element reads moves).

## Acceptance
- Every fixture = node; test262 0 regressions; navier-stokes / splay faster; nothing slower beyond noise.

## Notes / Result
- Measured first (a-03 perf of the T107 builds): navier-stokes ~79% in generic element reads + `js_add`
  (`get` 25%, `box_number` 24%, `to_number` 20%, `js_add` 10%); comparisons ≤ 1.1% anywhere (dropped, F402);
  crypto's and nbody's boxing isn't from element reads (F404; object properties, F323); splay is GC.
- Changed: `Intrinsic::NumOrHole` (`crates/ir`, inline in `crates/backend/src/lib.rs`: an i31's / `$number`'s
  value, else HOLE's bits); `Lowerer::num_sum` + `sum_operand` (`crates/lower/src/lib.rs`) replace `number_of` for
  the operands of `-`/`*`/`/`/`%`/`**`/relational operators when the operand is a `js_add` tree with a computed
  element read: operands in source order into `f64` locals, `f64.add` when both are numbers, else `js_add` at that
  point and ToNumber at the end. A lone element read is left to `get_num`.
- Results (a-03, T107 vs T108, median of 5): **navier-stokes 2,859 → 1,513 ms (−47.1%)**, crypto +0.2%; the other
  16 benchmarks' `.wasm` are byte-identical (±1.9%, noise). All outputs same. Full test262: no changes (28,105).
  `scripts/diff.sh tests/fixtures/arrays`: 8/8.
- Unsupported / left: comparisons (F402); a `jsval` local holding the sum (`lastX`: one box per iteration, F403);
  a `+` whose value isn't consumed as a number (`o.p = x[i] + y[i]`, `f(x[i] + 1)`) is unchanged.
- Next smallest step: F403 (`lastX` as `f64`) for navier-stokes' last box per iteration.

## Progress
- 2026-10-11: task created as the handoff after T107 (user: "write handoff for F401 for new session"). Not
  started. State at handoff: `main` at 6921634 (T107), test262 baseline 28,105 of 28,984; a-03 has
  `~/js2wasi3-t106` and `~/js2wasi3-t107` compilers and the bench script `~/t107-bench.sh` (copy and rename for
  T108: head = T107, new = T108; `~/t107-all.sh` chains benchmarks and test262; run detached with
  `setsid nohup bash -l …`, a non-login shell lacks `wasmtime`).
- 2026-10-11: started. Local IR of navier-stokes `lin_solve` (`js2wasi3 ir`): the inner loop is
  `lastX = f64->jsval(... f64.mul(jsval->f64($js_add($get(x0, k), f64->jsval(f64.mul(a, jsval->f64($js_add($js_add($js_add(lastX, $get(x, ..)), $get(x, ..)), $get(x, ..))))))), invC) ...)`:
  4 boxing `$get`s, 4 `$js_add`s, two `jsval->f64` after `$js_add`, and `lastX` is a `jsval` local (its first
  write is a `$get`), so `f64->jsval` of the stored value boxes once more per iteration. Every `+` here is under a
  numeric consumer (`f64.mul`). Next: perf on a-03 (asked).
- 2026-10-11: measured on a-03 (user OK): `~/t108-prof.sh`, perf of the T107 builds in `~/t107-bench`, profiles in
  `~/t108-prof`. Self time: **navier-stokes** `get` 25.2%, `box_number` 24.3% (20.1% from `lin_solve`, 1.9%
  `project`, 1.5% `advect`), `to_number` 19.7%, `js_add` 9.7%, `set_num` 5.8%, `get_num` 3.2%, `js_less` 1.1%:
  ~79% is the generic element read + `+` path. **crypto** `box_number` 21.9% (18.6% from `am3`), but not from
  element reads: `am3`'s elements are i31; the boxes are `f64->jsval` of the `jsval` locals/params `l`, `c`, `n`,
  `i`, `j` (`l = $js_add($js_add(f64->jsval(..), $get(w_array, j)), c)`): a local-type question, not F401.
  **nbody** `box_number` 15.6%, all from `advance`'s object properties (`bodies[i].vx`), not arrays: out of scope.
  **splay** GC-bound (copying collector ~72%), `js_less` 0.7%, `box_number` 0.2%: nothing for T108.
  Decision: comparisons are < 3% everywhere (max 1.1%), so they're dropped from this task (follow-up). The `+` chain under a numeric
  consumer is the whole task (navier-stokes).
- 2026-10-11: implemented. IR: `Intrinsic::NumOrHole` (`jsval -> f64`: an i31's or `$number`'s value, else
  HOLE's bits; inline in the backend, no call). Lowering: `Lowerer::num_sum` replaces `number_of` for the operands
  of arithmetic/relational operators (`binary`, the `f64` path): a `js_add` tree with a computed element read
  (`GetProp` with a non-string key) becomes a `seq` that evaluates each operand in source order into an `f64` local
  (element: `ElemNum`, on HOLE `get` + `NumOrHole`; `f64->jsval(e)`: `e`; anything else: `NumOrHole`) and each `+`
  as `f64.add` when both are numbers, else `js_add` of the operands (boxed only there) at that same point. HOLE is a
  signalling NaN, so no `f64.add` result has its bits. `lin_solve`'s inner loop now has no `js_add`/`get` on the
  fast path (`lastX` stays a `jsval` local: one box per iteration left).
  Fixture `tests/fixtures/arrays/elem-reads.js` (number mode, i31 arrays, strings, holes behind a prototype
  element, NaN/-0/Infinity, getters, valueOf order, a right operand that changes the array, a chain whose first
  `+` converts before the next operand, typed arrays, plain objects, strings, null receiver, comparisons):
  `scripts/diff.sh tests/fixtures/arrays` 8/8 pass; navier-stokes `wasm-tools validate` ok; local
  `scripts/bench.sh -r 3 octane-navier-stokes`: SAME output, 2.33 s (node 0.21). Next: a-03 benchmarks + test262 (ask).
- 2026-10-11: a-03 run started (user OK): tree rsynced, `~/js2wasi3-t108` built; `~/t108-all.sh` = `~/t108-bench.sh` (T107 vs T108, 18 benchmarks, 5 interleaved rounds → `~/t108-bench.log`) then full test262 `--update` → `~/t262-t108.log` (rsync `tests/test262/baseline.json` back after).
- 2026-10-11: first a-03 run showed mandelbrot +3.5%, spectralnorm +2.1%: `num_sum` also took a lone element read
  (`a[i] * m`), which replaced T107's `get_num` peephole. Fixed: only a `js_add` tree is rewritten (now only
  crypto (2 sites) and navier-stokes (8) of the 18 benchmarks change). Run stopped, `scripts/diff.sh
  tests/fixtures/arrays` 8/8 again, rsynced, rebuilt, `~/t108-all.sh` restarted.
- 2026-10-11: a-03 benchmarks (`~/t108-bench.log`, T107 vs T108, median of 5 interleaved, ms): **navier-stokes
  2,859 → 1,513 (−47.1%)**, crypto 5,317 → 5,325 (+0.2%); only these two `.wasm` differ from T107 (the other 16
  are byte-identical: mandelbrot +1.2%, spectralnorm +1.8%, regexredux +1.9%, revcomp +1.2% are noise; the rest
  −0.7…+0.9%). All outputs same. Full test262 running.
- 2026-10-11: full test262 on a-03 (`~/t262-t108.log`, 1,282 s): no changes since the baseline (28,105 pass). Docs updated (design.md, performance.md, HISTORY.md, tasks/README.md: F401 ticked, F402–F404 added). Done.
