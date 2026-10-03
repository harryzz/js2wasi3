#!/bin/bash
# T74: micro.js under node and under each given js2wasi3 compiler (wasmtime with zigbee2mqtt's 256 MiB GC heap),
# RUNS times each; prints the minimum per test, and checks every build's stdout against node's.
# Usage: tasks/T74-files/micro.sh <compiler>...   (RUNS=5)
T=$(cd "$(dirname "$0")" && pwd); W=$(mktemp -d); RUNS=${RUNS:-5}
node $T/micro.js >$W/node.out 2>/dev/null
min() { awk '{t[$1" "$2]=(t[$1" "$2]==""||$3<t[$1" "$2])?$3:t[$1" "$2]} END {for (k in t) printf "%-18s %5d ms\n", k, t[k]}' | sort; }
echo "== node"; for i in $(seq $RUNS); do node $T/micro.js 2>&1 >/dev/null; done | sed 's/ ms//; s/^big objects/big_objects x/; s/^prototype methods/prototype_methods x/; s/^\([a-z]*\) \([0-9]*\)$/\1 x \2/' | min
for c in "$@"; do
  $c build $T/micro.js -o $W/m.wasm || exit 1
  wasmtime run -Sp3 -O gc-heap-initial-size=268435456 $W/m.wasm >$W/m.out 2>/dev/null; cmp -s $W/node.out $W/m.out || echo "OUTPUT DIFFERS: $c"
  echo "== $c"; for i in $(seq $RUNS); do wasmtime run -Sp3 -O gc-heap-initial-size=268435456 $W/m.wasm 2>&1 >/dev/null; done | sed 's/ ms//; s/^big objects/big_objects x/; s/^prototype methods/prototype_methods x/; s/^\([a-z]*\) \([0-9]*\)$/\1 x \2/' | min
done
rm -rf $W
