#!/bin/sh
# Replace node_modules/depd with the js2wasi3 shim. Run after `npm install` (`npm run setup` does both).
set -e
cd "$(dirname "$0")/.."
test -f node_modules/depd/index.js || { echo "run npm install first"; exit 1; }
cp shims/depd.js node_modules/depd/index.js
echo "depd shim applied"
