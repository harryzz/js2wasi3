#!/usr/bin/env bash
# T78: run-time benchmarks against node: the Benchmarks Game's programs and Octane 2 (tests/bench; results on the compatibility page: `node scripts/compat.mjs bench <outdir>/results.tsv`).
#
# Usage: scripts/bench.sh [-r runs] [-s small|official] [-g] [-c] [-b base-compiler] [-o outdir] [name...]
#   name: a Benchmarks Game program (binarytrees, fannkuchredux, fasta, knucleotide, mandelbrot, nbody, pidigits,
#   regexredux, revcomp, spectralnorm) or an Octane benchmark (octane-richards, octane-deltablue, octane-raytrace,
#   octane-navier-stokes, octane-crypto, octane-splay, octane-earley-boyer, octane-regexp); default: all of them.
#   -r: runs per program, alternating node and js2wasi3 (default 5); the minimum and the median are reported.
#   -s: input sizes (default small: seconds under js2wasi3; official: the Benchmarks Game's sizes and Octane's full
#       deterministicIterations, which take minutes to hours under js2wasi3).
#   -g: also give wasmtime `-O gc-heap-initial-size=268435456` (compile and run).
#   -o: work directory (default out/bench).
#   -c: count instead of timing (T112): build with `--count`, run once, check stdout against node's, and write
#       the fast-path counters (`count <name> <n>` on stderr) to <outdir>/counts.tsv (program, counter, n).
#   -b: compare against another js2wasi3 binary instead of node (T112's gate, `scripts/gate.sh`): a program whose
#       `.wasm` is the same bytes under both is reported `identical` and not run; else base and new alternate,
#       `runs` times each; `slower`/`faster` when the minimums differ by more than 2% (noise only adds time, so
#       the minimum is the measure), else `same`.
#       Outputs must match node's first run. Details in <outdir>/gate.tsv.
#       Runs alternate the order (odd runs base first, even runs new first; an odd -r is rounded up: T129); run i runs both builds
#       under names padded by i characters (x.base_i.cwasm, x.next_i.cwasm): argv carries the path, which moves a
#       GC-heavy program's collections, so the runs sample several layouts, the same for both builds (F440).
#       With `perf` (on PATH, or $PERF): every timed run is also under `perf stat -e instructions:u`, the `instr`
#       column (change of the minimum counts): a time change with an instruction change under 0.5% is code placement
#       (the same work, run from another address; T113: pidigits +5.5% at equal instructions), not work the change
#       added. `~x%` after it: the counts of one build spread by x% over its runs (GC-heavy programs: the collector's
#       timing; raytrace ±10%), so that column can't tell work from noise there.
# Each program is built once (`js2wasi3 build` with the release compiler, `wasmtime compile`; not timed) and then
# run as `wasmtime run -Sp3 --allow-precompiled x.cwasm`, the same source as `node x.js`; wall time from outside.
# Outputs (stdout, stderr, exit status of the first run) must be equal bytes, else the row says DIFF.
# A program that can't be built under its original source runs as its copy in tests/bench/patched/ (marked `*`);
# Octane's files are always concatenated as prelude.js + base.js + the benchmark + driver.js (driver mode `fixed`).
# Output: one Markdown table row per program on stdout; details in <outdir>/results.tsv.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
B=$ROOT/tests/bench
RUNS=5 SIZE=small GC=() OUT=$ROOT/out/bench TIMEOUT=${TIMEOUT:-3600} COUNT= BASE=
while getopts "r:s:gcb:o:" o; do
  case $o in
    r) RUNS=$OPTARG ;; s) SIZE=$OPTARG ;; g) GC=(-O gc-heap-initial-size=268435456) ;; c) COUNT=--count ;; b) BASE=$(realpath "$OPTARG") ;; o) OUT=$OPTARG ;;
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
# perf for the gate's instruction counts: $PERF, else `perf` on PATH if it can count (perf stat needs the full
# path of what it runs).
PERF=${PERF:-$(command -v perf || true)}
[ -n "$PERF" ] && ! "$PERF" stat -x, -e instructions:u true >/dev/null 2>&1 && PERF=
WASMTIME=$(command -v wasmtime)

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

if [ -n "$COUNT" ]; then
  printf 'program\tcounter\tn\n' >"$OUT/counts.tsv"
