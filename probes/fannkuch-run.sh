#!/usr/bin/env bash
# T140: build and time the three fannkuch models (probes/fannkuch-gen.py) on wasmtime; minimum of $RUNS runs.
# Usage: probes/fannkuch-run.sh [n] [runs]   (node's answer for n=10: checksum 73196, flips 38)
set -u
cd "$(dirname "$0")/.."; N=${1:-10}; RUNS=${2:-5}
python3 probes/fannkuch-gen.py >/dev/null; mkdir -p probes/out
for m in 0 1 2 3 4 5 6; do
  wasm-tools parse probes/fannkuch-m$m.wat -o probes/out/fannkuch-m$m.wasm || exit 1
  # precompile once so the time is the run (as the benchmarks' .cwasm)
  wasmtime compile -W gc=y,function-references=y probes/out/fannkuch-m$m.wasm -o probes/out/fannkuch-m$m.cwasm || exit 1
  best=999999 res=
  for r in $(seq $RUNS); do
    s=$(date +%s%N)
    res=$(wasmtime run -W gc=y,function-references=y --allow-precompiled --invoke run probes/out/fannkuch-m$m.cwasm $N 2>/dev/null | tr '\n' ' ')
    e=$(( ($(date +%s%N) - s) / 1000000 )); [ $e -lt $best ] && best=$e
  done
  echo "M$m: $best ms  result: $res"
done
