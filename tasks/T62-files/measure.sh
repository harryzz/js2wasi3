#!/bin/bash
# usage: measure.sh <log> <cmd...>   prints wall ms and peak RSS (MB) of cmd (sampled /proc every 50 ms)
log=$1; shift
t0=$(date +%s%N)
"$@" > "$log" 2>&1 & pid=$!
peak=0
while kill -0 $pid 2>/dev/null; do
  r=$(awk '/VmRSS/{print $2}' /proc/$pid/status 2>/dev/null); [ -n "$r" ] && [ "$r" -gt "$peak" ] && peak=$r
  sleep 0.05
done
wait $pid; rc=$?
t1=$(date +%s%N)
echo "rc=$rc wall_ms=$(( (t1-t0)/1000000 )) peak_rss_mb=$((peak/1024))"
