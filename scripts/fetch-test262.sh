#!/usr/bin/env bash
# Fetch test262 at the commit pinned in TOOLS.md into test262/ (gitignored): a blobless sparse clone
# with harness/ and the directories scripts/test262.mjs's subset uses. Add more with
#   git -C test262 sparse-checkout add test/<dir>
# Rerunning moves an existing clone to the pinned commit.
set -eu

ROOT=$(cd "$(dirname "$0")/.." && pwd)
COMMIT=$(sed -n 's/^| test262 | `\([0-9a-f]*\)`.*/\1/p' "$ROOT/TOOLS.md")
[ -n "$COMMIT" ] || { echo "no test262 commit in TOOLS.md" >&2; exit 2; }
DIR="$ROOT/test262"

if [ ! -d "$DIR/.git" ]; then
  git clone -q --filter=blob:none --no-checkout https://github.com/tc39/test262 "$DIR"
else
  git -C "$DIR" fetch -q origin
fi
# Set on every run, so an existing clone gets directories added to the subset (T36: Reflect).
git -C "$DIR" sparse-checkout set --cone harness \
  test/language/expressions test/language/statements \
  test/built-ins/Array test/built-ins/String test/built-ins/Math test/built-ins/JSON \
  test/built-ins/Promise test/built-ins/Map test/built-ins/Set test/built-ins/Reflect \
  test/built-ins/RegExp test/built-ins/Date test/built-ins/BigInt \
  test/language/module-code test/language/import test/language/export \
  test/built-ins/global test/language/global-code \
  test/built-ins/eval test/language/eval-code test/built-ins/Function \
  test/built-ins/DisposableStack test/built-ins/AsyncDisposableStack test/built-ins/SuppressedError \
  test/built-ins/Symbol
git -C "$DIR" checkout -q "$COMMIT"
echo "test262 at $COMMIT"
