#!/bin/bash
# T64: patch (t64/patch.sh) (level 2), build main.js, compile with wasmtime, run against a fresh data dir. Prints measurements.
# T64 usage: copy tasks/T62-files to $S/t62 (run its setup.sh) and this directory to $S/t64, then T63_GLOBALS=0 $S/t64/build-run.sh.
S=$(cd "$(dirname "$0")/.." && pwd); export PATH=~/tools/bin:$PATH
T62_LEVEL=2 $S/t64/patch.sh >/dev/null || exit 1
cd $S/zp/patched
echo "build: $($S/t62/measure.sh $S/build-p2.log ${JS2WASI3:-js2wasi3} build --command main.js ${T62_INCLUDE:+--include "$T62_INCLUDE"} -o $S/p2.wasm) size=$(stat -c %s $S/p2.wasm)"
grep -E "error:|panicked" $S/build-p2.log | head -5
echo "compile: $($S/t62/measure.sh $S/wt-compile.log wasmtime compile $S/p2.wasm -o $S/p2.cwasm)"
rm -rf $S/data; cp -r $S/t62/data $S/data; cd $S
echo "run: $($S/t62/measure.sh $S/run-p2.log timeout 120 wasmtime run -Sp3 -S inherit-network --allow-precompiled --dir $S::$S --env ZIGBEE2MQTT_DATA=$S/data --env Z2M_ONBOARD_NO_SERVER=1 $S/p2.cwasm)"
sed "s|$S|\$S|g" $S/run-p2.log | head -c 4000
