#!/usr/bin/env python3
"""T74: a perf profile of a js2wasi3 program (wasmtime --profile=perfmap) by category, the top items of one,
and the immediate callers of chosen functions.
Usage: perf script -i perf.data -F ip,sym > ps.txt; perf-cats.py ps.txt [function ...]"""
import collections, re, sys

stacks, cur = [], []
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        if cur:
            stacks.append(cur)
        cur = []
        continue
    parts = line.split(None, 1)
    cur.append(re.sub(r'wasm\[0\]::function\[\d+\]::', '', parts[1] if len(parts) > 1 else '?'))
if cur:
    stacks.append(cur)
tot = len(stacks) or 1
PROPS = {'get', 'get_named', 'find_own', 'find_own_named', 'lookup', 'lookup_named', 'own_get', 'string_hash',
         'string_eq', 'to_property_key', 'property_value', 'ic_store', 'get_length', 'set', 'set_named',
         'ordinary_set', 'ordinary_set_named', 'define', 'define_named', 'append_named', 'define_property',
         'array_own_get', 'array_index', 'grow', 'reindex', 'alloc_object', 'new_object', 'holder', 'has_property'}
OPS = {'strict_eq', 'loose_eq', 'to_string', 'to_number', 'to_numeric', 'to_boolean', 'box_number', 'is_number',
       'js_add', 'js_less', 'typeof', 'typeof_string', 'numeric', 'concat', 'is_nullish', 'to_primitive'}

def cat(sym):
    if re.search(r'gc7enabled|Copying|GarbageCollection|gc_runtime|trace_vmctx|VMGcHeader|type_registry|is_subtype|'
                 r'memmove|memset|from_wasm_type|engine_type_index', sym):
        return 'GC + wasmtime type checks'
    if sym in PROPS:
        return 'property access + object building (T74)'
    if sym in OPS:
        return 'conversions/operators/boxing (T75)'
    if sym.startswith('$$re'):
        return 'regex VM'
    if sym.startswith('$$json'):
        return 'JSON (prelude)'
    if sym.startswith('$$'):
        return 'other prelude'
    return 'compiled program + other'

own = collections.Counter(st[0] for st in stacks)
cats = collections.Counter()
for k, v in own.items():
    cats[cat(k)] += v
for k, v in cats.most_common():
    print(f"{100 * v / tot:5.1f}%  {k}")
for c in ('property access + object building (T74)', 'conversions/operators/boxing (T75)'):
    print(f"--- {c}:", ', '.join(f"{k} {100 * v / tot:.1f}" for k, v in own.most_common(80) if cat(k) == c))
for target in sys.argv[2:]:
    callers = collections.Counter(st[1] if len(st) > 1 else '?' for st in stacks if st[0] == target)
    n = sum(callers.values())
    print(f"== {target} {100 * n / tot:.1f}%: callers", ', '.join(f"{k} {100 * v / tot:.1f}" for k, v in callers.most_common(8)))
