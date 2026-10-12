# T112: root-cause map and hit/miss counts before the next speed fix

Status: done | Depends on: T110 (a52761d) | Local (+ a-03 only after asking)

User, 2026-10-12: all data and comparisons only from the standard tests and benchmarks (fixtures, test262, the 18
programs, micro-benchmarks); nothing else is used or read.

## Goal
Stop the fix-one-break-another cycle of T106–T110 (user, 2026-10-12: "from 5 days we fix one then other broke …
this is not right way"; "write it as T112 and do it first"). The history:

| task | gain | what it broke |
|---|---|---|
| T106 | spectralnorm −32% | navier-stokes +5% (F399) |
| T107 | number arrays | knucleotide +2.3%, deltablue +1.7% (F400) |
| T108 | navier-stokes −47% | mandelbrot +3.5% in the first version (patched) |
| T109 | splay −27%, binarytrees −26% | raytrace +3% (patched with a `new X` name filter) |
| T110 | nbody −34%, richards −27% | knucleotide +7.2% (F409) |

Why it cycles: (1) each change is a compile-time guess (a key name, a key list, a value type) whose **miss** costs
more than the old path, and the miss cost was never measured before coding; (2) symptoms get patched (a name
filter, nearly a name list), so the same pattern breaks the next program; (3) several problems share root causes
that keep being worked around (wasmtime's `is_subtype` on `ref.cast (ref $object)`, F376: behind T92, T93, T96,
T109's design, F409; number boxing: F323, F403, F404, F406).

This task changes no speed code. It produces the map and the tools the next fixes are judged with.

## Plan
- [x] 1. **Root-cause map** (`docs/root-causes.md`): every open follow-up in `tasks/README.md` grouped by root
  cause (`is_subtype` casts, number boxing, property access, GC, code layout / noise, others), with which
  follow-ups each root blocks or makes risky, the programs it costs (from existing profiles), and an order of
  attack by how much each root unblocks.
- [x] 2. **Counting build**: a compiler option (`build --count`) that puts a hit and a miss counter on every fast
  path with a guard (T74/T105 `G_IC` guesses, T110 inline slots, T107 number-mode paths, T90 add path, T109
  `layout_set`/`layout_define`) and prints the totals per kind at exit. Fixture: counts on a small program
  match a hand count. No effect on code built without the option.
- [x] 3. **Counts** for the 18 benchmarks (laptop, small sizes) and the fixtures: hit/miss per fast-path kind,
  recorded in the map.
- [x] 4. **Miss-cost rule and micro-benchmarks**: `tests/bench/micro/` (or `scripts/`): for each fast path, its hit
  and its miss case (e.g. T110: a plain object and a `Map` at a layout-key site) vs the path without it. The
  rule (in `docs/design.md`): a fast path's miss may not cost more than the old path beyond noise.
- [x] 5. **One gate** (`scripts/gate.sh` or a documented procedure): all 18 benchmarks (+ micro-benchmarks of the
  touched fast paths) against the previous commit before a speed commit; anything slower beyond noise = the design changes, not a patch.
- [x] 6. From 1–4: the next task to do (likely the `is_subtype` root: a cheap plain-object test, which F409 and
  T111 need), written as a task file. T110 stays paused (F409), T111 paused until then.

## Acceptance
- The map covers every open follow-up; each speed follow-up has a root cause and the programs it costs.
- `build --count` works on all 18 benchmarks; counts recorded; output of the counted builds = node.
- Micro-benchmarks show hit and miss cost for every fast path listed in 2.
- The user agrees on the next task from the map.

## Notes / Result
**Done 2026-10-12** (the user agreed on T113 as next). No speed code changed.
- `docs/root-causes.md`: all 264 open follow-ups by root cause (R1 `is_subtype` on `$object` subtypes, R2 number
  boxing, R3 property access, R4 calls/argument arrays, R5 GC, R6 regex VM, R7 strings, R8 noise/code layout, R9
  compile speed/size; the rest by area), the T106–T110 regressions mapped to roots, the counts of the 18 programs.
- `build --count` (24 counters: `get_named`, `set_named`, T110 inline reads/writes with miss kinds, `layout_set`,
  `layout_define`, `define`), `scripts/bench.sh -c` (counts.tsv, stdout checked against node).
- `tests/bench/micro/property-paths.js`: hit and miss of each property path.
- `scripts/gate.sh <base-ref>` and `scripts/bench.sh -b <compiler>`: base vs new on the 18 programs and the
  micro-benchmarks, verdicts on minimum times (2%; micro 1 ns and 3%).
- Rules in `docs/design.md`, "Speed work".
- Findings: T110's every miss kind costs more than the old path (plain +6 ns, prototype +7.5, non-object +18,
  Map method +31); counts: knucleotide 17.5M subtype misses, splay 66% plain misses, raytrace 31%.
- Commands: `scripts/bench.sh -c -o out/count` (laptop, 2m43s, all 18 = node); micro T109 vs T110 (5 alternating
  runs); `scripts/gate.sh -r 3 e088f40 nbody knucleotide` (trial); `scripts/diff.sh` objects 16/16, classes 12/12,
  arrays 8/8, commonjs 15/15, modules 14/14, builtins 73/73 (1 skipped), node 42/42 (one flaky failure on the
  first run), async 15/15, generators 4/4, syntax 10/10 (1 skipped).
- Next: T113 (R1: a plain-object test without `is_subtype`), proposed, waiting for the user.

## Progress
- 2026-10-12: task created (user: "write it as T112 and do it first"). T111 paused before implementation (JSC
  cross-check and design options in its Progress log).
- 2026-10-12: user: "do not ever run zigbee2mqtt for test and measurement until i ask for this. use standart tests
  and benchmarks!": plan steps 3 and 5 changed to the fixtures, the 18 benchmarks and micro-benchmarks.
- 2026-10-12: step 1 done: `docs/root-causes.md`. All 264 open follow-ups placed (checked by script: every open id
  is in the map; F321/F363/F347, which are closed, are mentioned as closed only). Roots: R1 `is_subtype` on
  `$object` subtypes, R2 number boxing, R3 property access, R4 calls/argument arrays, R5 GC, R6 regex VM, R7
  strings, R8 noise/code layout, R9 compile speed/size; the rest correctness/features by area. Order of attack:
  R1 first (it blocks T110's inline sites and any inline object path). The T106–T110 regressions mapped to roots.
  Sources only the 18 benchmarks' profiles and per-task results. Next: step 2, `build --count`.
- 2026-10-12: step 2 done: `build --count` (`ir::Module::count`; `runtime::G_COUNTS`, a `$nums` array made on the
  first count; `runtime::COUNTS` = 24 named counters; `runtime::count`, `is_object_subtype`; `host::Helper::Counts`
  prints `count <name> <n>` to stderr when the event loop ends). Counted: `get_named` (guess hit / own / prototype
  or absent / not an object), `set_named` (guess hit / own / T90 add / other / not an object), T110 inline reads
  and writes (hit / miss: not an object, `$object` subtype, plain object), `layout_set` (add / overwrite /
  fallback), `layout_define` (fast / fallback), `define` (shared index kept / dropped). Without `--count` the
  output is unchanged (every `count` call is a no-op). Hand check on a small program (scratchpad `cnt.js`): every
  counter as expected (a Map with its own `x` is an inline *hit*, reached through `is_subtype`).
  `scripts/bench.sh -c` builds with `--count`, runs each program once, checks stdout against node's and writes
  `<outdir>/counts.tsv`.
- 2026-10-12: step 3: counts of the 18 benchmarks (laptop, small sizes, `scripts/bench.sh -c -o out/count`,
  2m43s; all 18 stdout = node). Findings (table in `docs/root-causes.md`, "Counts"):
  - knucleotide: 17.5M inline-read misses on a `$object` subtype (the Map `get`/`set`: F409 confirmed by count),
    8.8M `get_named` on non-objects (string methods).
  - Inline misses on plain objects (each pays the inline check and then the call): splay 1.6M of 2.4M reads
    (66%) and 840k of 840k writes; raytrace 1.9M of 6.1M reads (31%) and 1.2M of 1.3M writes; deltablue 5.1M of
    29M reads (18%), 198k writes; richards 952k of 65M reads.
  - Not touched by layouts: richards 16.6M and deltablue 24.8M `get_named` prototype-or-absent (method reads);
    raytrace 41M `get_named` guess hits and 11.7M `set_named` other (fields set in `initialize`, F407).
  - `layout_set.overwrite` 0 and `define.shared_dropped` 0 everywhere: those paths never ran.
- 2026-10-12: step 4: `tests/bench/micro/property-paths.js` (hit and miss of each property path, ns per operation
  with the loop subtracted, best of 5 inside a run). T109 (`e088f40` code) vs T110 (`a52761d`), laptop, 5
  alternating runs, minimum per case:

  | case | T109 ns | T110 ns | Δ ns |
  |---|---|---|---|
  | read hit (plain, layout slot) | 27.5 | 14.5 | −13.0 |
  | read miss plain (other slot) | 28.0 | 34.0 | **+6.0** |
  | read miss proto (inherited) | 45.0 | 52.5 | **+7.5** |
  | read Map own `x` (subtype) | 25.0 | 26.5 | +1.5 |
  | read array own `x` (subtype) | 25.0 | 24.5 | −0.5 |
  | read string `.x` (not an object) | 40.5 | 58.5 | **+18.0** |
  | call Map `get` (subtype, key `get`) | 95.5 | 126.5 | **+31.0** |
  | write hit (plain, layout slot) | 25.0 | 12.0 | −13.0 |
  | write miss plain (other slot) | 25.5 | 31.0 | **+5.5** |
  | write Map own `x` (subtype) | 25.0 | 21.5 | −3.5 |
  | `new P` (`layout_set` adds) | 169.5 | 188.0 | +18.5 (T110 didn't change this path: R8?) |
  | literal `{x, y}` (`layout_define`) | 94.0 | 91.5 | −2.5 |

  T110 breaks the miss-cost rule on every miss kind: a plain-object miss +6 ns, a prototype hit +7.5, a
  non-object +18 (a string is a struct: the cast to the non-final `$object` calls `is_subtype`), a Map method +31.
  With the step-3 counts this explains the benchmarks: knucleotide 17.5M subtype misses, splay 66% plain misses
  (+0.1% net), raytrace 31% misses. `new P` +18.5 on a path T110 didn't touch: to check for layout noise (R8)
  in the gate (step 5).
- 2026-10-12: step 5: `scripts/gate.sh [-r runs] <base-ref> [name...]`: builds the base compiler in a detached
  worktree (`out/gate/base-src`, own target dir), then `scripts/bench.sh -b <base>` (new option: identical `.wasm`
  = no change, not run; else base and new alternate) and every `tests/bench/micro/*.js` under both. Verdicts on
  the minimum times (noise only adds time): programs `slower`/`faster` past 2%, micro cases past 1 ns and 3%.
  First trial (laptop, 3 runs, base `e088f40` = T109, nbody + knucleotide): with "no overlap" as the verdict
  knucleotide's real +6.7% read `same` (one base outlier, 6206 ms): changed to the minimum rule. The rules are in
  `docs/design.md`, "Speed work". Note: T112's `G_COUNTS` global shifts every later global index, so every
  `.wasm` differs from pre-T112 builds; `identical` works between post-T112 commits.
