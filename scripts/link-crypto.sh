#!/usr/bin/env bash
# Compose a js2wasi3 component with the crypto helper component (T149) so it runs on a stock wasmtime.
# A program that writes `pbkdf2`, `scrypt` or `hkdf` imports `js2wasi3:crypto/primitives@0.1.0`;
# `wac plug` fills that import with crates/js2wasi3/assets/crypto-helper.wasm (scripts/build-crypto-helper.sh).
#
# Usage: scripts/link-crypto.sh app.wasm [-o linked.wasm]   (default: replaces app.wasm)
# A component without that import is left as it is.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
app=${1:?usage: link-crypto.sh app.wasm [-o linked.wasm]}
out=$app
[ "${2:-}" = "-o" ] && out=${3:?-o needs a path}
if ! grep -q -a 'js2wasi3:crypto/primitives' "$app"; then
  [ "$out" = "$app" ] || cp "$app" "$out"
  exit 0
fi
tmp=$(mktemp "${out}.XXXXXX")
trap 'rm -f "$tmp"' EXIT
wac plug "$app" --plug "$ROOT/crates/js2wasi3/assets/crypto-helper.wasm" -o "$tmp"
mv "$tmp" "$out"
trap - EXIT
