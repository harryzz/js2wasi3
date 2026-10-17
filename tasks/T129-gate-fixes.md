# T129: gate fixes: micro run order (F439), instruction counts (F440)

Status: done (2026-10-17) | Depends on: T112, T113, T128 | Local (a-03 only to confirm, after asking)

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
- [x] F439: alternate the order per run (odd runs base→new, even runs new→base), keep minimum-of-runs. Check: a gate of
  HEAD against itself (`scripts/gate.sh HEAD`: every `.wasm` identical, so programs are skipped; micros must come out
  all `same`; before the fix, compare with the reversed-order evidence above).
- [x] F440: give both builds paths of equal length (e.g. `base/x.cwasm` and `new/x.cwasm`, or `x.base.cwasm` /
  `x.new0.cwasm`), the same `--dir` and argv; count instructions over several runs (the gate's `RUNS`, or 3) and
  report the minimum, plus the spread when it exceeds 0.5% (marked, so a reader knows the column is GC noise for that
  program). Check: binarytrees base vs base (same compiler) gives an `instr` change under 0.01%; raytrace shows its
  spread.
- [x] Update the usage comments in both scripts and `docs/design.md` "Speed work" (equal instructions = placement still
  holds; say how GC-heavy programs are read).
- [x] Laptop: one gate of HEAD vs HEAD (sanity) and nothing else; a-03 only if the user asks.

## Rules that apply
- Write a dated Progress line after every step; status `in progress` in `tasks/README.md` when starting.
- Only the standard tests and benchmarks; no zigbee2mqtt. Ask before a-03 runs and long runs.
- Never commit/push without asking each time; no attribution lines.
- At the end: Notes/Result, `tasks/README.md` status and follow-ups (next free F is F445), tick F439/F440, `CLAUDE.md`.

## State at handoff (2026-10-17)
- `main` = `10471c6` (T128), pushed. wasmtime 49.0.3. test262 baseline 28,123.

## Progress
- 2026-10-17: handoff written (user: "F439/F440 next up").
- 2026-10-17: started (user: "commit then next up"); status `in progress`. Found: `bench.sh -b`'s timing loop also
  always runs base then new (the programs' verdicts may share F439's bias). Plan: alternate both loops; equal-length
  names `x.base.cwasm` / `x.next.cwasm`; count instructions inside every timing run (`perf stat` counting mode, no
  extra runs) and report the minimum, marking a spread over 0.5%.

## Notes / Result
- 2026-10-17: implemented: `bench.sh -b` alternates the order per run, runs the new build as `x.next.cwasm` (hard link, same length as `x.base.cwasm`), counts instructions inside every timed run (`perf stat -o x.<v>.instr<i>`), reports the change of minimums plus `~x%` when a build spreads over 0.5% (gate.tsv gets an `instr` header); `gate.sh` micros: `x.base.*`/`x.next.*`, alternating order. Usage comments updated.
- 2026-10-17: test gate vs ebe6055, binarytrees + raytrace, 5 runs, per-run padded names (`out/gate-t129-test.log`): binarytrees instr −0.00% (base 58,769.92–58,769.93M, new 58,768.43–58,768.44M; before T129: +0.76%), raytrace −2.67% (spread < 0.01%: its ±10% earlier came from my ad-hoc runs with other paths/`--dir`, not run-to-run). Micros still mostly "slower" (56) with alternating order: next, HEAD vs HEAD (identical builds) to separate artefact from placement.
- 2026-10-17: HEAD vs HEAD (`out/gate-t129-self.log`, micro builds byte-identical): still 39 "slower"/7 "faster" with 5 alternating runs: base ran first in 3 pairs, next in 2, and the first of a pair is faster, so base got one more chance at a low minimum. Fix: an even number of runs (odd `-r` rounded up; gate default 6).
- 2026-10-17: HEAD vs HEAD with 6 runs (`out/gate-t129-self6.log`): still 37 "slower"; with a fresh copy of each
  `.cwasm` per run (new page-cache pages; `out/gate-t129-selfcp.log`): 58 "slower", 2 "faster". Per-run means
  (selfcp, primitives): run 1's first build (base) 47.1 ns, every other run 51.7–54.4 (object-kinds 79.0 vs
  81.6–88.3): the first run after the builds is fast (the laptop's boost after the compile step), and the minimum
  picks it, always for base. Not order within pairs, not the files. Fix: one untimed run of each build before the
  timed runs; hard links again (copies made no difference).
- 2026-10-17: the warm-up didn't help (`out/gate-t129-selfw.log`: per-run means 55–99 ns with no pattern by build or
  order; the laptop is idle inside WSL, load 0.9: the noise is outside the VM). Diagnosis: whole runs drift together,
  so the per-case minimum lets one lucky run make dozens of one build's cases faster at once; the 37–58 "slower" rows
  were that, not run order. Fix: `scripts/micro-verdict.py`: a case's change is the median of paired differences
  (new − base, each pair back to back), verdict when it exceeds max(1 ns, 3%) and ≥ 5 of 6 pairs agree (sign test).
  Replayed on the saved runs: HEAD vs HEAD 1/90, 4/90 (6 runs), 4/90 (copies), 18/90 (the very noisy warm-up run,
  10 slower/8 faster: random, not biased); T128 final 4/90 and its real effect kept (`add over proto default
  (plain)` −11.5 ns, 0 of 5 pairs against). Warm-up removed; alternation and even runs kept (balanced pairs).
