#!/usr/bin/env bash
# Differential tests: run each JS fixture with node (the oracle) and as a js2wasi3 command
# (`js2wasi3 build` + `wasmtime run -Sp3`), then compare stdout, stderr and the exit status.
#
# Usage: scripts/diff.sh [-j jobs] [fixture.js | dir]...
#   (default: tests/fixtures and its subdirectories, except unsupported/ which holds rejection tests)
#   A dir means its *.js files (not subdirectories); in a dir named `modules` or `commonjs` (T54), each `<case>/main.js`
#   is the entry of a multi-file program (T41). `x.skip` next to `x.js` skips it; its
#   first line is the reason. Each fixture should take under 2 s: slower ones are flagged.
#   `x.flags` next to `x.js`: extra build options (T41: `--include`), from the fixture's directory.
#   `x.args` next to `x.js` (T55): the program's arguments (one line, shell words), given to both.
#   `x.env` next to `x.js` (T55): `NAME=value` lines, set for node and passed to wasmtime (`--env`);
#   wasmtime passes no other variables, so such a fixture prints only these.
#   `x.fs/` next to `x.js` (T59): a directory copied fresh for each run; node runs in its copy, and
#   wasmtime gets its copy preopened as `/` (`--dir`), the program's working directory.
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

# T78: node's results are cached in out/node-cache/<key>/ (node.out, node.err, node.status): node runs
# only when the key changes. The key hashes the node version, this script's way of running node
# (NODE_RUN), the fixture, its .args/.env/.uncaught files and .fs directory, its directory's
# package.json and, for a multi-file program (<dir>/<case>/main.js), every file of the case.
# DIFF_NODE_CACHE=0: always run node (and refresh the cache).
NODE_CACHE=$ROOT/out/node-cache
NODE_RUN="v1 TZ=UTC --no-warnings uncaught-reduced"
node_key() {
  local f=$1 dir; dir=$(dirname "$f")
  {
    echo "$NODE_VERSION $NODE_RUN"
    sha256sum <"$f"
    for x in args env uncaught; do
      [ -f "${f%.js}.$x" ] && { echo "$x"; sha256sum <"${f%.js}.$x"; }
    done
    [ -f "$dir/package.json" ] && sha256sum <"$dir/package.json"
    [ -d "${f%.js}.fs" ] && (cd "${f%.js}.fs" && find . -type f | LC_ALL=C sort | xargs -r -d '\n' sha256sum)
    if [ "$(basename "$f")" = main.js ]; then
      case "$(basename "$(dirname "$dir")")" in
        modules | commonjs) (cd "$dir" && find . -type f | LC_ALL=C sort | xargs -r -d '\n' sha256sum) ;;
      esac
    fi
  } | sha256sum | cut -c1-32
}

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
  local args=() envs=() wenv=()
  if [ -f "${f%.js}.args" ]; then
    eval "args=($(head -1 "${f%.js}.args"))"
  fi
  if [ -f "${f%.js}.env" ]; then
    while IFS= read -r line; do
      [ -n "$line" ] && envs+=("$line") && wenv+=(--env "$line")
    done <"${f%.js}.env"
  fi
  # --no-warnings: process warnings (node 24: TimeoutNegativeWarning, …) carry the pid.
  # TZ=UTC: js2wasi3's local time is UTC (T26: WASI 0.3.1 has no stable time zone interface).
  local ncwd=. wdir=()
  if [ -d "${f%.js}.fs" ]; then
    rm -rf "$d/node.fs" "$d/wasm.fs"
    cp -r "${f%.js}.fs" "$d/node.fs" && cp -r "${f%.js}.fs" "$d/wasm.fs"
    ncwd="$d/node.fs" wdir=(--dir "$d/wasm.fs::/")
  fi
  local key cached
  key=$(node_key "$f")
  cached=$NODE_CACHE/$key
  if [ "${DIFF_NODE_CACHE:-1}" != 0 ] && [ -f "$cached/node.status" ]; then
    cp "$cached/node.out" "$cached/node.err" "$cached/node.status" "$d/"
  else
    (cd "$ncwd" && env -u FORCE_COLOR TZ=UTC "${envs[@]}" timeout "$TIMEOUT" node --no-warnings "$f" "${args[@]}") >"$d/node.out" 2>"$d/node.err"
    echo $? >"$d/node.status"
    if [ -f "${f%.js}.uncaught" ]; then
      mv "$d/node.err" "$d/node.err.raw"
      node "$ROOT/scripts/node-uncaught.mjs" "$d/node.err.raw" >"$d/node.err"
    fi
    # Not a result of the timeout (status 124): that one is node's speed, not its answer.
    if [ "$(cat "$d/node.status")" != 124 ]; then
      mkdir -p "$NODE_CACHE"
      local tmp; tmp=$(mktemp -d "$NODE_CACHE/.tmp.XXXXXX")
      cp "$d/node.out" "$d/node.err" "$d/node.status" "$tmp/" && mv -T "$tmp" "$cached" 2>/dev/null || rm -rf "$tmp"
    fi
  fi
  # `x.flags` next to `x.js`: more build options (one line, e.g. `--include 'lib/*.js'`), used
  # from the fixture's directory.
  local flags=()
  if [ -f "${f%.js}.flags" ]; then
    eval "flags=($(head -1 "${f%.js}.flags"))"
  fi
  if ! (cd "$(dirname "$f")" && "$BIN" build "$f" -o "$d/x.wasm" "${flags[@]}") 2>"$d/build.err"; then
    echo "FAIL $(($(now_ms) - t0)) build failed" >"$res"
    cat "$d/build.err" >"$diff"
    return
  fi
  # Sockets (T66): TCP and name lookup on the host network, like node.
  timeout "$TIMEOUT" wasmtime run -Sp3 -S inherit-network -S allow-ip-name-lookup -S http "${wenv[@]}" "${wdir[@]}" "$d/x.wasm" "${args[@]}" >"$d/wasm.out" 2>"$d/wasm.err"
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
    # A multi-file program (T41): <dir>/<case>/main.js; its other files aren't fixtures.
    for f in "$p"/*/main.js; do [ -e "$f" ] && case "$(basename "$p")" in modules | commonjs) files+=("$f") ;; esac; done
  elif [ -f "$p" ]; then
    files+=("$p")
  else
    echo "no such fixture or directory: $arg" >&2
    exit 2
  fi
done
[ ${#files[@]} -eq 0 ] && { echo "no fixtures" >&2; exit 2; }

(cd "$ROOT" && cargo build -q -p js2wasi3) || exit 2
NODE_VERSION=$(node --version)
export NODE_VERSION

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
