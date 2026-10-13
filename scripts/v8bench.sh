#!/usr/bin/env bash
# T116: the same component on wasmtime and on node's V8 (through jco + preview3-shim), next to node running the JS:
# how much of the gap is the engine (GC, casts, compiler) and how much our code. A measurement, not a test host.
#
# Usage: scripts/v8bench.sh [-r runs] [-o outdir] [name...]
#   name: as scripts/bench.sh (its 18 programs, small sizes, the same sources, patched copies and fasta inputs).
#   -r: runs of each (default 3), alternating node, wasmtime, V8; minimums reported.
#   JCO: the jco CLI (default ~/.local/jco/node_modules/.bin/jco; its node_modules hold preview3-shim).
# V8 times are split by scripts/v8run.mjs into startup (import, compile, instantiate the module) and `run()`;
# "V8 run" is the comparable number (wasmtime runs a precompiled .cwasm, ~15 ms startup).
# Output: a Markdown table on stdout; details in <outdir>/v8bench.tsv.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
B=$ROOT/tests/bench
RUNS=3 OUT=$ROOT/out/v8bench TIMEOUT=${TIMEOUT:-3600}
JCO=${JCO:-$HOME/.local/jco/node_modules/.bin/jco}
while getopts "r:o:" o; do
  case $o in
    r) RUNS=$OPTARG ;; o) OUT=$OPTARG ;;
    *) sed -n '2,12p' "$0"; exit 2 ;;
  esac
done
shift $((OPTIND - 1))

# As scripts/bench.sh: name | argument (small) | stdin input: fasta size (small)
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

[ -x "$JCO" ] || { echo "jco not found: $JCO" >&2; exit 1; }
mkdir -p "$OUT" || exit 1
OUT=$(cd "$OUT" && pwd)
echo '{"type":"commonjs"}' >"$OUT/package.json"
ln -sfn "$(dirname "$(dirname "$JCO")")" "$OUT/node_modules"
cp "$ROOT/scripts/v8run.mjs" "$OUT/v8run.mjs"
(cd "$ROOT" && cargo build --release -q -p js2wasi3) || exit 1
J=$ROOT/target/release/js2wasi3
MAPS=()
for p in cli clocks filesystem http random sockets; do
  MAPS+=(--map "wasi:$p/*@0.3.1=@bytecodealliance/preview3-shim/$p#*")
done

now_ns() { date +%s%N; }
ms() { echo $((($2 - $1) / 1000000)); }
min() { sort -n | head -1; }

# The source of program $1 in $OUT/$1.js (as scripts/bench.sh); prints `*` if it's a patched copy.
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

printf '| program | node JS ms | wasmtime ms | V8 total ms | V8 startup ms | V8 run ms | wasmtime / V8 run | V8 run / node | output |\n'
printf '|---|---|---|---|---|---|---|---|---|\n'
printf 'program\tnode_ms\twasmtime_ms\tv8_total_ms\tv8_init_ms\tv8_run_ms\tsame\n' >"$OUT/v8bench.tsv"
for name in "$@"; do
  line=$(echo "$PROGRAMS" | grep "^$name|") || { echo "unknown program: $name" >&2; continue; }
  IFS='|' read -r _ arg input <<<"$line"
  mark=$(source_of "$name")
  stdin=/dev/null
  if [ -n "$input" ]; then
    [ -f "$OUT/fasta-$input.fa" ] || node "$B/benchmarksgame/fasta.js" "$input" >"$OUT/fasta-$input.fa"
    cp "$OUT/fasta-$input.fa" "$OUT/input.fa"
    stdin=$OUT/input.fa
  fi
  if ! "$J" build "$OUT/$name.js" -o "$OUT/$name.wasm" >"$OUT/$name.build" 2>&1 ||
     ! wasmtime compile "$OUT/$name.wasm" -o "$OUT/$name.cwasm" 2>>"$OUT/$name.build" ||
     ! (rm -rf "$OUT/$name.jco" && "$JCO" transpile "$OUT/$name.wasm" -o "$OUT/$name.jco" --bindgen-enable-wasm-exnref \
          "${MAPS[@]}" >>"$OUT/$name.build" 2>&1 && echo '{"type":"module"}' >"$OUT/$name.jco/package.json"); then
    printf '| %s%s | build fails (see %s.build) | | | | | | | |\n' "$name" "$mark" "$name"
    continue
  fi
  (cd "$OUT" && env -u FORCE_COLOR timeout "$TIMEOUT" node "$name.js" $arg <"$stdin" >"$OUT/$name.node.out" 2>&1; echo "exit $?" >>"$OUT/$name.node.out")
  nt=() wt=() vt=() same=SAME
  rm -f "$OUT/$name.v8times"
  for i in $(seq "$RUNS"); do
    s=$(now_ns)
    (cd "$OUT" && env -u FORCE_COLOR timeout "$TIMEOUT" node "$name.js" $arg <"$stdin" >/dev/null 2>&1)
    nt+=("$(ms "$s" "$(now_ns)")")
    s=$(now_ns)
    (cd "$OUT" && timeout "$TIMEOUT" wasmtime run -Sp3 --allow-precompiled --dir "$OUT::/" "$name.cwasm" $arg \
       <"$stdin" >"$OUT/$name.wasmtime.out" 2>&1; echo "exit $?" >>"$OUT/$name.wasmtime.out")
    wt+=("$(ms "$s" "$(now_ns)")")
    s=$(now_ns)
    (cd "$OUT" && V8RUN_TIMES="$OUT/$name.v8times" timeout "$TIMEOUT" node v8run.mjs "$name.jco/$name.js" $arg \
       <"$stdin" >"$OUT/$name.v8.out" 2>&1; echo "exit $?" >>"$OUT/$name.v8.out")
    vt+=("$(ms "$s" "$(now_ns)")")
    if [ "$i" = 1 ]; then
      cmp -s "$OUT/$name.node.out" "$OUT/$name.wasmtime.out" || same="DIFF(wasmtime)"
      cmp -s "$OUT/$name.node.out" "$OUT/$name.v8.out" || same="$same DIFF(V8)"
    fi
  done
  n=$(printf '%s\n' "${nt[@]}" | min)
  w=$(printf '%s\n' "${wt[@]}" | min)
  v=$(printf '%s\n' "${vt[@]}" | min)
  vi=$(awk '{print $1}' "$OUT/$name.v8times" 2>/dev/null | sort -n | head -1)
  vr=$(awk '{print $2}' "$OUT/$name.v8times" 2>/dev/null | sort -n | head -1)
  vi=${vi:-0} vr=${vr:-0}
  printf '| %s%s | %s | %s | %s | %.0f | %.0f | %s | %s | %s |\n' "$name" "$mark" "$n" "$w" "$v" "$vi" "$vr" \
    "$(awk -v a="$w" -v b="$vr" 'BEGIN { if (b > 0) printf "%.2f×", a / b }')" \
    "$(awk -v a="$vr" -v b="$n" 'BEGIN { if (b > 0) printf "%.1f×", a / b }')" "$same"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "$n" "$w" "$v" "$vi" "$vr" "$same" >>"$OUT/v8bench.tsv"
done
