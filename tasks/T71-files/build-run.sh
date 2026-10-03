#!/bin/bash
# T71: zigbee2mqtt with its frontend on (data/configuration.yaml: test-only topics and network, frontend on
# 127.0.0.1:18080) under node and under js2wasi3 + wasmtime; client.cjs fetches pages and opens the WebSocket
# once "Started frontend" is logged. Usage: copy tasks/T62-files to $S/t62 (run its setup.sh), T66-files to $S/t66,
# T68-files to $S/t68 and this directory to $S/t71, then
#   Z2M_SERIAL=tcp://<host>:6638 Z2M_MQTT=mqtt://<host>:1883 [T71_BUILD=0] [Z2M_DEVICES=1] T63_GLOBALS=0 $S/t71/build-run.sh
# Writes $S/node-run.log, $S/node-client.log, $S/run-p2.log, $S/wasm-client.log.
S=$(cd "$(dirname "$0")/.." && pwd); export PATH=~/tools/bin:$PATH
: "${Z2M_SERIAL:?set Z2M_SERIAL}" "${Z2M_MQTT:?set Z2M_MQTT}"
GC=(-O gc-heap-initial-size=${Z2M_GC_HEAP:-268435456})
ENV=(ZIGBEE2MQTT_CONFIG_SERIAL_PORT=$Z2M_SERIAL ZIGBEE2MQTT_CONFIG_MQTT_SERVER=$Z2M_MQTT Z2M_ONBOARD_NO_SERVER=1)
T=${T71_TIMEOUT:-90}
# Runs "$@" in the background with its log in $1, waits for the frontend, runs the client into $2, waits.
run() {
  local log=$1 client=$2; shift 2
  "$@" >"$log" 2>&1 &
  local pid=$!
  for _ in $(seq 1 $((T * 2))); do
    grep -q "Started frontend" "$log" && break
    kill -0 $pid 2>/dev/null || break
    sleep 0.5
  done
  if grep -q "Started frontend" "$log"; then
    sleep 2
    node $S/t71/client.cjs 18080 $S/zp/package/node_modules/ws >"$client" 2>&1
  else
    echo "no frontend" >"$client"
  fi
  wait $pid
}
rm -rf $S/data-node && cp -r $S/t71/data $S/data-node
(cd $S/zp/package && run $S/node-run.log $S/node-client.log env "${ENV[@]}" ZIGBEE2MQTT_DATA=$S/data-node timeout $T node index.js)
echo "node: $(grep -cE 'error' $S/node-run.log) error lines; client: $(tr '\n' '|' <$S/node-client.log | cut -c1-200)"
if [ "${T71_BUILD:-1}" = 1 ]; then
  T62_LEVEL=2 $S/t71/patch.sh >/dev/null || exit 1
  INC=(); [ "${Z2M_DEVICES:-0}" = 1 ] && INC=(--include 'node_modules/zigbee-herdsman-converters/dist/devices/*.js')
  (cd $S/zp/patched && echo "build: $($S/t62/measure.sh $S/build-p2.log ${JS2WASI3:-js2wasi3} build --command main.js "${INC[@]}" -o $S/p2.wasm)")
  grep -E "error:|panicked" $S/build-p2.log | head -5
  echo "compile: $($S/t62/measure.sh $S/wt-compile.log wasmtime compile "${GC[@]}" $S/p2.wasm -o $S/p2.cwasm)"
fi
rm -rf $S/data && cp -r $S/t71/data $S/data
WENV=(); for e in "${ENV[@]}"; do WENV+=(--env "$e"); done
cd $S && run $S/run-p2.log $S/wasm-client.log timeout $T wasmtime run "${GC[@]}" -Sp3 -S inherit-network -S allow-ip-name-lookup -S http --allow-precompiled --dir $S::$S --env ZIGBEE2MQTT_DATA=$S/data "${WENV[@]}" $S/p2.cwasm
echo "wasm client: $(tr '\n' '|' <$S/wasm-client.log | cut -c1-200)"
diff $S/node-client.log $S/wasm-client.log && echo "client logs equal"
grep -E 'info|warn|error|^\s+at ' $S/run-p2.log | grep -v 'zh:zstack:unpi' | sed "s|$S|\$S|g" | tail -30
