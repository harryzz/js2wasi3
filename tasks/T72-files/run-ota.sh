#!/bin/bash
# T72: ota-index.cjs under node and js2wasi3 + wasmtime, from zigbee2mqtt's package directory (T62's setup: $Z2M/package).
# Usage: Z2M=<dir with package/> tasks/T72-files/run-ota.sh   (writes $Z2M/ota-node.log, $Z2M/ota-wasm.log)
set -u
: "${Z2M:?set Z2M}"; T=$(cd "$(dirname "$0")" && pwd)
cp $T/ota-index.cjs $Z2M/package/ota-index.cjs && cd $Z2M/package || exit 1
node ota-index.cjs >$Z2M/ota-node.log 2>&1; echo "node rc=$?"
${JS2WASI3:-js2wasi3} build ota-index.cjs -o $Z2M/ota.wasm || exit 1
wasmtime run -Sp3 -S http -S inherit-network -S allow-ip-name-lookup $Z2M/ota.wasm >$Z2M/ota-wasm.log 2>&1; echo "wasm rc=$?"
mask() { sed -E "s/^\[[0-9TZ:.-]+\]/[time]/" "$1"; }
diff <(mask $Z2M/ota-node.log) <(mask $Z2M/ota-wasm.log) && echo same
