#!/usr/bin/env bash
# Build the crypto helper component (T149): RustCrypto behind `js2wasi3:crypto/primitives`.
# Output: crates/js2wasi3/assets/crypto-helper.wasm (checked in; rebuild after changing crates/crypto-helper).
set -euo pipefail
cd "$(dirname "$0")/.."
(cd crates/crypto-helper && cargo build --release --target wasm32-unknown-unknown)
mkdir -p crates/js2wasi3/assets
wasm-tools component new \
  crates/crypto-helper/target/wasm32-unknown-unknown/release/js2wasi3_crypto_helper.wasm \
  -o crates/js2wasi3/assets/crypto-helper.wasm
wasm-tools validate crates/js2wasi3/assets/crypto-helper.wasm
ls -l crates/js2wasi3/assets/crypto-helper.wasm