elif [ -n "$BASE" ]; then
  printf '| program | base min ms | new min ms | change (min) | verdict | instr | output |\n|---|---|---|---|---|---|---|\n'
  printf 'program\tbase_ms\tnew_ms\tverdict\tsame\tinstr\n' >"$OUT/gate.tsv"
else
  printf '| program | argument / input | node min (median) s | js2wasi3 min (median) s | ratio (min) | output | .wasm KB |\n'
  printf '|---|---|---|---|---|---|---|\n'
  printf 'program\targ\tinput\tnode_ms\twasm_ms\tsame\twasm_bytes\n' >"$OUT/results.tsv"
fi
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
  if ! "$J" build $COUNT "$OUT/$name.js" -o "$OUT/$name.wasm" >"$OUT/$name.build" 2>&1 ||
     ! wasmtime compile "${GC[@]}" "$OUT/$name.wasm" -o "$OUT/$name.cwasm" 2>>"$OUT/$name.build"; then
    printf '| %s%s | %s | | | | build fails: %s | |\n' "$name" "$mark" "$shown" "$(grep -m1 error "$OUT/$name.build" | sed 's/.*error: //' | cut -c1-120)"
    continue
  fi
  size=$(stat -c %s "$OUT/$name.wasm")
  if [ -n "$BASE" ]; then
    if ! "$BASE" build "$OUT/$name.js" -o "$OUT/$name.base.wasm" >>"$OUT/$name.build" 2>&1 ||
       ! wasmtime compile "${GC[@]}" "$OUT/$name.base.wasm" -o "$OUT/$name.base.cwasm" 2>>"$OUT/$name.build"; then
      printf '| %s%s | base build fails | | | | | |\n' "$name" "$mark"; continue
    fi
    if cmp -s "$OUT/$name.wasm" "$OUT/$name.base.wasm"; then
      printf '| %s%s | | | | identical | | |\n' "$name" "$mark"
      printf '%s\t\t\tidentical\t\n' "$name" >>"$OUT/gate.tsv"; continue
    fi
    (cd "$OUT" && env -u FORCE_COLOR timeout "$TIMEOUT" node "$name.js" $arg <"$stdin" >"$OUT/$name.node.out" 2>&1; echo "exit $?" >>"$OUT/$name.node.out")
    # T129 (F440): argv carries the module's path, and the path moves where a GC-heavy program's collections fall
    # (the same binarytrees build: 58,768M or 59,218M instructions by its file name). Each run i uses its own name,
    # padded by i characters, the same for both builds (x.base_i.cwasm, x.next_i.cwasm: equal lengths), so the
    # runs sample several layouts and the minimums compare like with like (Mytkowicz et al., ASPLOS 2009).
    rm -f "$OUT/$name".*.instr* "$OUT/$name".base_*.cwasm "$OUT/$name".next_*.cwasm
    bt=() wt=() bi=() wi=() same=SAME
    # An even number of runs: each build first in as many pairs (T129).
    [ $((RUNS % 2)) -eq 1 ] && RUNS=$((RUNS + 1))
    for i in $(seq "$RUNS"); do
      pad=$(printf '%*s' "$i" '' | tr ' ' _)
      ln -f "$OUT/$name.base.cwasm" "$OUT/$name.base$pad.cwasm"
      ln -f "$OUT/$name.cwasm" "$OUT/$name.next$pad.cwasm"
      # T129: odd runs base first, even runs new first, so neither build always runs second.
      order="base new"; [ $((i % 2)) -eq 0 ] && order="new base"
      for v in $order; do
        c=$name.next$pad.cwasm; [ $v = base ] && c=$name.base$pad.cwasm
        # T129: every timed run also counts instructions (perf stat's counting mode adds no measurable time), so
        # the `instr` column is a minimum over the runs like the times (F440).
        pf=(); [ -n "$PERF" ] && pf=("$PERF" stat -x, -e instructions:u -o "$OUT/$name.$v.instr$i")
        s=$(now_ns)
        (cd "$OUT" && timeout "$TIMEOUT" "${pf[@]}" "$WASMTIME" run "${GC[@]}" -Sp3 --allow-precompiled --dir "$OUT::/" "$c" $arg \
          <"$stdin" >"$OUT/$name.$v.out" 2>&1; echo "exit $?" >>"$OUT/$name.$v.out")
        e=$(now_ns)
        [ $v = base ] && bt+=("$(ms "$s" "$e")") || wt+=("$(ms "$s" "$e")")
        if [ -n "$PERF" ]; then
          n=$(grep instructions "$OUT/$name.$v.instr$i" 2>/dev/null | cut -d, -f1)
          [ $v = base ] && bi+=("${n:-0}") || wi+=("${n:-0}")
        fi
        cmp -s "$OUT/$name.node.out" "$OUT/$name.$v.out" || same="DIFF ($v)"
      done
      rm -f "$OUT/$name.base$pad.cwasm" "$OUT/$name.next$pad.cwasm"
    done
    read -r bmin wmin verdict < <(printf '%s\n%s\n' "${bt[*]}" "${wt[*]}" | awk '
      function med(a, n,   t, i, j) { for (i = 1; i <= n; i++) for (j = i + 1; j <= n; j++) if (a[j] < a[i]) { t = a[i]; a[i] = a[j]; a[j] = t }
        return (n % 2) ? a[(n + 1) / 2] : int((a[n / 2] + a[n / 2 + 1]) / 2) }
      NR == 1 { nb = split($0, b, " ") } NR == 2 { nw = split($0, w, " ") }
      END { bmin = bmax = b[1]; for (i in b) { if (b[i] < bmin) bmin = b[i]; if (b[i] > bmax) bmax = b[i] }
            wmin = wmax = w[1]; for (i in w) { if (w[i] < wmin) wmin = w[i]; if (w[i] > wmax) wmax = w[i] }
            v = (wmin > bmin * 1.02) ? "slower" : (wmin < bmin * 0.98) ? "faster" : "same"; print bmin, wmin, v }')
    pct=$(awk -v b="$bmin" -v w="$wmin" 'BEGIN { printf "%+.1f%%", 100 * (w - b) / b }')
    # instr: the change of the minimum counts; `~` and the larger spread (max/min − 1 of either build) when that
    # is over 0.5%: the count then moves with the collector, not with the code (raytrace: ±10%, F440).
    ipct=
    if [ -n "$PERF" ]; then
      ipct=$(printf '%s\n%s\n' "${bi[*]}" "${wi[*]}" | awk '
        NR == 1 { nb = split($0, b, " ") } NR == 2 { nw = split($0, w, " ") }
        END { bmin = bmax = b[1]; for (i in b) { if (b[i] < bmin) bmin = b[i]; if (b[i] > bmax) bmax = b[i] }
              wmin = wmax = w[1]; for (i in w) { if (w[i] < wmin) wmin = w[i]; if (w[i] > wmax) wmax = w[i] }
              if (!(bmin > 0 && wmin > 0)) { print "?"; exit }
              sp = bmax / bmin; if (wmax / wmin > sp) sp = wmax / wmin; sp = 100 * (sp - 1)
              printf "%+.2f%%", 100 * (wmin - bmin) / bmin; if (sp > 0.5) printf " ~%.1f%%", sp }')
    fi
    printf '| %s%s | %s | %s | %s | %s | %s | %s |\n' "$name" "$mark" "$bmin" "$wmin" "$pct" "$verdict" "$ipct" "$same"
    printf '%s\t%s\t%s\t%s\t%s\t%s\n' "$name" "${bt[*]}" "${wt[*]}" "$verdict" "$same" "$ipct" >>"$OUT/gate.tsv"
    continue
  fi
  if [ -n "$COUNT" ]; then
    # One run each: stdout must be node's; the counters go to counts.tsv.
    (cd "$OUT" && env -u FORCE_COLOR timeout "$TIMEOUT" node "$name.js" $arg <"$stdin" >"$OUT/$name.node.stdout" 2>/dev/null)
    (cd "$OUT" && timeout "$TIMEOUT" wasmtime run "${GC[@]}" -Sp3 --allow-precompiled --dir "$OUT::/" "$name.cwasm" $arg \
      <"$stdin" >"$OUT/$name.wasm.stdout" 2>"$OUT/$name.counts")
    cmp -s "$OUT/$name.node.stdout" "$OUT/$name.wasm.stdout" && same=SAME || same=DIFF
    grep '^count ' "$OUT/$name.counts" | awk -v p="$name" '{ print p "\t" $2 "\t" $3 }' >>"$OUT/counts.tsv"
    echo "$name$mark: $same, $(grep -c '^count ' "$OUT/$name.counts") counters"
    continue
  fi
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
