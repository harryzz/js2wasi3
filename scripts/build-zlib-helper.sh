#!/usr/bin/env bash
# Build the zlib helper component (T150): node's zlib binding on Rust crates, `js2wasi3:zlib/codecs`.
# Output: crates/js2wasi3/assets/zlib-helper.wasm (checked in; rebuild after changing crates/zlib-helper).
set -euo pipefail
cd "$(dirname "$0")/.."
(cd crates/zlib-helper && cargo build --release --target wasm32-unknown-unknown)
mkdir -p crates/js2wasi3/assets
wasm-tools component new \
  crates/zlib-helper/target/wasm32-unknown-unknown/release/js2wasi3_zlib_helper.wasm \
  -o crates/js2wasi3/assets/zlib-helper.wasm
wasm-tools validate crates/js2wasi3/assets/zlib-helper.wasm
ls -l crates/js2wasi3/assets/zlib-helper.wasm
