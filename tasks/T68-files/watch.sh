#!/bin/bash
# T68: record MQTT during a live run ($1 = a label): the test topics in full ($S/mqtt-test-$1.log), and any message on the
# production topics that names the test instance (js2wasi3, its PAN 0x4a3c = 19004), which must stay empty
# ($S/mqtt-prod-$1.log). Usage: MQTT_HOST=<broker> $S/t68/watch.sh run1 &  (stop it with kill when the runs are done)
S=$(cd "$(dirname "$0")/.." && pwd); H=${MQTT_HOST:?set MQTT_HOST}
mosquitto_sub -h $H -v -t 'zigbee2mqtt-js2wasi3/#' -t 'homeassistant-js2wasi3/#' > $S/mqtt-test-$1.log 2>&1 &
mosquitto_sub -h $H -v -t 'zigbee2mqtt/#' -t 'homeassistant/#' 2>&1 | grep --line-buffered -iE 'js2wasi3|19004|0x4a3c' > $S/mqtt-prod-$1.log &
wait
