#!/bin/bash
# T68: start zigbee2mqtt ($1 = node | wasm), request a restart on the test topic once started, stop after the second start.
# Usage (layout as build-run.sh; needs build-run.sh's $S/p2.cwasm): Z2M_SERIAL=tcp://<host>:6638 Z2M_MQTT=mqtt://<host>:1883 $S/t68/restart-run.sh node|wasm
# Writes $S/restart-<node|wasm>.log.
S=$(cd "$(dirname "$0")/.." && pwd); W=$1; L=$S/restart-$W.log
H=${Z2M_MQTT#mqtt://}; H=${H%:*}
# T69: the GC heap p2.cwasm was compiled with (build-run.sh's Z2M_GC_HEAP).
GC=(-O gc-heap-initial-size=${Z2M_GC_HEAP:-268435456})
ENV=(ZIGBEE2MQTT_CONFIG_SERIAL_PORT=$Z2M_SERIAL ZIGBEE2MQTT_CONFIG_MQTT_SERVER=$Z2M_MQTT Z2M_ONBOARD_NO_SERVER=1)
rm -rf $S/data-r-$W && cp -r $S/t68/data $S/data-r-$W
if [ $W = node ]; then
  (cd $S/zp/package && exec env "${ENV[@]}" ZIGBEE2MQTT_DATA=$S/data-r-$W timeout 150 node index.js) >$L 2>&1 &
else
  WENV=(); for e in "${ENV[@]}"; do WENV+=(--env "$e"); done
  (cd $S && exec timeout 150 wasmtime run "${GC[@]}" -Sp3 -S inherit-network -S allow-ip-name-lookup -S http --allow-precompiled --dir $S::$S --env ZIGBEE2MQTT_DATA=$S/data-r-$W "${WENV[@]}" $S/p2.cwasm) >$L 2>&1 &
fi
P=$!
until [ $(grep -c 'Zigbee2MQTT started' $L) -ge 1 ] || ! kill -0 $P 2>/dev/null; do sleep 1; done; sleep 8
echo "restart request at $(date +%T)"; mosquitto_pub -h $H -t zigbee2mqtt-js2wasi3/bridge/request/restart -m '' 2>&1 | head -2
until [ $(grep -c 'Zigbee2MQTT started' $L) -ge 2 ] || ! kill -0 $P 2>/dev/null; do sleep 1; done; sleep 8
kill $P 2>/dev/null; wait $P; echo "$W rc=$? started=$(grep -c 'Zigbee2MQTT started' $L)"
