# T108: element reads in `+` and comparisons without a box (F401)

Status: todo | Depends on: T107 | Local

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
- [ ] Measure first (a-03, ask): perf of navier-stokes, splay, crypto, nbody with T107: how much is `box_number` /
  allocation from generic element reads now; IR of the hot loops (`js2wasi3 ir`) to list the exact shapes. If
  < ~3% anywhere, record that in F401 and stop.
- [ ] Comparisons first (smallest, no result-type question): `o[k] <op> x` / `x <op> o[k]` / both, for
  `< <= > >=` (and `=== !==` if they show up).
- [ ] `+` under a numeric consumer (the navier-stokes chain).
- [ ] Fixture `tests/fixtures/arrays/elem-reads.js`: order of evaluation (a right operand that changes the array,
  getters on elements counted, `valueOf` on an object element, string elements, holes behind a prototype element,
  `NaN`/`-0`, typed arrays, plain objects with numeric keys) = node.
- [ ] `scripts/diff.sh` on the touched dirs; test262 (full run on a-03, ask); a-03 benchmarks (ask): nothing
  slower beyond noise (F399: ±5% layout effects; compare hot IR when a program without element reads moves).

## Acceptance
- Every fixture = node; test262 0 regressions; navier-stokes / splay faster; nothing slower beyond noise.

## Notes / Result

## Progress
- 2026-10-11: task created as the handoff after T107 (user: "write handoff for F401 for new session"). Not
  started. State at handoff: `main` at 6921634 (T107), test262 baseline 28,105 of 28,984; a-03 has
  `~/js2wasi3-t106` and `~/js2wasi3-t107` compilers and the bench script `~/t107-bench.sh` (copy and rename for
  T108: head = T107, new = T108; `~/t107-all.sh` chains benchmarks and test262; run detached with
  `setsid nohup bash -l …`, a non-login shell lacks `wasmtime`).
