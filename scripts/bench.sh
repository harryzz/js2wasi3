#!/usr/bin/env bash
# T78: run-time benchmarks against node: the Benchmarks Game's programs and Octane 2 (tests/bench, docs/benchmarks.md).
#
# Usage: scripts/bench.sh [-r runs] [-s small|official] [-g] [-o outdir] [name...]
#   name: a Benchmarks Game program (binarytrees, fannkuchredux, fasta, knucleotide, mandelbrot, nbody, pidigits,
#   regexredux, revcomp, spectralnorm) or an Octane benchmark (octane-richards, octane-deltablue, octane-raytrace,
#   octane-navier-stokes, octane-crypto, octane-splay, octane-earley-boyer, octane-regexp); default: all of them.
#   -r: runs per program, alternating node and js2wasi3 (default 5); the minimum and the median are reported.
#   -s: input sizes (default small: seconds under js2wasi3; official: the Benchmarks Game's sizes and Octane's full
#       deterministicIterations, which take minutes to hours under js2wasi3).
#   -g: also give wasmtime `-O gc-heap-initial-size=268435456` (compile and run).
#   -o: work directory (default out/bench).
# Each program is built once (`js2wasi3 build` with the release compiler, `wasmtime compile`; not timed) and then
# run as `wasmtime run -Sp3 --allow-precompiled x.cwasm`, the same source as `node x.js`; wall time from outside.
# Outputs (stdout, stderr, exit status of the first run) must be equal bytes, else the row says DIFF.
# A program that can't be built under its original source runs as its copy in tests/bench/patched/ (marked `*`);
# Octane's files are always concatenated as prelude.js + base.js + the benchmark + driver.js (driver mode `fixed`).
# Output: one Markdown table row per program on stdout; details in <outdir>/results.tsv.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
B=$ROOT/tests/bench
RUNS=5 SIZE=small GC=() OUT=$ROOT/out/bench TIMEOUT=${TIMEOUT:-3600}
while getopts "r:s:go:" o; do
  case $o in
    r) RUNS=$OPTARG ;; s) SIZE=$OPTARG ;; g) GC=(-O gc-heap-initial-size=268435456) ;; o) OUT=$OPTARG ;;
    *) sed -n '2,20p' "$0"; exit 2 ;;
  esac
done
shift $((OPTIND - 1))

# name | argument (small) | argument (official) | stdin input: fasta size (small) | (official)
PROGRAMS="
binarytrees|16|21||
fannkuchredux|10|12||
fasta|250000|25000000||
knucleotide|||250000|25000000
mandelbrot|1000|16000||
nbody|500000|50000000||
pidigits|2000|10000||
regexredux|||500000|5000000
revcomp|||250000|25000000
spectralnorm|500|5500||
octane-richards|fixed 0.05|fixed 1||
octane-deltablue|fixed 0.05|fixed 1||
octane-raytrace|fixed 0.05|fixed 1||
octane-navier-stokes|fixed 0.05|fixed 1||
octane-crypto|fixed 0.05|fixed 1||
octane-splay|fixed 0.05|fixed 1||
octane-earley-boyer|fixed 0.05|fixed 1||
octane-regexp|fixed 0.05|fixed 1||
"
[ $# -eq 0 ] && set -- $(echo "$PROGRAMS" | cut -d'|' -f1)

mkdir -p "$OUT" || exit 1
OUT=$(cd "$OUT" && pwd)
echo '{"type":"commonjs"}' >"$OUT/package.json"
(cd "$ROOT" && cargo build --release -q -p js2wasi3) || exit 1
J=$ROOT/target/release/js2wasi3

now_ns() { date +%s%N; }
ms() { echo $((($2 - $1) / 1000000)); }
# min and median of numbers on stdin
stats() { sort -n | awk '{a[NR]=$1} END {m=(NR%2)?a[(NR+1)/2]:int((a[NR/2]+a[NR/2+1])/2); print a[1], m}'; }

# The source of program $1 in $OUT/$1.js; prints `*` if it's a patched copy.
source_of() {
  local n=$1 b
  case $n in
    octane-*)
      b=${n#octane-}
      local src=$B/octane/$b.js mark=""
      [ -f "$B/patched/$b.js" ] && src=$B/patched/$b.js mark="*"
      { cat "$B/octane/prelude.js"; cat "$B/octane/base.js" "$src" "$B/octane/driver.js" | sed 's/Math\.random/__random/g'; } >"$OUT/$n.js"
      echo "$mark" ;;
    *)
      if [ -f "$B/patched/$n.js" ]; then cp "$B/patched/$n.js" "$OUT/$n.js"; echo "*"; else cp "$B/benchmarksgame/$n.js" "$OUT/$n.js"; fi ;;
  esac
}

