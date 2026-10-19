#!/bin/sh
# wo.sh <wasm-opt flags...> <in> <out>
n=$#; in=$(eval echo \${$((n-1))}); out=$(eval echo \${$n})
set -- $(echo "$@" | awk '{for(i=1;i<=NF-2;i++) printf "%s ", $i}')
exec wasm-opt "$@" "$in" -o "$out"
