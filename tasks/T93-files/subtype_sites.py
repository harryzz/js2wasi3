# Usage: subtype_sites.py <prog.wat> <perf symbols file>
# perf symbols file: lines "<pct>% <symbol>" with the full perf symbol, e.g.
#   perf report --no-children --sort symbol -g none | grep '%' | grep -v '^#' | awk '{print $1, $3}'
# (symbols like wasm[0]::function[67744]::get). Matches wasm functions by index (names repeat: the runtime's
# `get` and a JS function `get` print as `$get` and `$"#func67744 get"`), then lists the hot functions that
# contain a ref.test/ref.cast to $object (type 4): wasmtime calls is_subtype there for any object that isn't
# exactly $object.
import re, sys
wat, syms = sys.argv[1], sys.argv[2]
sites = {}; names = {}; cur = None
for line in open(wat):
    m = re.match(r'\s*\(func \$(?:"#func\d+ (\S+?)"|(\S+)) \(;(\d+);\)', line)
    if m:
        cur = int(m.group(3)); names[cur] = m.group(1) or m.group(2); continue
    if cur is not None and re.search(r'ref\.(test|cast) \(ref (null )?4\)', line):
        sites[cur] = sites.get(cur, 0) + 1
sub = 0.0; rows = []
for l in open(syms):
    parts = l.split(None, 1)
    if len(parts) < 2: continue
    p = float(parts[0].rstrip('%')); n = parts[1].strip()
    if 'is_subtype' in n: sub += p; continue
    m = re.search(r'function\[(\d+)\]', n)
    if m and int(m.group(1)) in sites and p >= 0.3:
        i = int(m.group(1)); rows.append((p, names.get(i, n), i, sites[i]))
print(f"is_subtype total: {sub:.1f}%")
for p, n, i, c in sorted(rows, reverse=True)[:15]:
    print(f"  {p:5.1f}%  {n} (function {i}, {c} $object test/cast)")
