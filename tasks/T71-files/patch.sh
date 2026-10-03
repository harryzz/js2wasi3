#!/bin/bash
# T71: T66's patch.sh (T62's survey patches) without the srvx stub, and the frontend package imported statically.
# Usage: as T66's ($S/t62, $S/t66, $S/t71 copies of the task directories), T62_LEVEL=2 T63_GLOBALS=0 $S/t71/patch.sh
set -e
S=$(cd "$(dirname "$0")/.." && pwd)
$S/t66/patch.sh
cd $S/zp/patched
# srvx is real now (node:http's server, the Fetch globals): zigbee2mqtt's own files again.
cp $S/zp/package/dist/util/staticFileServer.js dist/util/staticFileServer.js
cp $S/zp/package/node_modules/srvx/dist/body-limit.mjs node_modules/srvx/dist/body-limit.mjs
# The frontend package by name (a computed import() needs its module at build time): the default package.
sed -i 's#const frontend = (await import(settings.get().frontend.package));#const frontend = (await import("zigbee2mqtt-windfront")); // T71: settings.get().frontend.package#' dist/extension/frontend.js
grep -q 'T71: settings' dist/extension/frontend.js
echo "T71: srvx unstubbed, frontend import static"
