#!/usr/bin/env python3
"""CPU by category under node and js2wasi3 from scripts/profile.sh's output (docs/performance.md).
Usage: scripts/profile-compare.py <outdir> [per] [md]
  per: the top four categories of each program; md: the tables as Markdown.
A js2wasi3 sample is charged to the helper it landed in (wasmtime --profile=perfmap names); a node sample to its leaf,
or to the JIT compiler / startup by the native frames above the first JS or builtin frame. Seconds = samples / rate."""
import collections,glob,re,os,sys
D=sys.argv[1] if len(sys.argv)>1 and os.path.isdir(sys.argv[1]) else "."
def groups(f):
    cur=[]
    for line in open(f,errors='replace'):
        if not line.strip():
            if cur: yield cur
            cur=[]; continue
        cur.append(line.rstrip('\n'))
    if cur: yield cur
def wstacks(f):  # js2wasi3: frames only (the comm/tid line dropped)
    for g in groups(f):
        if not g[0].startswith((' ','\t')): g=g[1:]
        if not g: continue
        yield [re.sub(r'wasm\[0\]::function\[\d+\]::','$',(l.strip().split(None,1)+['?'])[1]) for l in g]
def nstacks(f):  # node: first line is thread
    for g in groups(f):
        yield [g[0].split()[0]]+[(l.strip().split(None,1)+['?'])[1] for l in g[1:]]
PROPS=r'(get|get_named|find_own|find_own_named|set|set_named|lookup|lookup_named|append_named|grow|property_value|own_get|define|define_named|ordinary_set|ordinary_set_named|to_property_key|string_hash|get_length|array_own_get|array_index|reindex|holder|has_property|ic_\w+|fn_prototype)$'
def wcat(s):
    if not s.startswith('$'):
        if re.search(r'gc::|Copying|GarbageCollection|gc_ref|VMGcHeader|type_registry|trace_vmctx|RegisteredType|GcHeap|gc_runtime|memory_fill|is_subtype',s): return 'GC'
        return 'startup/host'
    n=s[1:]
    if n.startswith(('$$re','re_','$$$re')): return 'regex'
    if n.startswith('bigint'): return 'BigInt'
    if n in('box_number','to_number','numeric','is_number','to_numeric'): return 'numbers (box/unbox)'
    if re.match(PROPS,n): return 'property access'
    if re.match(r'(alloc_\w+|new_object\w*)$',n): return 'GC'
    if re.match(r'(js_\w+|loose_eq|strict_eq|to_boolean|is_nullish|same_value\w*|instanceof_value|typeof\w*|to_primitive|to_string|concat)$',n): return 'operators'
    if re.match(r'(array_\w+|typed_\w+|new_array|list_from_array_like|arg_get|make_arguments)$',n): return 'arrays/args'
    if re.match(r'(call\w*|construct\w*|create_this)$',n): return 'calls'
    if re.match(r'(string_\w+|str_\w+|number_to_string)$',n): return 'strings'
    if n.startswith('$') or re.match(r'(map_\w+|set_\w+|stream_read_done|iter_step|define_property)$',n): return 'other runtime/builtins'
    return 'JS code'
def ncat(st):
    s=st[1] if len(st)>1 else '?'
    nat=[]
    for x in st[1:]:
        if x.startswith(('JS:','Builtins_','RegExp')): break
        nat.append(x)
    full=' '.join(nat)
    if re.search(r'Scavenge|MarkCompact|Marking|Sweeper|Heap::|IteratePointers|Evacuat|MinorMS|RootScavenge|SizeFromMap|IterateObjectCache|OnMoveEvent|RecordWrite|Allocat',s): return 'GC'
    if re.search(r'compiler::|maglev::|turboshaft|Pipeline|Compile|baseline::|BytecodeGenerator|BytecodeArrayBuilder|RegisterAllocat|GraphReducer|Zone::|Assembler|SourcePosition',full) and not s.startswith('JS:'): return 'JIT compiler'
    if re.search(r'Isolate::Init|Deserializ|Bootstrap|node::Start|LoadEnvironment|ReadOnlyHeap|Scanner|Parser|CalculateLineEnds|Flag',full) and not s.startswith(('JS:','RegExp')): return 'startup/host'
    if re.search(r'RegExp|irregexp|regexp',s): return 'regex'
    if 'bigint' in s: return 'BigInt'
    if s.startswith('JS:') or s=='[unknown]' or re.search(r'Handler|_Baseline',s): return 'JS code'
    if re.search(r'(LoadIC|StoreIC|KeyedLoad|KeyedStore|DefineNamedOwn|DefineKeyedOwn|LoadGlobal|StoreGlobal|GetProperty|SetProperty|HasProperty|Megamorphic|LookupIterator|IC::|TransitionsAccessor|DescriptorArray|JSObject::|NameDictionary)',s): return 'property access'
    if re.search(r'Call|Construct|NewObject|Apply',s): return 'calls'
    if re.search(r'String|SubString|wmem|memcpy|memmove|Utf8|NumberToString|StringAdd|Flat|CopyChars|utf',s): return 'strings'
    return 'other runtime/builtins' if s.startswith(('Builtins_','v8::')) else 'startup/host'
