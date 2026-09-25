#!/usr/bin/env bash
# Differential tests: run each JS fixture with node (the oracle) and as a js2wasi3 command
# (`js2wasi3 build` + `wasmtime run -Sp3`), then compare stdout, stderr and the exit status.
#
# Usage: scripts/diff.sh [-j jobs] [fixture.js | dir]...
#   (default: tests/fixtures and its subdirectories, except unsupported/ which holds rejection tests)
#   A dir means its *.js files (not subdirectories). `x.skip` next to `x.js` skips it; its
#   first line is the reason. Each fixture should take under 2 s: slower ones are flagged.
#   `x.uncaught` next to `x.js`: the program ends with an uncaught exception. node's stderr (source
#   excerpt and stack) is first reduced to `Uncaught <message>`, as a js2wasi3 command prints it
#   (scripts/node-uncaught.mjs).
# Exit status: 0 if nothing failed, 1 otherwise.
set -u

ROOT=$(cd "$(dirname "$0")/.." && pwd)
BIN="$ROOT/target/debug/js2wasi3"
SLOW_MS=2000
TIMEOUT=10s

now_ms() { echo $(($(date +%s%N) / 1000000)); }

# One fixture: writes <work>/<id>.result ("PASS ms" / "FAIL ms reason" / "SKIP reason") and <id>.diff.
one() {
  local f=$1 work=$2
  local id; id=$(echo "${f#"$ROOT"/}" | tr '/' '_')
  local d="$work/$id.d" res="$work/$id.result" diff="$work/$id.diff"
  mkdir -p "$d"
  if [ -f "${f%.js}.skip" ]; then
    echo "SKIP $(head -1 "${f%.js}.skip")" >"$res"
    return
  fi
  local t0; t0=$(now_ms)
  timeout "$TIMEOUT" node "$f" >"$d/node.out" 2>"$d/node.err"
  echo $? >"$d/node.status"
  if [ -f "${f%.js}.uncaught" ]; then
    mv "$d/node.err" "$d/node.err.raw"
    node "$ROOT/scripts/node-uncaught.mjs" "$d/node.err.raw" >"$d/node.err"
  fi
  if ! "$BIN" build "$f" -o "$d/x.wasm" 2>"$d/build.err"; then
    echo "FAIL $(($(now_ms) - t0)) build failed" >"$res"
    cat "$d/build.err" >"$diff"
    return
  fi
  timeout "$TIMEOUT" wasmtime run -Sp3 "$d/x.wasm" >"$d/wasm.out" 2>"$d/wasm.err"
  echo $? >"$d/wasm.status"
  local ms=$(($(now_ms) - t0))
  local what=()
  for k in out err status; do
    if ! cmp -s "$d/node.$k" "$d/wasm.$k"; then
      what+=("$k")
      diff -u --label "node $k" --label "js2wasi3 $k" "$d/node.$k" "$d/wasm.$k" >>"$diff"
    fi
  done
  if [ ${#what[@]} -eq 0 ]; then
    echo "PASS $ms" >"$res"
  else
    echo "FAIL $ms ${what[*]} differ" >"$res"
  fi
}

if [ "${1:-}" = "--one" ]; then
  one "$2" "$3"
  exit 0
fi

jobs=$(nproc 2>/dev/null || echo 4)
if [ "${1:-}" = "-j" ]; then
  jobs=$2
  shift 2
fi
if [ $# -eq 0 ]; then
  set -- "$ROOT/tests/fixtures"
  for d in "$ROOT"/tests/fixtures/*/; do
    [ "$(basename "$d")" != unsupported ] && set -- "$@" "${d%/}"
  done
fi

files=()
for arg in "$@"; do
  p=$(realpath "$arg")
  if [ -d "$p" ]; then
    for f in "$p"/*.js; do [ -e "$f" ] && files+=("$f"); done
  elif [ -f "$p" ]; then
    files+=("$p")
  else
    echo "no such fixture or directory: $arg" >&2
    exit 2
  fi
done
[ ${#files[@]} -eq 0 ] && { echo "no fixtures" >&2; exit 2; }

(cd "$ROOT" && cargo build -q -p js2wasi3) || exit 2

work=$(mktemp -d "${TMPDIR:-/tmp}/js2wasi3-diff.XXXXXX")
trap 'rm -rf "$work"' EXIT
printf '%s\0' "${files[@]}" | xargs -0 -P "$jobs" -I{} "$0" --one {} "$work"

pass=0 fail=0 skip=0 slow=0
for f in "${files[@]}"; do
  rel=${f#"$ROOT"/}
  id=$(echo "$rel" | tr '/' '_')
  read -r kind rest <"$work/$id.result"
  case $kind in
    PASS)
      pass=$((pass + 1))
      flag=""
      [ "$rest" -ge "$SLOW_MS" ] && { flag="  SLOW (over ${SLOW_MS} ms)"; slow=$((slow + 1)); }
      printf 'PASS  %-40s %5s ms%s\n' "$rel" "$rest" "$flag" ;;
    SKIP)
      skip=$((skip + 1))
      printf 'SKIP  %-40s %s\n' "$rel" "$rest" ;;
    FAIL)
      fail=$((fail + 1))
      read -r ms reason <<<"$rest"
      printf 'FAIL  %-40s %5s ms  %s\n' "$rel" "$ms" "$reason"
      sed 's/^/      /' "$work/$id.diff" | head -60 ;;
  esac
done
echo "$pass passed, $fail failed, $skip skipped${slow:+, $slow slow}" | sed 's/, 0 slow//'
[ $fail -eq 0 ]
