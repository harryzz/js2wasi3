#!/usr/bin/env python3
"""T129: verdicts of scripts/gate.sh's micro-benchmarks from paired runs.

Usage: scripts/micro-verdict.py <dir> <name> <runs> [new-label]
  Reads <dir>/<name>.base.run<i> and <dir>/<name>.<new-label>.run<i> (new-label default `next`; T128's gates wrote
  `new`), i = 1..runs: lines `<case> <ns>`. Prints a Markdown table.

Run i of both builds ran back to back, so each pair saw the same machine state. On the laptop whole runs speed up or
slow down together (per-run means 55-99 ns for one micro file), so the old rule, comparing per-case minimums, let one
lucky run make dozens of one build's cases look faster at once: byte-identical builds came out "slower" in 37-58
cases (F439). Here a case's change is the median of its paired differences new_i - base_i, and the verdict needs that
median beyond max(1 ns, 3% of base's median) and at least 5 of every 6 pairs on that side (a sign test: drift that
moves both builds of a pair alike cancels).
"""
import math
import statistics
import sys

d, n, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
new = sys.argv[4] if len(sys.argv) > 4 else "next"


def read(v, i):
    out = {}
    try:
        lines = open(f"{d}/{n}.{v}.run{i}").read().splitlines()
    except OSError:
        return out
    for line in lines:
        name, _, val = line.rstrip().rpartition(" ")
        try:
            out[name] = float(val)
        except ValueError:
            continue
    return out


pairs = [(read("base", i), read(new, i)) for i in range(1, runs + 1)]
cases = [k for k in pairs[0][0] if any(k in w for _, w in pairs)]
print("| case | base ns (median) | new ns (median) | change (median of pairs) | pairs + / − | verdict |")
print("|---|---|---|---|---|---|")
for k in cases:
    ps = [(b[k], w[k]) for b, w in pairs if k in b and k in w]
    if not ps:
        continue
    diffs = [w - b for b, w in ps]
    med = statistics.median(diffs)
    bmed = statistics.median(b for b, _ in ps)
    wmed = statistics.median(w for _, w in ps)
    thr = max(1.0, 0.03 * abs(bmed))
    need = math.ceil(5 * len(ps) / 6)
    up = sum(x > 0 for x in diffs)
    down = sum(x < 0 for x in diffs)
    verdict = "slower" if med > thr and up >= need else "faster" if -med > thr and down >= need else "same"
    print(f"| {k} | {bmed:.1f} | {wmed:.1f} | {med:+.1f} | {up} / {down} | {verdict} |")
