#!/bin/bash
# T69: zigbee2mqtt (p67.cwasm) against the live rig under perf; $1 = label, $2 = log level (debug|info).
# Layout as tasks/T68-files (copy this to $S next to t68/); CWASM (default p67.cwasm) and WT_OPTS select the module and
# wasmtime options; Z2M_SERIAL / Z2M_MQTT from the environment. Writes $S/run-<label>.log and $S/perf-<label>.data.
S=$(cd "$(dirname "$0")" && pwd); L=$1; LVL=${2:-debug}
rm -rf $S/data-$L && cp -r $S/t68/data $S/data-$L && sed -i "s/log_level: debug/log_level: $LVL/" $S/data-$L/configuration.yaml
MQTT_HOST=${Z2M_MQTT#mqtt://}; MQTT_HOST=${MQTT_HOST%:*}
mosquitto_sub -h $MQTT_HOST -v -t 'zigbee2mqtt-js2wasi3/#' -t 'homeassistant-js2wasi3/#' > $S/mqtt-test-$L.log 2>&1 & W1=$!
mosquitto_sub -h $MQTT_HOST -v -t 'zigbee2mqtt/#' -t 'homeassistant/#' 2>/dev/null | grep --line-buffered -iE 'js2wasi3|19004|0x4a3c' > $S/mqtt-prod-$L.log & W2=$!
cd $S && timeout ${T69_TIMEOUT:-60} perf record -q -e cpu-clock -k CLOCK_REALTIME -F 1000 -g -o $S/perf-$L.data \
  wasmtime run ${WT_OPTS} -Sp3 -S inherit-network -S allow-ip-name-lookup -S http --profile=perfmap --allow-precompiled --dir $S::$S \
  --env ZIGBEE2MQTT_DATA=$S/data-$L --env ZIGBEE2MQTT_CONFIG_SERIAL_PORT=$Z2M_SERIAL --env ZIGBEE2MQTT_CONFIG_MQTT_SERVER=$Z2M_MQTT \
  --env Z2M_ONBOARD_NO_SERVER=1 $S/${CWASM:-p67.cwasm} > $S/run-$L.log 2>&1
echo "rc=$?"
kill $W1 $W2 2>/dev/null; pkill -P $$ mosquitto_sub 2>/dev/null; wait 2>/dev/null
echo "prod lines: $(wc -l < $S/mqtt-prod-$L.log)"
