#!/bin/bash
# T62: copy $S/zp/patched to $S/zp/traced with a load/done line on stderr around every .js/.cjs module
# (same line 1: line numbers kept), to find which module's top-level code throws (no stack traces yet).
S=$(cd "$(dirname "$0")/.." && pwd)
rm -rf $S/zp/traced; cp -r $S/zp/patched $S/zp/traced; cd $S/zp/traced
find . -name '*.js' -o -name '*.cjs' | grep -v '/test/' | while read f; do
  r=${f#./}
  sed -i "1s#^#console.error(\"T62 load $r\"); #" "$f"
  printf '\n;console.error("T62 done %s");\n' "$r" >> "$f"
done
