#!/bin/bash
# Samples a process's RSS and CPU every 5 s into a CSV: sample.sh node|wasm <out.csv> [pid]
# (time, elapsed s, rss kB, cpu s total, %cpu over the last interval). Waits for the process, stops when it exits.
name=$1; out=$2; hz=$(getconf CLK_TCK)
case $name in node) pat='^node index.js';; *) pat='^wasmtime run';; esac
P=${3:-}
until [ -n "$P" ] || { P=$(pgrep -n -f "$pat"); [ -n "$P" ]; }; do sleep 0.5; done
echo "time,elapsed_s,rss_kb,cpu_s,cpu_pct" >$out; t0=$(date +%s); prev=0
while [ -d /proc/$P ]; do
  read -r u s <<<"$(awk '{print $14, $15}' /proc/$P/stat 2>/dev/null)"; rss=$(awk '/VmRSS/ {print $2}' /proc/$P/status 2>/dev/null)
  [ -z "$rss" ] && break
  cpu=$(( (u + s) )); now=$(date +%s)
  echo "$(date +%T),$((now - t0)),$rss,$(awk -v c=$cpu -v h=$hz 'BEGIN{printf "%.2f", c/h}'),$(awk -v c=$cpu -v p=$prev -v h=$hz 'BEGIN{printf "%.1f", (c-p)/h/5*100}')" >>$out
  prev=$cpu; sleep 5
done
