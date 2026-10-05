# T85: cheap shape transitions (F362)

Status: done (2026-10-05) | Depends on: T83, T84 (profile) | Local (user: cloud sessions use the same limits)

## Goal
T83's `shape_child` (find or make the transition for an added key: a linked list of children, keys compared) is
5.7% self on the zigbee2mqtt replay (T84 perf, `t79/out-t83-perf`), about T83's whole loss there (23.9 s vs T82's
22.6 s). Make it cheap without changing results.

## Why (T84 perf; don't re-measure)
Callers of `shape_child` (via `add_shaped`): `define` 3.85% (`define_property` from `%ArrayIteratorPrototype%.next`
0.74% and `create_iter_result_object` 0.65%, `regexp_new` 0.47%, `make_arguments` 0.40%, `copy_props` 0.40%,
`ordinary_set` 0.29%, `new_function` 0.25%, `getOwnPropertyDescriptor` 0.18%); `append_named` 1.82% (`define_named`
1.58%: literal defines whose site missed). Every object starts at the root shape, so the root collects up to
`CHILD_MAX` (64) children: a first key deep in the list, or absent from a full list, scans all of them.

## Rule (user, 2026-10-05)
After this task, one replay on a-03: below T82's 22.6 s → keep T83 and go on to F363; 22.6 s or more → revert T83.

## Plan
1. Micro-benchmark `tasks/T85-files/transition-bench.js`: fill the root with many first keys, then hot paths
   (array spread, generator results, literals, `arguments`, RegExp objects), node vs before/after.
2. `shape_child`: move a found child to the front of its parent's list (hot transitions found first).
3. If still significant: a fixed precomputed shape for `{value, done}` (`create_iter_result_object`, the array
   iterator's `next`); a children hash for big fan-outs.
4. Tests: `scripts/diff.sh` on `builtins/object-shapes.js` and object/class/iterator fixtures, `wasm-tools validate`,
   a wasmtime smoke run, `cargo test -p js2wasi3-backend`.

## Acceptance
- [x] Micro-benchmark before/after; `shape_child` no longer a hot spot in it.
- [x] Fixtures pass, `cargo test`, validate, smoke run.
- [x] One replay on a-03 (ask first); the rule applied; docs and follow-ups.

## Notes / Result
- Cause: a large program fills the root shape's 64 transitions early; every later object with a new first key
  scanned all 64, found no room and went to dictionary mode (no shape, so no cache either).
- Change (`crates/backend/src/object.rs`, `runtime.rs`): `$shape` gets `table` (field 7, a `$vals`): past 8
  transitions they're hashed (`string_hash(k) + 31 * attrs`, linear probing, rebuilt at half full, `shape_put`);
  CHILD_MAX 64 → 4096 (the global budget of 65,536 shapes stays); a found transition in the short list moves to the
  front. Fixture `builtins/object-shapes.js` gained a T85 section.
- Micro-benchmark (laptop, min of 5, ms; node / T82 / T83 / T85): arguments 3/290/390/159, descriptors
  6/138/183/79, generator 6/190/275/103, literals 3/83/255/30, spread 2/32/25/18.
- Replay on a-03 (`t79/out-t85`): **22.63 s** (T82 22.59 s, T83 23.9 s; node 8.66 s), first minute 6.4 s, RSS
  777 MB (grows as T82's), 13,478 publishes, the same 9 error lines. The rule said "22.6 s or more → revert": a tie;
  the user chose to keep shapes and do F363 next, with a new rule (after F363 ≤ 21.5 s, else revert all shapes work).
- Not run: full test262 with T85 (T83's had 0 regressions; T85 only changes how transitions are found).
- Next: T86 (F363).

## Progress
- 2026-10-05: task started (local, F362). Read `shape_child`, `add_shaped`, `IterResult`.
- 2026-10-05: micro-benchmark `tasks/T85-files/transition-bench.js` (+ `bench.sh`, from T83's) reproduces it: T83
  slower than T82 on every runtime-built case, literals 3× (83 → 255 ms). Move-to-front alone: no change. Local perf
  on the literals case: `shape_child` 51% + `unshape`: the setup fills the root's 64 children (CHILD_MAX), so every
  later object with a new first key scans all 64, finds no room and goes to dictionary mode (no shape, no cache).
- 2026-10-05: fix: a shape hashes its transitions past 8 (`SH_TABLE`, a `$vals` table, linear probing on
  `string_hash(k) + 31 * attrs`, rebuilt at half full; new helper `shape_put`), CHILD_MAX 64 → 4096 (the global
  budget of 65,536 shapes stays); move-to-front kept for short lists. Bench (min of 5, ms; node / T82 / T83 / T85):
  arguments 3/290/390/159, descriptors 6/138/183/79, generator 6/190/275/103, literals 3/83/255/30, spread
  2/32/25/18; output = node's.
- 2026-10-05: tests: `cargo test -p js2wasi3-backend` ok; `scripts/diff.sh` objects/, classes/, generators/, arrays/,
  async/ and `builtins/object-shapes.js` (new T85 section: 600 objects with distinct first keys, same key with other
  attributes, literals after that) all pass; `wasm-tools validate` + wasmtime runs of the benches. T83's bench:
  unchanged vs T83 (no regression). Bench scripts: `env -u FORCE_COLOR node` (node's colors made "OUTPUT DIFFERS").
