#!/bin/bash
# T62: zigbee-herdsman-converters with N of its 390 device modules (the rest deleted from a copy): build time,
# peak RSS, .wasm size, wasmtime compile time and RSS, and the device files' JS bytes.
S=$(cd "$(dirname "$0")/.." && pwd); export PATH=~/tools/bin:$PATH
rm -rf $S/zp/scale; cp -r $S/zp/patched $S/zp/scale; cd $S/zp/scale
D=node_modules/zigbee-herdsman-converters/dist/devices
echo 'require("zigbee-herdsman-converters"); console.log("loaded");' > t62-scale.js
for n in 390 200 100 50 0; do
  ls $D/*.js | sort | tail -n +$((n+1)) | xargs -r rm -f
  bytes=$(cat $D/*.js 2>/dev/null | wc -c)
  b=$($S/t62/measure.sh $S/t62/scale-$n.log ${JS2WASI3:-js2wasi3} build --command t62-scale.js --include "node_modules/zigbee-herdsman-converters/dist/devices/*.js" -o $S/t62/scale.wasm)
  c=$($S/t62/measure.sh /dev/null wasmtime compile $S/t62/scale.wasm -o $S/t62/scale.cwasm)
  echo "N=$n device_js_bytes=$bytes | build $b | size=$(stat -c %s $S/t62/scale.wasm) | compile $c"
done
rm -f $S/t62/scale.wasm $S/t62/scale.cwasm
