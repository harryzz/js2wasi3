#!/bin/bash
# T74: build and run bench.cjs (node and js2wasi3 + wasmtime) in a zigbee2mqtt package tree (T62's zp/patched:
# zemismart.js's TextEncoder patch), with only the 56 device modules the 52 devices load.
# Usage: Z2M=<zp/patched> [JS2WASI3=<compiler>] [OUT=<dir>] [RUNS=3] tasks/T74-files/bench.sh [node|build|run|all]
set -u
T=$(cd "$(dirname "$0")" && pwd); : "${Z2M:?set Z2M}"; OUT=${OUT:-$Z2M/../bench-out}; mkdir -p $OUT
MODS="alecto automaton avatto box bseed custom_devices_diy daewoo efekta engo essentials evanell fantem frankever futurehome
giex gledopto ikea immax iotperfect javis lellki lidl lincukoo lonsonho lumi makegood manhot mazda mercator moes mowe namron
nedis neo nous nova_digital novo onenuo orztech qa qoto rtx saswell sber senoro shelly siterwell skydance somgoms sonoff
spacetronik tech tuya weten woox xenon zemismart"
INC=(); for m in $MODS; do INC+=(--include "node_modules/zigbee-herdsman-converters/dist/devices/$m.js"); done
GC=(-O gc-heap-initial-size=268435456)
cp $T/bench.cjs $T/devices.json $Z2M/
cd $Z2M
case ${1:-all} in node|all) for i in $(seq ${RUNS:-3}); do node bench.cjs; done;; esac
case ${1:-all} in build|all)
  s=$(date +%s%N); ${JS2WASI3:-js2wasi3} build bench.cjs "${INC[@]}" -o $OUT/bench.wasm || exit 1
  m=$(date +%s%N); wasmtime compile "${GC[@]}" $OUT/bench.wasm -o $OUT/bench.cwasm || exit 1
  echo "build $(( (m - s) / 1000000 )) ms, compile $(( ($(date +%s%N) - m) / 1000000 )) ms, $(stat -c %s $OUT/bench.wasm) bytes";;
esac
case ${1:-all} in run|all) for i in $(seq ${RUNS:-3}); do wasmtime run "${GC[@]}" -Sp3 -S http -S inherit-network --allow-precompiled --dir .::/ $OUT/bench.cwasm; done;; esac
