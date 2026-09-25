#!/bin/sh
# Verify pinned tool versions (TOOLS.md). Exit 1 on mismatch.
fail=0
check() { # tool expected-substring
  got=$($1 --version 2>/dev/null | head -1)
  case "$got" in *"$2"*) echo "ok   $1: $got";; *) echo "FAIL $1: want $2, got '${got:-missing}'"; fail=1;; esac
}
check wasmtime 49.0.1
check wasm-tools 1.259.0
check wkg 0.16.1
check wit-bindgen 0.62.0
check wac 0.11.0
check wasm-opt "version 133"
check rustc 1.98.1
exit $fail
