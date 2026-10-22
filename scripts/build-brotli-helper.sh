#!/usr/bin/env bash
# Build the brotli helper component (T150): node's Brotli streams on the brotli crate, `js2wasi3:brotli/codecs`.
# Output: crates/js2wasi3/assets/brotli-helper.wasm (checked in; rebuild after changing crates/brotli-helper).
set -euo pipefail
cd "$(dirname "$0")/.."
(cd crates/brotli-helper && cargo build --release --target wasm32-unknown-unknown)
mkdir -p crates/js2wasi3/assets
wasm-tools component new \
  crates/brotli-helper/target/wasm32-unknown-unknown/release/js2wasi3_brotli_helper.wasm \
  -o crates/js2wasi3/assets/brotli-helper.wasm
wasm-tools validate crates/js2wasi3/assets/brotli-helper.wasm
ls -l crates/js2wasi3/assets/brotli-helper.wasm