C=['JS code','property access','numbers (box/unbox)','GC','operators','arrays/args','calls','strings','regex','BigInt','other runtime/builtins','JIT compiler','startup/host']
W={};N={}
for f in sorted(glob.glob(D+'/wasm/*.txt')):
    p=os.path.basename(f)[:-4]; c=collections.Counter(wcat(s[0]) for s in wstacks(f)); W[p]={k:v/999 for k,v in c.items()}
for f in sorted(glob.glob(D+'/node/*.txt')):
    p=os.path.basename(f)[:-4]; c=collections.Counter(ncat(s) for s in nstacks(f)); N[p]={k:v/4999 for k,v in c.items()}
P=list(W)
def tot(d,k): return sum(d[p].get(k,0) for p in P)
TW=sum(sum(d.values()) for d in W.values()); TN=sum(sum(d.values()) for d in N.values())
avgW={k:sum(100*W[p].get(k,0)/sum(W[p].values()) for p in P)/len(P) for k in C}
avgN={k:sum(100*N[p].get(k,0)/sum(N[p].values()) for p in P)/len(P) for k in C}
print(f'CPU total: js2wasi3 {TW:.1f} s, node {TN:.1f} s')
print(f"{'category':24}{'node avg%':>10}{'wasi avg%':>10}{'node s':>8}{'wasi s':>8}{'gap s':>8}")
for k in sorted(C,key=lambda k:-(tot(W,k)-tot(N,k))):
    print(f'{k:24}{avgN[k]:10.1f}{avgW[k]:10.1f}{tot(N,k):8.2f}{tot(W,k):8.2f}{tot(W,k)-tot(N,k):8.2f}')
if 'per' in sys.argv:
    for p in P:
        tw=sum(W[p].values()); tn=sum(N[p].values())
        top=lambda d,t: ', '.join(f'{k} {100*v/t:.0f}' for k,v in sorted(d.items(),key=lambda x:-x[1])[:4])
        print(f'{p:22} node {tn:5.2f}s: {top(N[p],tn)}\n{"":22} wasi {tw:5.2f}s: {top(W[p],tw)}')
if 'md' in sys.argv:
    print('\n| category | node avg % | js2wasi3 avg % | node CPU s | js2wasi3 CPU s | gap s |\n|---|---|---|---|---|---|')
    for k in sorted(C,key=lambda k:-(tot(W,k)-tot(N,k))):
        print(f'| {k} | {avgN[k]:.1f} | {avgW[k]:.1f} | {tot(N,k):.2f} | {tot(W,k):.2f} | {tot(W,k)-tot(N,k):+.2f} |')
    print(f'| **total** | | | **{TN:.1f}** | **{TW:.1f}** | |')
    K=['property access','numbers (box/unbox)','GC','operators','arrays/args','regex','BigInt','JS code']
    for name,D_ in (('js2wasi3',W),('node',N)):
        print(f'\n{name}:\n\n| program | CPU s | '+' | '.join(K)+' | top other |\n|---|---|'+'---|'*(len(K)+1))
        for p in P:
            t=sum(D_[p].values()); rest=sorted(((k,v) for k,v in D_[p].items() if k not in K),key=lambda x:-x[1])[:1]
            other=f'{rest[0][0]} {100*rest[0][1]/t:.0f}' if rest else ''
            print(f'| {p} | {t:.2f} | '+' | '.join(f'{100*D_[p].get(k,0)/t:.0f}' for k in K)+f' | {other} |')
