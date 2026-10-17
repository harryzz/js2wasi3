# T129: gate fixes: micro run order (F439), instruction counts (F440)

Status: not started (handoff, 2026-10-17) | Depends on: T112, T113, T128 | Local (a-03 only to confirm, after asking)

## Goal
`scripts/gate.sh` is the one check before every speed commit (`docs/design.md` "Speed work"). T128 found two artefacts
in it. Both made changes look slower when they weren't. This task is a **tooling** task: no compiler change, no `.wasm`
change.
- **F439, micro run order.** The micro loop (`scripts/gate.sh`, "Micro-benchmarks") always runs `base` then `new` in
  each of the `RUNS` pairs. On the laptop the second of each pair is slower: T128's four gates each showed 20–41 micro
  cases "slower" by 2–11 ns in code T128 doesn't touch (`typeof`, `==`, `k in o`), at equal instructions. With the
  order reversed (5 runs of `primitives`), new came out "faster" in 19 of 30 cases (`out/gate-t128-final/micro`, T128
  Progress 2026-10-17).
- **F440, the `instr` column is one run and depends on incidental heap layout.** `scripts/bench.sh -b` takes one
  `perf stat -e instructions:u` run per build. For GC-heavy programs the count depends on where wasmtime's copying
  collector runs:
  - the same binarytrees `.cwasm` bytes gave 58,768.5M at one path and 59,218.0M at another, because argv carries
    the path (`x.base.cwasm` vs `x.cwasm` differ in length). The base build happened not to move. With
    `-O gc-heap-initial-size=256M`, base 29,349.93M = new 29,349.77M at both path lengths;
  - raytrace varies ±10% between identical runs of one `.cwasm` (47.7G–56.8G).

  The column is the gate's tool for telling added work from code placement (T113), so it must be repeatable.

## Read first
- `scripts/gate.sh` (84 lines; micro loop ~lines 44–60 and its Python summary), `scripts/bench.sh` (`-b` mode: the
  build names `$name.cwasm` / `$name.base.cwasm`, the `perf stat` block ~lines 156–164).
- `docs/design.md` "Speed work" (what the gate's verdicts mean), T128 Progress 2026-10-17 (the measurements above).

## Plan (suggested)
- [ ] F439: alternate the order per run (odd runs base→new, even runs new→base), keep minimum-of-runs. Check: a gate of
  HEAD against itself (`scripts/gate.sh HEAD`: every `.wasm` identical, so programs are skipped; micros must come out
  all `same`; before the fix, compare with the reversed-order evidence above).
- [ ] F440: give both builds paths of equal length (e.g. `base/x.cwasm` and `new/x.cwasm`, or `x.base.cwasm` /
  `x.new0.cwasm`), the same `--dir` and argv; count instructions over several runs (the gate's `RUNS`, or 3) and
  report the minimum, plus the spread when it exceeds 0.5% (marked, so a reader knows the column is GC noise for that
  program). Check: binarytrees base vs base (same compiler) gives an `instr` change under 0.01%; raytrace shows its
  spread.
- [ ] Update the usage comments in both scripts and `docs/design.md` "Speed work" (equal instructions = placement still
  holds; say how GC-heavy programs are read).
- [ ] Laptop: one gate of HEAD vs HEAD (sanity) and nothing else; a-03 only if the user asks.

## Rules that apply
- Write a dated Progress line after every step; status `in progress` in `tasks/README.md` when starting.
- Only the standard tests and benchmarks; no zigbee2mqtt. Ask before a-03 runs and long runs.
- Never commit/push without asking each time; no attribution lines.
- At the end: Notes/Result, `tasks/README.md` status and follow-ups (next free F is F445), tick F439/F440, `CLAUDE.md`.

## State at handoff (2026-10-17)
- `main` = `10471c6` (T128), pushed. wasmtime 49.0.3. test262 baseline 28,123.

## Progress
- 2026-10-17: handoff written (user: "F439/F440 next up").

## Notes / Result
