#!/bin/bash
# T62: set up the zigbee2mqtt survey next to this directory ($S = its parent):
#   $S/zp/package  zigbee2mqtt 2.14.1 from npm with production deps (--ignore-scripts: no native builds)
#   $S/t62/stubs   node built-ins js2wasi3 lacks (gen-stubs.mjs) + web.js, srvx.js (checked in)
#   $S/t62/ajv     zigbee2mqtt's settings validators, ajv standalone (ajv-standalone.cjs)
# Needs node 24.21.0 and npm on PATH. Then: patch.sh, build-run.sh (JS2WASI3=<path to the release binary>).
set -e
S=$(cd "$(dirname "$0")/.." && pwd); V=${Z2M_VERSION:-2.14.1}
mkdir -p $S/zp && cd $S/zp && npm pack zigbee2mqtt@$V >/dev/null && rm -rf package && tar xzf zigbee2mqtt-$V.tgz
cd package && npm install --omit=dev --ignore-scripts >/dev/null
node $S/t62/gen-stubs.mjs $S/t62/stubs child_process crypto url dns net tls dgram http https http2 zlib perf_hooks
node $S/t62/ajv-standalone.cjs $S/zp/package $S/t62/ajv
