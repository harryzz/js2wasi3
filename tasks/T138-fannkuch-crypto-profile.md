# T138: profile fannkuchredux and crypto (where our generated code loses to V8)

Status: done (2026-10-20) | Depends on: T134, T137 | Local (+ a-03 only for gates, after asking)

## Goal
On node's V8 our components run fannkuchredux 15.9× and crypto 10.8× node's JS (`out/v8bench-t137-a03.md`), the two
largest gaps that are our code and not wasmtime's collector. Find what costs the time (counts with `build --count`,
perf profiles by helper) and report the biggest items. No code changes in this task: each fix is its own task.

## Read first
- `docs/root-causes.md`, `docs/design.md` "Speed work" (split number slots, element reads and writes).
- `tasks/T134-typed-function-versions.md` (Notes / Result), `tasks/T136-element-copies.md` (fannkuchredux's copies
  were never boxed; 177M generic write hits with `i31`s).
- `scripts/profile.sh`, `scripts/profile-compare.py`, `scripts/bench.sh -c`.

## Plan
- [x] Counts (`build --count`) for fannkuchredux and crypto: which helpers run how often.
- [x] perf profile by helper (laptop; WSL perf by full path) and the hottest compiled functions.
- [x] Report the biggest costs and propose the smallest next task for each (no code).

## Notes / Result
Laptop, `out/bench-t138/` (normal build), `out/prof-t138/` (perf by helper), `out/count-t138/` (`--count`). No code.
- **fannkuchredux** (3.78 s, 12.2× node on wasmtime): 93.2% of samples are in the compiled `fannkuch` itself,
  5.1% `box_number`, 1.4% `to_boolean`; every helper count is 0 except 177,222,102 element-write hits.
  The listing (`wasm-tools print` of `build --core`) shows why one array access is long:
  1. `i`, `k` and the other counters are `f64`; every element access converts the key `f64 → i32 → f64` and compares
     (`trunc_sat`, `convert`, `f64.ne`).
  2. The receiver (`perm`, `perm1`, `count`: `const` locals holding array literals) is tested and cast on every access.
  3. A read of a generic-store element is `array.get`, `ref.test i31`, `ref.cast`, `i31.get_s`, `convert`, then a NaN-tag
     test for a miss: about 37 Wasm instructions on the hit path.
  4. A write tests length, number-mode store, value kind, a second store, frozen/sealed bits, and that the slot isn't
     a hole (nine tests) before `array.set`.
  5. A number held in a split slot (`let t = perm[i]`) is boxed with a call to `box_number` when it is written back
     into a generic store (`perm[k-i] = t`): the `i31 → f64 → i31` round trip is the 5.1%.
- **octane-crypto** (4.08 s, 12.2× node): 68.2% `am3` (the big-number multiply-add loop), `get_named` 11.1%,
  `find_own_named` 4.0%, `montReduce` 3.0%, `arg_get` 2.7%, `bnpSquareTo` 2.4%, `box_number` 2.0%.
  - `am3` is a method assigned on the prototype: its `this`, `x`, `w`, `c`, `n` are untyped, so `this.array` and
    `w.array` are `get_named` calls (13,295,124 guess hits, 2,817,685 prototype-or-absent) and every `&`, `>>`, `<<`
    converts `f64 → i32` with a range test (18 `to_int32` calls in its code); the split parameter `n` tests its
    null tag twice per `--n >= 0`. 35,517,342 element-write hits.
  - `arg_get` and `am3$code`: the call through the code table.
- **Common cause:** integers are `f64` everywhere. Every `i` index, every `&`/`>>` result and every element read
  converts between `f64`, `i32` and `i31`; V8 keeps these as int32 (Smi) values with overflow checks.
- **Smallest next tasks (proposed, none started):** (a) write an int32-valued `f64` into a generic store inline
  (`i31.new`) instead of calling `box_number` (fannkuchredux 5.1%); (b) `i32` split slots: locals whose every
  assignment is an int32 expression (bit ops, `|0`, small counters) stay `i32` (both programs); (c) skip the
  receiver test/cast and half of the write guards for `const` locals the analysis knows are arrays; (d) types for
  methods assigned on prototypes so `this.array` is a typed read (crypto `get_named` 15%).

## Progress
- 2026-10-20: started (user: "fannkuchredux and crypto profile first").
- 2026-10-20: counts (`out/count-t138/`), perf by helper (`out/prof-t138/`) and the Wasm listings of `fannkuch` and `am3`; findings in Notes / Result. Done.
