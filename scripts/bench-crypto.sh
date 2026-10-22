#!/usr/bin/env bash
# Node's own crypto benchmarks (tests/bench/crypto, run through tests/bench/common.js) under node and under
# js2wasi3 + wasmtime, ns per operation for every config (T149). Usage: scripts/bench-crypto.sh [out-dir]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/out/bench-crypto}
mkdir -p "$OUT"
for f in "$ROOT"/tests/bench/crypto/*.js; do
  n=$(basename "$f" .js)
  node "$f" >"$OUT/$n.node.txt"
  "$ROOT/target/release/js2wasi3" build "$f" -o "$OUT/$n.wasm" 2>/dev/null
  wasmtime run -Sp3 "$OUT/$n.wasm" >"$OUT/$n.wasm.txt"
  paste -d'|' <(cut -d: -f1 "$OUT/$n.node.txt") <(cut -d: -f2 "$OUT/$n.node.txt") <(cut -d: -f2 "$OUT/$n.wasm.txt") |
    awk -F'|' -v n="$n" '{split($1,c," "); k=c[1]; split($2,a," "); split($3,b," "); N[k]+=a[1]; W[k]+=b[1]; C[k]++}
      END{for (k in C) printf "%s %s: node %.0f ns/op, js2wasi3 %.0f ns/op (%.2fx), %d configs\n", n, k, N[k]/C[k], W[k]/C[k], W[k]/N[k], C[k]}' | sort
done
