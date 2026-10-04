#!/bin/bash
# T79: one offline replay of a recording (record-prod.sh) into zigbee2mqtt under node or js2wasi3.
#   run-replay.sh <recording dir> node|wasm <out dir> [seconds after zigbee2mqtt started (default: the recording's)]
# Needs: <recording dir>/{zigbee.jsonl,mqtt.jsonl,snapshot/}; mosquitto (MOSQUITTO, default /usr/sbin/mosquitto);
# wasm: $CWASM (default ~/js2wasi3-prod-run/p.cwasm) compiled with $WASM_FLAGS (default
# "-O gc-heap-initial-size=268435456"). Parallel runs: MQTT_PORT, ZIGBEE_PORT, FRONTEND_PORT.
# The wasm run preopens ~/js2wasi3-prod-run (the build reads t62/ajv/* there by absolute path): keep <out dir> in it.
# Profiling: PERF=<perf binary> (wasm: wasmtime --profile=perfmap; perf attaches at the anchor for the steady
# window: <out dir>/perf.data, perf -e cpu-clock -g at PERF_FREQ (default 499) Hz); NODE_PROF=1 (node --cpu-prof
# into <out dir>/prof/).
# Writes <out dir>/: z2m.log, replay.log, stats.json (replayer), cpu.csv (sample.sh), result.txt (CPU summary).
set -u
REC=$(cd "$1" && pwd); V=$2; OUT=$3; SECS=${4:-}
W=~/js2wasi3-prod-run; F=$(cd "$(dirname "$0")" && pwd)
MOSQ=${MOSQUITTO:-/usr/sbin/mosquitto}; PORT=${MQTT_PORT:-18830}; ZPORT=${ZIGBEE_PORT:-16638}; FPORT=${FRONTEND_PORT:-18080}
# wasmtime's GC flags: the .cwasm must have been compiled with the same ones.
read -r -a WFLAGS <<<"${WASM_FLAGS:--O gc-heap-initial-size=268435456}"
CWASM=${CWASM:-$W/p.cwasm}
mkdir -p $OUT && OUT=$(cd $OUT && pwd)
rm -rf $OUT/data && cp -r $REC/snapshot $OUT/data && chmod -R u+w $OUT/data
sed -i '/^  auth_token:/d' $OUT/data/configuration.yaml
BASE=$(awk '/^mqtt:/{m=1} m&&/base_topic:/{print $2; exit}' $OUT/data/configuration.yaml); BASE=${BASE:-zigbee2mqtt}
# Default length: the recording's time from zigbee2mqtt's start to its stop (replay.cjs --print-steady), + 5 s.
[ -z "$SECS" ] && SECS=$(( $(Z2M_BASE_TOPIC=$BASE node $F/replay.cjs $REC --print-steady) + 5 ))
printf 'listener %s 127.0.0.1\nallow_anonymous true\npersistence false\n' $PORT >$OUT/mosquitto.conf
$MOSQ -c $OUT/mosquitto.conf >$OUT/mosquitto.log 2>&1 & MP=$!
sleep 0.5
Z2M_BASE_TOPIC=$BASE node $F/replay.cjs $REC --port $ZPORT --mqtt mqtt://127.0.0.1:$PORT --stats $OUT/stats.json >$OUT/replay.log 2>&1 & RP=$!
until grep -q "preloaded\|no retained" $OUT/replay.log 2>/dev/null; do sleep 0.2; kill -0 $RP 2>/dev/null || { cat $OUT/replay.log; exit 1; }; done

ENV=(ZIGBEE2MQTT_DATA=$OUT/data ZIGBEE2MQTT_CONFIG_SERIAL_PORT=tcp://127.0.0.1:$ZPORT
     ZIGBEE2MQTT_CONFIG_MQTT_SERVER=mqtt://127.0.0.1:$PORT ZIGBEE2MQTT_CONFIG_FRONTEND_HOST=127.0.0.1
     ZIGBEE2MQTT_CONFIG_FRONTEND_PORT=$FPORT ZIGBEE2MQTT_CONFIG_ADVANCED_LOG_LEVEL=info Z2M_ONBOARD_NO_SERVER=1 TZ=UTC)
