#!/usr/bin/env bash
# Build the zstd helper component (T150): node's zstd streams on libzstd (C, built with clang for wasm), `js2wasi3:zstd/codecs`.
# Output: crates/js2wasi3/assets/zstd-helper.wasm (checked in; rebuild after changing crates/zstd-helper).
set -euo pipefail
cd "$(dirname "$0")/.."
(cd crates/zstd-helper && CC_wasm32_unknown_unknown=clang cargo build --release --target wasm32-unknown-unknown)
mkdir -p crates/js2wasi3/assets
wasm-tools component new \
  crates/zstd-helper/target/wasm32-unknown-unknown/release/js2wasi3_zstd_helper.wasm \
  -o crates/js2wasi3/assets/zstd-helper.wasm
wasm-tools validate crates/js2wasi3/assets/zstd-helper.wasm
ls -l crates/js2wasi3/assets/zstd-helper.wasm
