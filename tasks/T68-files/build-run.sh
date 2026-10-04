#!/bin/bash
# T68: T66's build (no stubs) run against a live coordinator and MQTT broker, and node on the same config.
# Usage: copy tasks/T62-files to $S/t62 (run its setup.sh), tasks/T66-files to $S/t66 and this directory to
# $S/t68, then Z2M_SERIAL=tcp://<host>:6638 Z2M_MQTT=mqtt://<host>:1883 [T68_BUILD=0] T63_GLOBALS=0 $S/t68/build-run.sh
# Writes $S/node-run.log and $S/run-p2.log (fresh data dirs; the coordinator is reset by each run's network start).
S=$(cd "$(dirname "$0")/.." && pwd); export PATH=~/tools/bin:$PATH
: "${Z2M_SERIAL:?set Z2M_SERIAL}" "${Z2M_MQTT:?set Z2M_MQTT}"
# T69: a bigger initial GC heap (wasmtime 49's copying collector thrashes with zigbee2mqtt's live set
# in its default heap; 64 MiB isn't enough; T79: 512 MiB, 3.7x less CPU than 256 MiB, F347). Compile and run must
# agree; Z2M_GC_HEAP=0 gives wasmtime's default.
GC=(-O gc-heap-initial-size=${Z2M_GC_HEAP:-536870912})
ENV=(ZIGBEE2MQTT_CONFIG_SERIAL_PORT=$Z2M_SERIAL ZIGBEE2MQTT_CONFIG_MQTT_SERVER=$Z2M_MQTT Z2M_ONBOARD_NO_SERVER=1)
rm -rf $S/data-node && cp -r $S/t68/data $S/data-node
(cd $S/zp/package && env "${ENV[@]}" ZIGBEE2MQTT_DATA=$S/data-node timeout ${T68_TIMEOUT:-120} node index.js >$S/node-run.log 2>&1)
echo "node: rc=$? $(grep -cE 'error' $S/node-run.log) error lines"
if [ "${T68_BUILD:-1}" = 1 ]; then
  T62_LEVEL=2 $S/t66/patch.sh >/dev/null || exit 1
  # Z2M_DEVICES=1: with zigbee-herdsman-converters' 390 device modules (F272: needed once a device is joined, else
  # the start fails with "Cannot find module './devices/<vendor>'"); ~4.5 min, 60 MB, compile ~2 min / 9 GB.
  INC=(); [ "${Z2M_DEVICES:-0}" = 1 ] && INC=(--include 'node_modules/zigbee-herdsman-converters/dist/devices/*.js')
  (cd $S/zp/patched && echo "build: $($S/t62/measure.sh $S/build-p2.log ${JS2WASI3:-js2wasi3} build --command main.js "${INC[@]}" -o $S/p2.wasm)")
  grep -E "error:|panicked" $S/build-p2.log | head -5
  echo "compile: $($S/t62/measure.sh $S/wt-compile.log wasmtime compile "${GC[@]}" $S/p2.wasm -o $S/p2.cwasm)"
fi
rm -rf $S/data && cp -r $S/t68/data $S/data
WENV=(); for e in "${ENV[@]}"; do WENV+=(--env "$e"); done
cd $S && echo "run: $($S/t62/measure.sh $S/run-p2.log timeout ${T68_TIMEOUT:-120} wasmtime run "${GC[@]}" -Sp3 -S inherit-network -S allow-ip-name-lookup -S http --allow-precompiled --dir $S::$S --env ZIGBEE2MQTT_DATA=$S/data "${WENV[@]}" $S/p2.cwasm)"
grep -E 'info|warn|error|^\s+at ' $S/run-p2.log | grep -v 'zh:zstack:unpi' | sed "s|$S|\$S|g" | tail -30