case $V in
  node) NOPT=(); [ -n "${NODE_PROF:-}" ] && NOPT=(--cpu-prof --cpu-prof-dir=$OUT/prof)
        (cd $W/zp/package && exec env -u FORCE_COLOR "${ENV[@]}" node "${NOPT[@]}" index.js >$OUT/z2m.log 2>&1) & ZP=$!;;
  wasm) WENV=(); for e in "${ENV[@]}"; do WENV+=(--env "$e"); done
        WOPT=(); [ -n "${PERF:-}" ] && WOPT=(--profile=perfmap)
        (cd $W && exec wasmtime run "${WOPT[@]}" "${WFLAGS[@]}" -Sp3 -S inherit-network -S allow-ip-name-lookup \
          -S http --allow-precompiled --dir $W::$W "${WENV[@]}" $CWASM >$OUT/z2m.log 2>&1) & ZP=$!;;
  *) echo "variant: node | wasm"; kill $MP $RP; exit 1;;
esac
$F/sample.sh $V $OUT/cpu.csv $ZP & SP=$!
# Wait for the anchor, then the replay's length.
T0=$(date +%s)
until grep -q "anchor:" $OUT/replay.log; do
  sleep 0.5
  kill -0 $ZP 2>/dev/null || { echo "zigbee2mqtt exited before starting"; break; }
  [ $(( $(date +%s) - T0 )) -gt 600 ] && { echo "no start within 600 s"; break; }
done
cpu() { awk '{print ($14 + $15) / '"$(getconf CLK_TCK)"'}' /proc/$1/stat 2>/dev/null; }
PID=$ZP  # the subshell execs: $ZP is zigbee2mqtt's process
C0=$(cpu $PID); S0=$(date +%s.%N)
if [ -n "${PERF:-}" ]; then
  $PERF record -e cpu-clock -F ${PERF_FREQ:-499} -g -p $PID -o $OUT/perf.data -- sleep $SECS 2>$OUT/perf.log
else
  sleep $SECS
fi
C1=$(cpu $PID); S1=$(date +%s.%N)
kill -TERM $RP; sleep 1; kill $ZP $PID 2>/dev/null
# node --cpu-prof writes its profile as zigbee2mqtt exits (it stops gracefully on SIGTERM).
for i in $(seq 1 40); do kill -0 $PID 2>/dev/null || break; sleep 0.5; done
kill -9 $ZP $PID 2>/dev/null; kill $MP $SP 2>/dev/null; wait 2>/dev/null
[ -n "${PERF:-}" ] && [ -f /tmp/perf-$PID.map ] && cp /tmp/perf-$PID.map $OUT/
node -e '
const s = JSON.parse(require("fs").readFileSync(process.argv[1], "utf8"));
const [c0, c1, s0, s1] = process.argv.slice(2).map(Number);
const ev = s.unsolicitedSent;
console.log(JSON.stringify({ variant: process.argv[6], startup_s: (s.anchor - s.startedAt) / 1000, steady_s: +(s1 - s0).toFixed(1),
  cpu_startup_s: c0, cpu_steady_s: +(c1 - c0).toFixed(2), events: ev, mqtt_in: s.mqttInSent,
  ms_cpu_per_event: ev ? +((c1 - c0) * 1000 / ev).toFixed(2) : null, exact: s.matchedExact, byCommand: s.matchedByCommand,
  byName: s.matchedByName,
  unmatched: s.unmatched, unmatchedNames: s.unmatchedNames }))' $OUT/stats.json "$C0" "$C1" "$S0" "$S1" $V | tee $OUT/result.txt
