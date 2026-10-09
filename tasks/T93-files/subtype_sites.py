# Usage: subtype_sites.py <prog.wat> <perf report symbols file>
# For each hot function (self time), count ref.test/ref.cast to $object (type 4), which call wasmtime's is_subtype
# for objects that aren't exactly $object.
import re, sys
wat, syms = sys.argv[1], sys.argv[2]
sites = {}; cur = None
for line in open(wat):
    m = re.match(r'\s*\(func \$(\S+) \(;\d+;\)', line)
    if m: cur = m.group(1); continue
    if cur and re.search(r'ref\.(test|cast) \(ref (null )?4\)', line): sites[cur] = sites.get(cur, 0) + 1
sub = 0.0; rows = []
for l in open(syms):
    p, n = l.split(' ', 1); p = float(p.rstrip('%')); n = n.strip()
    if 'is_subtype' in n or 'builtin_is_subtype' in n: sub += p
    elif n in sites and p >= 0.3: rows.append((p, n, sites[n]))
print(f"is_subtype total: {sub:.1f}%")
for p, n, c in sorted(rows, reverse=True)[:12]: print(f"  {p:5.1f}%  {n}  ({c} object test/cast)")
