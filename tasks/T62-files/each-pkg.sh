#!/bin/bash
# T62: for each package in $S/zp/patched/node_modules: build `require("<pkg>")` (release), run it on
# wasmtime and in node; one line per package: build rc/time/RSS, size, wasmtime compile time, node vs wasm.
S=$(cd "$(dirname "$0")/.." && pwd); export PATH=~/tools/bin:$PATH; cd $S/zp/patched; mkdir -p $S/t62/pkg
pkgs="$@"; [ -n "$pkgs" ] || pkgs=$(for d in node_modules/* node_modules/@*/*; do [ -f $d/package.json ] && echo ${d#node_modules/}; done)
for p in $pkgs; do
  n=${p//\//_}; rm -f $S/t62/pkg/$n.wasm
  echo "require(\"$p\"); console.log(\"loaded\");" > t62-$n.js
  r=$($S/t62/measure.sh $S/t62/pkg/$n.log ${JS2WASI3:-js2wasi3} build --command t62-$n.js -o $S/t62/pkg/$n.wasm)
  if [ -f $S/t62/pkg/$n.wasm ]; then
    size=$(stat -c %s $S/t62/pkg/$n.wasm)
    c=$($S/t62/measure.sh /dev/null wasmtime compile $S/t62/pkg/$n.wasm -o $S/t62/pkg/$n.cwasm | sed -E 's/.*wall_ms=([0-9]+).*/\1/')
    timeout 20 wasmtime run -Sp3 --allow-precompiled --dir $S::$S $S/t62/pkg/$n.cwasm > $S/t62/pkg/$n.run 2>&1; wrc=$?
    timeout 20 node t62-$n.js > $S/t62/pkg/$n.node 2>&1; nrc=$?
    same=$(cmp -s $S/t62/pkg/$n.run $S/t62/pkg/$n.node && echo same || echo DIFF)
    first=$(grep -v "^loaded$" $S/t62/pkg/$n.run | head -1 | cut -c1-140)
    rm -f $S/t62/pkg/$n.cwasm
    echo "$p | $r | size=$size compile_ms=$c | node=$nrc wasm=$wrc $same | $first"
  else
    first=$(grep -m1 -E "error:|panicked" $S/t62/pkg/$n.log | sed "s|$S/zp/patched/||" | cut -c1-160)
    echo "$p | $r | size=- | $first"
  fi
  rm -f t62-$n.js
done
