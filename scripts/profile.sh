#!/usr/bin/env bash
# perf profiles of the benchmarks under node and js2wasi3 (docs/performance.md); compare with scripts/profile-compare.py.
#
# Usage: scripts/profile.sh <benchdir> <outdir> [name...]
#   benchdir: a `scripts/bench.sh -o <benchdir>` directory (its .js, .cwasm and fasta-*.fa files; small sizes).
#   Writes <outdir>/wasm/<name>.txt and <outdir>/node/<name>.txt (`perf script -F comm,tid,ip,sym`).
#   PERF: the perf binary (default perf; WSL needs the full path of linux-tools' perf, not Ubuntu's wrapper).
# js2wasi3: `wasmtime run --profile=perfmap` at 999 Hz; node: `--perf-basic-prof --interpreted-frames-native-stack`
# at 4999 Hz (its runs are ~20x shorter). `-e cpu-clock -g` over all threads (WSL has no PMU).
set -u
[ $# -ge 2 ] || { sed -n '2,10p' "$0"; exit 2; }
O=$(cd "$1" && pwd) P=$2; shift 2
PERF=${PERF:-perf}
mkdir -p "$P/wasm" "$P/node"; P=$(cd "$P" && pwd)
# name | argument | stdin: fasta size (scripts/bench.sh's small sizes)
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
for n in "$@"; do
  IFS='|' read -r _ a in < <(echo "$PROGRAMS" | grep "^$n|") || { echo "unknown program: $n" >&2; continue; }
  st=/dev/null
  if [ -n "$in" ]; then cp "$O/fasta-$in.fa" "$O/input.fa"; st=$O/input.fa; fi
  (cd "$O" && $PERF record -q -e cpu-clock -F 999 -g -o "$P/wasm/$n.data" \
    wasmtime run -Sp3 --profile=perfmap --allow-precompiled --dir "$O::/" "$n.cwasm" $a <"$st" >"$P/wasm/$n.out" 2>&1)
  $PERF script -i "$P/wasm/$n.data" -F comm,tid,ip,sym >"$P/wasm/$n.txt" 2>/dev/null
  (cd "$O" && env -u FORCE_COLOR $PERF record -q -e cpu-clock -F 4999 -g -o "$P/node/$n.data" \
    node --perf-basic-prof --interpreted-frames-native-stack "$n.js" $a <"$st" >"$P/node/$n.out" 2>&1)
  $PERF script -i "$P/node/$n.data" -F comm,tid,ip,sym >"$P/node/$n.txt" 2>/dev/null
  cmp -s "$P/wasm/$n.out" "$P/node/$n.out" && echo "$n: done, same output" || echo "$n: done, OUTPUT DIFFERS"
done