- 2026-10-17: end-to-end HEAD vs HEAD with the paired rule (`out/gate-t129-final-self.log`): 18 of 90 verdicts on
  byte-identical builds (13 slower, 5 faster, several 6/0 or 5/1), during a much noisier period (every value ~30%
  above the earlier gates: `typeof v [plain]` 9 vs 5 ns; the disk was at 100% after a gate build failed with
  "No space left on device" and I deleted today's rebuildable `base-target` dirs, 3.5 GB). The paired rule cuts the
  false verdicts in normal conditions (replays: 1–4/90) but can't tell structured noise from an effect. Asked the
  user: keep the paired rule and confirm laptop micro verdicts on a-03 (T128: 4 false there), or add an A/A control
  (a second base run in each pair, its verdicts = the file's false-positive rate; micros +50% time).
- 2026-10-17: user: "why not run 1 on a-03?": HEAD vs HEAD with the new scripts on a-03 (quieter: T128 a-03 gate 4 false micro rows vs 41 on the laptop), to see if the paired rule alone suffices there.
- 2026-10-17: user: "Rerun the T128 gate on a-03 with the new scripts": queued after the self-gate: `scripts/gate.sh -o out/gate-t128-v2 ebe6055` on a-03 (tree = 5a68ca1 + new scripts; compiler = T128's).
- 2026-10-17: a-03 HEAD vs HEAD with the new scripts (`out/gate-t129-self-a03.log`): **0 of 93** micro verdicts on byte-identical builds (all 93 cases `same`). The paired rule alone is clean on a-03; the laptop's 18/90 was its noise. T128 gate rerun on a-03 started.
- 2026-10-17: user chose "find out why first" for the 3 micro slowdowns T128 shows on a-03 (`read miss proto` +2.5, `write miss plain` +1.5, `write Map own x` +2.8 ns, all 6/0): per-case instruction counts on a-03, base (ebe6055) vs T128 compiler.
- 2026-10-17: T128 gate rerun on a-03 with the new scripts (`out/gate-t128-v2-a03.log`, `.tsv`): raytrace −3.1%
  (instr −2.66%), pidigits −2.5%, mandelbrot −3.3%, navier-stokes −2.2% faster; fasta +3.9%, revcomp +3.4%,
  earley-boyer +2.1% "slower" at equal instructions (+0.00/+0.00/+0.06%: placement); binarytrees instr −0.00%.
  Micros: `add over proto default (plain)` −8.0 (0/6), `Object.keys(o).length [function]` −5.0 (0/6) faster;
  `read miss proto` +2.5, `write miss plain` +1.5, `write Map own x` +2.8 ns slower (all 6/0).
- 2026-10-17: per-case instruction counts (a-03, 10M ops each, minus a control loop; scratchpad `t129/*.js`):
  `write miss plain` 306.0 = 306.0 and `write Map own x` 323.0 = 323.0 instr/op (placement); `read miss proto`
  510.0 → 549.0 (+39). Cause: the micro's `plain` is the FIRST `P` built; its adds miss `layout_set`'s empty add
  cache and take the generic path, where base copies the keys (`define.layout_own`) and drops the shared index (a
  2-key linear scan), while T128 keeps the layout (`define.layout_next`), so the prototype read probes the shared
  hash index. With T128's fast path disabled the case is 510 again (laptop: 5,529.19M vs 5,919.20M). Through a
  second `P` instance (every later instance keeps its layout in both builds) base = T128: 5,921.44M vs 5,921.47M,
  549/op. So T128 adds no work; it made the first instance like the others. Found on the way: lookups through a
  layout object's shared index cost ~39 instructions more than a linear scan of its 2 keys (prototype reads that
  reach a layout object: F445).
- 2026-10-17: user, after the numbers: "more code and worse results"; geometric mean of the 18 programs on a-03:
  T128/base 0.995 (first gate), 0.998 (rerun). User: keep T128, F441 next. T129 closed.

**Done 2026-10-17.** Tooling only (no compiler change).
- What changed: `scripts/bench.sh -b`: pairs back to back with the order swapped every other pair, an even number of
  runs (odd `-r` rounded up), run i under names padded by i characters for both builds (`x.base_…_.cwasm`,
  `x.next_…_.cwasm`; argv carries the path), instructions counted in every timed run (`perf stat -o
  x.<v>.instr<i>`), `instr` = change of minimums plus `~x%` when a build spreads over 0.5%. `scripts/gate.sh`: the same
  for micros (default 6 runs), verdicts by the new `scripts/micro-verdict.py` (median of paired differences, ≥ 5 of
  6 pairs). `docs/design.md` "Speed work": how to read the gate.
- Results: binarytrees' false +0.76% instructions → −0.00% (counts repeat to 6 digits); byte-identical builds: micro
  verdicts 37–58 → 0 of 93 on a-03 (laptop: up to 18 in noisy periods: advisory there). The T128 rerun on a-03
  (`out/gate-t128-v2-a03.log`) found three consistent micro slowdowns: two at equal instructions (placement), one an
  artefact of the micro (`plain` is the first `P`, the one instance base builds without its layout; through a second
  instance base = T128, 549 instructions/read). Found: F445 (lookups through a layout's shared index cost ~39
  instructions more than a short linear scan).
- Commands: `scripts/gate.sh -o out/gate-t129-{test,self,self6,selfcp,selfw} …` (laptop), `scripts/gate.sh -o
  out/gate-t129-self HEAD fasta` and `-o out/gate-t128-v2 ebe6055` (a-03; logs in `out/`), per-case counts
  (scratchpad `t129/*.js`, a-03).
- Not done: a micro A/A control (not needed on a-03); the laptop's micro noise (outside WSL) stays.
- Next: F441 (T130).
