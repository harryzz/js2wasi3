#!/bin/bash
# T86: length-bench.js under node and under each given js2wasi3 compiler (wasmtime 49.0.2, 512 MiB initial GC heap,
# compile time excluded), RUNS times each; prints the minimum per test and checks every build's stdout against node's.
# Usage: tasks/T86-files/bench.sh <compiler>...   (RUNS=3)
T=$(cd "$(dirname "$0")" && pwd); W=$(mktemp -d); RUNS=${RUNS:-3}
min() { sed 's/ ms$//' | awk '{t[$1]=(t[$1]==""||$2<t[$1])?$2:t[$1]} END {for (k in t) printf "%-12s %6d ms\n", k, t[k]}' | sort; }
env -u FORCE_COLOR node $T/length-bench.js >$W/node.out 2>/dev/null
echo "== node"; for i in $(seq $RUNS); do env -u FORCE_COLOR node $T/length-bench.js 2>&1 >/dev/null; done | min
for c in "$@"; do
  $c build $T/length-bench.js -o $W/m.wasm || exit 1
  wasmtime compile -O gc-heap-initial-size=536870912 $W/m.wasm -o $W/m.cwasm || exit 1
  run() { wasmtime run --allow-precompiled -Sp3 -O gc-heap-initial-size=536870912 $W/m.cwasm; }
  run >$W/m.out 2>/dev/null; cmp -s $W/node.out $W/m.out || { echo "OUTPUT DIFFERS: $c"; diff $W/node.out $W/m.out; }
  echo "== $c"; for i in $(seq $RUNS); do run 2>&1 >/dev/null; done | min
done
rm -rf $W
