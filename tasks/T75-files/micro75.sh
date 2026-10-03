#!/bin/bash
# T75: micro75.js (or MICRO=<file>) under node and under each given js2wasi3 compiler (wasmtime with zigbee2mqtt's
# 256 MiB GC heap), RUNS times each; prints the minimum per test, and checks every build's stdout against node's.
# Usage: tasks/T75-files/micro75.sh <compiler>...   (RUNS=5)
T=$(cd "$(dirname "$0")" && pwd); W=$(mktemp -d); RUNS=${RUNS:-5}; M=${MICRO:-$T/micro75.js}
node $M >$W/node.out 2>/dev/null
min() { awk '{t[$1]=(t[$1]==""||$2<t[$1])?$2:t[$1]} END {for (k in t) printf "%-12s %5d ms\n", k, t[k]}' | sort; }
echo "== node"; for i in $(seq $RUNS); do node $M 2>&1 >/dev/null; done | min
for c in "$@"; do
  $c build $M -o $W/m.wasm || exit 1
  wasmtime run -Sp3 -O gc-heap-initial-size=268435456 $W/m.wasm >$W/m.out 2>/dev/null; cmp -s $W/node.out $W/m.out || echo "OUTPUT DIFFERS: $c"
  echo "== $c"; for i in $(seq $RUNS); do wasmtime run -Sp3 -O gc-heap-initial-size=268435456 $W/m.wasm 2>&1 >/dev/null; done | min
done
rm -rf $W
