#!/usr/bin/env bash
# T112: the gate before a speed commit: the working tree's compiler against a base commit's, on the 18 programs of
# tests/bench and the micro-benchmarks of tests/bench/micro.
#
# Usage: scripts/gate.sh [-r runs] [-o outdir] <base-ref> [name...]
#   base-ref: the commit to compare against (usually HEAD before the change, or the last speed commit).
#   name: programs to run (default: all 18; see scripts/bench.sh).
#   -r: runs of each build per program and per micro-benchmark (default 5), alternating base and new.
#   -o: work directory (default out/gate).
# Programs: `scripts/bench.sh -b` (identical `.wasm` = no change, not run; else `slower`/`faster` when the minimum
# times differ by more than 2%; outputs must match node's). Micro-benchmarks: each case's minimum over the runs,
# `slower`/`faster` when they differ by more than 1 ns and 3%. Minimums because noise only adds time.
# The rule (docs/design.md, "Speed work"): nothing slower; a fast path's miss may not cost more than the old path.
# Programs also get an `instr` column (instructions retired, base → new) when `perf` can count: on PATH, or
# PERF=<path> (a-03: PERF=/usr/lib/linux-tools/5.15.0-198-generic/perf). Equal instructions with a slower time =
# code placement, not added work (T113).
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
RUNS=5 OUT=$ROOT/out/gate
while getopts "r:o:" o; do
  case $o in
    r) RUNS=$OPTARG ;; o) OUT=$OPTARG ;;
    *) sed -n '2,16p' "$0"; exit 2 ;;
  esac
done
shift $((OPTIND - 1))
[ $# -ge 1 ] || { sed -n '2,16p' "$0"; exit 2; }
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

echo; echo "## Micro-benchmarks (ns per operation, minimum of $RUNS runs)"; echo
NEW=$ROOT/target/release/js2wasi3
M=$OUT/micro
mkdir -p "$M"
for f in "$ROOT"/tests/bench/micro/*.js; do
  n=$(basename "$f" .js)
  for v in base new; do
    c=$NEW; [ $v = base ] && c=$BASE
    "$c" build "$f" -o "$M/$n.$v.wasm" >/dev/null && wasmtime compile "$M/$n.$v.wasm" -o "$M/$n.$v.cwasm" || { echo "$n: $v build fails"; continue 2; }
  done
  rm -f "$M/$n".*.run*
  for i in $(seq "$RUNS"); do
    for v in base new; do wasmtime run -Sp3 --allow-precompiled "$M/$n.$v.cwasm" >"$M/$n.$v.run$i"; done
  done
  echo "### $n"; echo
  python3 - "$M" "$n" "$RUNS" <<'EOF'
import sys
m, n, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
def read(v):
    out = {}
    for i in range(1, runs + 1):
        for line in open(f"{m}/{n}.{v}.run{i}"):
            name, _, val = line.rstrip().rpartition(" ")
            try:
                x = float(val)
            except ValueError:
                continue
            out.setdefault(name, []).append(x)
    return out
b, w = read("base"), read("new")
print("| case | base ns | new ns | change | verdict |\n|---|---|---|---|---|")
for k in b:
    if k not in w:
        continue
    d = min(w[k]) - min(b[k])
    verdict = "slower" if d > max(1.0, 0.03 * min(b[k])) else "faster" if -d > max(1.0, 0.03 * min(b[k])) else "same"
    print(f"| {k} | {min(b[k]):.1f} | {min(w[k]):.1f} | {min(w[k]) - min(b[k]):+.1f} | {verdict} |")
EOF
  echo
done
