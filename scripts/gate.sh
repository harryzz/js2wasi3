#!/usr/bin/env bash
# T112: the gate before a speed commit: the working tree's compiler against a base commit's, on the 18 programs of
# tests/bench and the micro-benchmarks of tests/bench/micro.
#
# Usage: scripts/gate.sh [-r runs] [-o outdir] <base-ref> [name...]
#   base-ref: the commit to compare against (usually HEAD before the change, or the last speed commit).
#   name: programs to run (default: all 18; see scripts/bench.sh).
#   -r: runs of each build per program and per micro-benchmark (default 6; an odd number is rounded up), in pairs
#       of base and new back to back, the order swapped every other pair (T129).
#   -o: work directory (default out/gate).
# Programs: `scripts/bench.sh -b` (identical `.wasm` = no change, not run; else `slower`/`faster` when the minimum
# times differ by more than 2%; outputs must match node's). Micro-benchmarks (T129, scripts/micro-verdict.py): a
# case's change is the median of its paired differences (new − base, each pair run back to back), `slower`/`faster`
# when that is over 1 ns and 3% and at least 5 of 6 pairs agree: whole runs drift on the laptop, and the old
# per-case minimums made byte-identical builds "slower" in 37–58 cases (F439); now 1–4 of 90.
# The rule (docs/design.md, "Speed work"): nothing slower; a fast path's miss may not cost more than the old path.
# Programs also get an `instr` column (instructions retired, base → new) when `perf` can count: on PATH, or
# PERF=<path> (a-03: PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf). Equal instructions with a slower time =
# code placement, not added work (T113); it is the minimum over the runs, `~x%` marking a build whose counts spread
# by more than 0.5% (GC-heavy programs: the collector's timing, F440).
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
RUNS=6 OUT=$ROOT/out/gate
while getopts "r:o:" o; do
  case $o in
    r) RUNS=$OPTARG ;; o) OUT=$OPTARG ;;
    *) sed -n '2,19p' "$0"; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
[ $# -ge 1 ] || { sed -n '2,19p' "$0"; exit 2; }
# T129: an even number of runs, so each build runs first in as many pairs as the other.
[ $((RUNS % 2)) -eq 1 ] && RUNS=$((RUNS + 1))
REF=$1; shift
mkdir -p "$OUT" || exit 1
OUT=$(cd "$OUT" && pwd)

# The base compiler: a detached worktree of the ref, built in its own target directory (kept between runs).
SRC=$OUT/base-src
rev=$(git -C "$ROOT" rev-parse --verify "$REF^{commit}") || exit 1
if [ -d "$SRC" ]; then git -C "$SRC" checkout -q --detach "$rev" || exit 1
else git -C "$ROOT" worktree add -q --detach "$SRC" "$rev" || exit 1; fi
(cd "$SRC" && CARGO_TARGET_DIR=$OUT/base-target cargo build --release -q -p js2wasi3) || exit 1
BASE=$OUT/base-target/release/js2wasi3
echo "base: $REF ($(git -C "$ROOT" rev-parse --short "$rev")); new: the working tree"

echo; echo "## Programs"; echo
"$ROOT/scripts/bench.sh" -r "$RUNS" -b "$BASE" -o "$OUT/programs" "$@"
# The geomean of new/base minimum times over the programs that ran (identical `.wasm`: 1); gate.tsv lists every run.
awk -F'\t' 'NR > 1 { n++; if ($2 == "") next; split($2, b, " "); split($3, w, " "); mb = b[1]; mw = w[1];
  for (i in b) if (b[i] + 0 < mb + 0) mb = b[i]; for (i in w) if (w[i] + 0 < mw + 0) mw = w[i]; l += log(mw / mb) }
  END { if (n) printf "\nGeomean of the %d programs (new/base, minimum times; identical = 1): %.4f\n", n, exp(l / n) }' \
  "$OUT/programs/gate.tsv"

echo; echo "## Micro-benchmarks (ns per operation; $RUNS pairs, verdicts by scripts/micro-verdict.py)"; echo
NEW=$ROOT/target/release/js2wasi3
M=$OUT/micro
mkdir -p "$M"
for f in "$ROOT"/tests/bench/micro/*.js; do
  n=$(basename "$f" .js)
  # T129: the builds are x.base.* and x.next.* (equal-length names).
  for v in base next; do
    c=$NEW; [ $v = base ] && c=$BASE
    "$c" build "$f" -o "$M/$n.$v.wasm" >/dev/null && wasmtime compile "$M/$n.$v.wasm" -o "$M/$n.$v.cwasm" || { echo "$n: $v build fails"; continue 2; }
  done
  rm -f "$M/$n".*.run*
  for i in $(seq "$RUNS"); do
    # T129: run i is a pair, base and new back to back (scripts/micro-verdict.py compares within pairs), odd runs
    # base first, even runs new first; both under names padded by i characters (the path is in argv; F440).
    pad=$(printf '%*s' "$i" '' | tr ' ' _)
    order="base next"; [ $((i % 2)) -eq 0 ] && order="next base"
    for v in $order; do
      ln -f "$M/$n.$v.cwasm" "$M/$n.$v$pad.cwasm"
      wasmtime run -Sp3 --allow-precompiled "$M/$n.$v$pad.cwasm" >"$M/$n.$v.run$i"
      rm -f "$M/$n.$v$pad.cwasm"
    done
  done
  echo "### $n"; echo
  "$ROOT/scripts/micro-verdict.py" "$M" "$n" "$RUNS"
  echo
done
