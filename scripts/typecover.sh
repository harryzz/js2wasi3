#!/usr/bin/env bash
# T131: how much of the 18 programs' run-time types the type analysis (`js2wasi3 types`) predicts.
# Runs each program once under node with typeprobe's recording (outputs compared with plain node), then joins.
# Usage: scripts/typecover.sh [-o out/typecover] [-j 4] [program...]   (sources as scripts/v8bench.sh builds them)
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
B=$ROOT/tests/bench
OUT=$ROOT/out/typecover JOBS=4
while getopts "o:j:" o; do case $o in o) OUT=$OPTARG ;; j) JOBS=$OPTARG ;; *) exit 2 ;; esac; done
shift $((OPTIND - 1))
PROGRAMS="
binarytrees|16|
fannkuchredux|10|
fasta|250000|
knucleotide||250000
mandelbrot|1000|
nbody|500000|
pidigits|2000|
regexredux||500000
revcomp||250000
spectralnorm|500|
octane-richards|fixed 0.05|
octane-deltablue|fixed 0.05|
octane-raytrace|fixed 0.05|
octane-navier-stokes|fixed 0.05|
octane-crypto|fixed 0.05|
octane-splay|fixed 0.05|
octane-earley-boyer|fixed 0.05|
octane-regexp|fixed 0.05|
"
[ $# -eq 0 ] && set -- $(echo "$PROGRAMS" | cut -d'|' -f1)
mkdir -p "$OUT" && OUT=$(cd "$OUT" && pwd)
(cd "$ROOT" && cargo build --release -q -p js2wasi3 -p typeprobe) || exit 1
J=$ROOT/target/release/js2wasi3 TP=$ROOT/target/release/typeprobe

one() {
  local name=$1 arg=$2 input=$3 d=$OUT/run-$1 src
  mkdir -p "$d"
  case $name in
    octane-*)
      local b=${name#octane-}; src=$B/octane/$b.js; [ -f "$B/patched/$b.js" ] && src=$B/patched/$b.js
      { cat "$B/octane/prelude.js"; cat "$B/octane/base.js" "$src" "$B/octane/driver.js" | sed 's/Math\.random/__random/g'; } >"$d/p.js" ;;
    *) src=$B/benchmarksgame/$name.js; [ -f "$B/patched/$name.js" ] && src=$B/patched/$name.js; cp "$src" "$d/p.js" ;;
  esac
  local stdin=/dev/null
  if [ -n "$input" ]; then
    [ -f "$OUT/fasta-$input.fa" ] || node "$B/benchmarksgame/fasta.js" "$input" >"$OUT/fasta-$input.fa"
    cp "$OUT/fasta-$input.fa" "$d/input.fa"; stdin=$d/input.fa
  fi
  "$J" types "$d/p.js" >"$OUT/$name.types" 2>"$OUT/$name.types.log" || { echo "$name: types failed"; return; }
  "$TP" --sites "$d/p.js" "$d/probe.js" "$OUT/$name.tsv" || { echo "$name: rewrite failed"; return; }
  local t0=$(date +%s)
  (cd "$d" && TYPEPROBE_OUT=$OUT/$name.json timeout 3600 node --max-old-space-size=8000 probe.js $arg <"$stdin" >probe.out 2>&1)
  local st=$?
  (cd "$d" && node p.js $arg <"$stdin" >ref.out 2>&1)
  cmp -s "$d/probe.out" "$d/ref.out" && same=SAME || same=DIFF
  echo "$name exit $st $same $(($(date +%s) - t0))s $(cat "$OUT/$name.types.log")"
}
n=0
for name in "$@"; do
  line=$(echo "$PROGRAMS" | grep "^$name|") || { echo "unknown: $name"; continue; }
  IFS='|' read -r _ arg input <<<"$line"
  one "$name" "$arg" "$input" &
  n=$((n + 1)); [ $((n % JOBS)) -eq 0 ] && wait
done
wait
node "$ROOT/crates/typeprobe/report.mjs" "$OUT" "$@" | tee "$OUT/report.md"