printf '| program | argument / input | node min (median) s | js2wasi3 min (median) s | ratio (min) | output | .wasm KB |\n'
printf '|---|---|---|---|---|---|---|\n'
printf 'program\targ\tinput\tnode_ms\twasm_ms\tsame\twasm_bytes\n' >"$OUT/results.tsv"
for name in "$@"; do
  line=$(echo "$PROGRAMS" | grep "^$name|") || { echo "unknown program: $name" >&2; continue; }
  IFS='|' read -r _ a_small a_off in_small in_off <<<"$line"
  if [ "$SIZE" = official ]; then arg=$a_off input=$in_off; else arg=$a_small input=$in_small; fi
  mark=$(source_of "$name")
  shown="$arg" stdin=/dev/null
  if [ -n "$input" ]; then
    # stdin programs read input.fa (patched copies: js2wasi3 has no stdin, F239); fasta's own output is the input
    if [ ! -f "$OUT/fasta-$input.fa" ]; then node "$B/benchmarksgame/fasta.js" "$input" >"$OUT/fasta-$input.fa"; fi
    cp "$OUT/fasta-$input.fa" "$OUT/input.fa"
    stdin=$OUT/input.fa
    shown="fasta $input"
  fi
  if ! "$J" build "$OUT/$name.js" -o "$OUT/$name.wasm" >"$OUT/$name.build" 2>&1 ||
     ! wasmtime compile "${GC[@]}" "$OUT/$name.wasm" -o "$OUT/$name.cwasm" 2>>"$OUT/$name.build"; then
    printf '| %s%s | %s | | | | build fails: %s | |\n' "$name" "$mark" "$shown" "$(grep -m1 error "$OUT/$name.build" | sed 's/.*error: //' | cut -c1-120)"
    continue
  fi
  size=$(stat -c %s "$OUT/$name.wasm")
  nt=() wt=() same=SAME
  for i in $(seq "$RUNS"); do
    s=$(now_ns)
    (cd "$OUT" && env -u FORCE_COLOR timeout "$TIMEOUT" node "$name.js" $arg <"$stdin" >"$OUT/$name.node.out" 2>&1; echo "exit $?" >>"$OUT/$name.node.out")
    m=$(now_ns)
    (cd "$OUT" && timeout "$TIMEOUT" wasmtime run "${GC[@]}" -Sp3 --allow-precompiled --dir "$OUT::/" "$name.cwasm" $arg \
      <"$stdin" >"$OUT/$name.wasm.out" 2>&1; echo "exit $?" >>"$OUT/$name.wasm.out")
    e=$(now_ns)
    nt+=("$(ms "$s" "$m")") wt+=("$(ms "$m" "$e")")
    if [ "$i" = 1 ] && ! cmp -s "$OUT/$name.node.out" "$OUT/$name.wasm.out"; then
      same=DIFF; cp "$OUT/$name.node.out" "$OUT/$name.node.first"; cp "$OUT/$name.wasm.out" "$OUT/$name.wasm.first"
    fi
    [ "$same" = DIFF ] && break  # a wrong result isn't timed further
  done
  read -r nmin nmed < <(printf '%s\n' "${nt[@]}" | stats)
  read -r wmin wmed < <(printf '%s\n' "${wt[@]}" | stats)
  ratio=$(awk -v a="$wmin" -v b="$nmin" 'BEGIN { printf "%.1f", (b > 0 ? a / b : 0) }')
  sec() { awk -v x="$1" 'BEGIN { printf "%.2f", x / 1000 }'; }
  printf '| %s%s | %s | %s (%s) | %s (%s) | %s× | %s | %d |\n' "$name" "$mark" "$shown" "$(sec "$nmin")" "$(sec "$nmed")" \
    "$(sec "$wmin")" "$(sec "$wmed")" "$ratio" "$same" $((size / 1024))
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$arg" "$input" "${nt[*]}" "${wt[*]}" "$same" "$size" >>"$OUT/results.tsv"
done
