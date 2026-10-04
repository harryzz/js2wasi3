#!/bin/bash
# T79: record a production session once, for offline replays. Run on the laptop AFTER the user has stopped
# production's zigbee2mqtt (opi5plus); restart production afterwards (the user does).
#   tasks/T79-files/record-prod.sh [seconds]          (default 900)
# 1. snapshot of production's data dir (read only over ssh; refuses if database.db changed in the last minute),
# 2. zigbee2mqtt 2.14.1 under node (~/js2wasi3-prod-run/zp/package) on a fresh copy of it, as production runs it
#    (overrides as in the 2026-10-04 production runs: frontend 0.0.0.0:18080 without auth_token, log level info,
#    TZ=Europe/Sofia), but through two recording proxies (tcp-record.cjs):
#      coordinator 127.0.0.1:16638 -> 192.168.1.104:6638 (SLZB-06P7)     -> zigbee.jsonl
#      MQTT broker 127.0.0.1:11883 -> 192.168.1.18:1883                  -> mqtt.jsonl
# 3. CPU/RSS of node every 5 s (../sample.sh) -> cpu.csv.
# Writes ~/js2wasi3-prod-run/rec-<date>/: snapshot/ (read only), data/ (after the run), z2m.log, zigbee.jsonl,
# mqtt.jsonl, cpu.csv.
set -u
T=${1:-900}
W=~/js2wasi3-prod-run
D=$W/rec-$(date +%Y%m%d-%H%M)
F=$(cd "$(dirname "$0")" && pwd)
H=harry@opi5plus.zaharievi.internal; P=/opt/containers/zigbee2mqtt/data
mkdir -p $D
age=$(ssh $H "echo \$(( \$(date +%s) - \$(stat -c %Y $P/database.db) ))")
[ "$age" -ge 60 ] || { echo "database.db written ${age}s ago: is production really stopped?"; exit 1; }
rsync -a --exclude /log/ $H:$P/ $D/snapshot/ && chmod -R a-w $D/snapshot && echo "snapshot: $(ls $D/snapshot | tr '\n' ' ')"
cp -r $D/snapshot $D/data && chmod -R u+w $D/data
sed -i '/^  auth_token:/d' $D/data/configuration.yaml
grep -q "port: tcp://192.168.1.104:6638" $D/data/configuration.yaml || { echo "unexpected serial port in the snapshot"; exit 1; }

node $F/tcp-record.cjs 16638 192.168.1.104:6638 $D/zigbee.jsonl 2>>$D/proxies.log & P1=$!
node $F/tcp-record.cjs 11883 192.168.1.18:1883 $D/mqtt.jsonl 2>>$D/proxies.log & P2=$!
sleep 1
ENV=(ZIGBEE2MQTT_DATA=$D/data ZIGBEE2MQTT_CONFIG_SERIAL_PORT=tcp://127.0.0.1:16638
     ZIGBEE2MQTT_CONFIG_MQTT_SERVER=mqtt://127.0.0.1:11883 ZIGBEE2MQTT_CONFIG_FRONTEND_HOST=0.0.0.0
     ZIGBEE2MQTT_CONFIG_FRONTEND_PORT=18080 ZIGBEE2MQTT_CONFIG_ADVANCED_LOG_LEVEL=info Z2M_ONBOARD_NO_SERVER=1
     TZ=Europe/Sofia)
$F/sample.sh node $D/cpu.csv &
S=$!
echo "recording for ${T}s into $D"
(cd $W/zp/package && env -u FORCE_COLOR "${ENV[@]}" timeout $T node index.js >$D/z2m.log 2>&1)
echo "zigbee2mqtt rc=$?"
sleep 2
kill $P1 $P2 $S 2>/dev/null
wait
wc -l $D/zigbee.jsonl $D/mqtt.jsonl
grep -cE "\] .*(error|warn)" $D/z2m.log | sed 's/^/warn+error lines: /'
