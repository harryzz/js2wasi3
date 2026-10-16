# T123: wasmtime 50.0.0-rc.1 measured against 49.0.3

Status: done | Depends on: T122 | a-03 (user, 2026-10-16: "lets make test/benchmark with 50-rc1")

## Goal
wasmtime 50.0.0-rc.1 (2026-10-05) gives each GC struct field and array element, the GC header fields and the array
length their own alias regions in Cranelift (#14351, #14353): loads of one field can't be invalidated by stores to
another, so repeated field reads can be dropped. Measure it on the same js2wasi3 output (`f9d6de0`, unchanged
since): the 18 programs (time, instructions), the micro-benchmarks, full test262. The pin stays 49.0.3; no change
to the compiler.

## Plan
- [x] Install rc.1 side by side on a-03 (`~/.local/bin/wasmtime-50.0.0-rc.1`; `~/wt50/wasmtime` for PATH).
- [x] A/B (`scratchpad/rt-ab.sh`: the bench run's `.wasm` files compiled by each wasmtime, runs alternate, minimum
  ms, one `perf stat` each, outputs = node; micro-benchmarks the same way); test262 with rc.1 on PATH.
- [x] Report; decide with the user (pin 50.0.0 when final?).

## Progress
- 2026-10-16: rc.1 (`wasmtime-v50.0.0-rc.1-x86_64-linux.tar.xz`, GitHub release via `gh`) installed on a-03 as
  `~/.local/bin/wasmtime-50.0.0-rc.1` (`50.0.0-rc.1 (f378efc43 2026-10-05)`); 49.0.3 stays `wasmtime`.
- 2026-10-16: first A/B start had a script bug (`read` merges consecutive tabs: an empty column shifted the fields;
  binarytrees' row was valid: −8.7% time, −2.33% instructions); stopped by PID, fixed with `cut`, restarted
  (`~/ab-wt50.log`), then test262 with rc.1 (`~/t262-wt50.log`).
- 2026-10-16: A/B (a-03, `out/ab-wt50-a03.log`, script `out/rt-ab.sh`; A = 49.0.3, B = 50.0.0-rc.1, same `.wasm`,
  5 alternating runs, minimum): all outputs = node. pidigits 1,040 → 765 ms (−26.4%, instructions −27.2%), splay
  −9.1% (−4.25%), binarytrees −7.2% (−2.33%), regexp −3.5% (−1.16%), fannkuchredux −3.4% (+0.10%), earley-boyer
  −0.2% (−1.11%), knucleotide −0.9% (−0.63%); regexredux +4.3% (−0.09%) and spectralnorm +2.4% (0.00%): placement;
  the rest within ±1.1% at ±0.3% instructions. Micro: 71 same, 7 faster, 12 slower (`typeof v` on six kinds +2 ns
  at 9–14 ns, `k in o [Map]` +4, `write miss plain` +2.5, regex 2k-op cases ± either way); instructions per file on
  the laptop: `primitives.js` −0.37%, `object-kinds.js` −0.06% (no added work: placement). test262 with rc.1
  (`out/t262-wt50.log`, 1,526 s): no changes since the baseline.
- 2026-10-16: user: close with the plan to pin 50.0.0 when final (F436). Task done.

## Notes / Result
- **Result (a-03, same `.wasm` from `f9d6de0`):** 50.0.0-rc.1 vs 49.0.3: pidigits −26.4% (instructions −27.2%: BigInt
  limb loops, fitting the new GC alias regions), splay −9.1% (−4.25%), binarytrees −7.2% (−2.33%), regexp −3.5%,
  fannkuchredux −3.4%; nothing slower at more instructions (regexredux +4.3%, spectralnorm +2.4% at equal
  instructions: placement); micro mostly same, the moves placement (fewer instructions per file); test262 no changes.
- **Decision (user, 2026-10-16):** keep 49.0.3 pinned while 50 is a release candidate; pin 50.0.0 when final as T122
  did (install on the laptop and a-03, probes, test262, compatibility page): F436.
- **Commands:** `out/rt-ab.sh ~/bench-t122 ~/.local/bin/wasmtime ~/.local/bin/wasmtime-50.0.0-rc.1
  ~/js2wasi3/tests/bench/micro 5` and `PATH=~/wt50:$PATH node scripts/test262.mjs subset` (a-03); `perf stat` per
  micro file on the laptop.
- **Next smallest step:** F436 when 50.0.0 is released; meanwhile choose among F420, F427, F404.

