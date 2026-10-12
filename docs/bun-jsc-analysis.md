# Bun and JavaScriptCore: what js2wasi3 can use

Read 2026-10-10: Bun at `71d0d43` (github.com/oven-sh/bun, main), JavaScriptCore from Bun's WebKit fork at the commit
Bun pins (`scripts/build/deps/webkit.ts`: `0c06faa`, github.com/oven-sh/WebKit). Paths below are in those trees.
The costs quoted for js2wasi3 are the measurements in [performance.md](performance.md).

## What Bun is made of

Bun is a runtime: its own code (now Rust: `src/*/Cargo.toml`) is the transpiler, bundler, package manager, Node
APIs and the event loop. JavaScript runs on JavaScriptCore (JSC), WebKit's engine. Three parts concern a compiler:

| part | what it does | for speed? |
|---|---|---|
| `src/js_parser` + `src/js_printer` (esbuild's design) | parse JS/TS/JSX, strip types, then while visiting: constant folding (`fold.rs`, `ast/fold_string_addition.rs`: `"a" + 1` → `"a1"`), `const` inlining (`visit/mod.rs`: `const_values`), TypeScript `enum` inlining, `typeof x === "undefined"` → `typeof x > "u"` (`visit_binary.rs`), dead branches (`scan_side_effects.rs`), `--define` replacement (`defines_table.rs`: `process.env.NODE_ENV` → `"production"`), known-pure globals (`ast/known_global.rs`), minification | mostly bundle size; JSC re-optimizes anyway |
| `src/bundler` | module graph, tree shaking (`LinkerContext.rs`, `linker_context/`), cross-module `const`/`enum` inlining, scope hoisting | size and startup |
| `src/sema` | a TypeScript type checker in Rust, a port of typescript-go's binder and checker (`sema/lib.rs`): hash-consed types (`types.rs`), control-flow narrowing (`check/flow.rs`), object shapes of types (`check/shape.rs`) | no: type errors only, nothing feeds code generation |
| `function_identities.rs`, `bytecode_order.rs` | name functions so JSC's bytecode cache can be precomputed (`bun build --compile --bytecode`) | startup only |

So Bun's speed is JSC's. Bun's own transformations are what any bundler does before shipping code.

## How JSC gets its speed

Four tiers: LLInt (an interpreter written in an assembler DSL, `llint/`), Baseline JIT (`jit/`), DFG (`dfg/`,
optimizing), FTL (`ftl/` + B3, `b3/`). Values are NaN-boxed 64-bit words (`runtime/JSCJSValue.h`). Objects have a
`Structure` (hidden class: property names → offsets, `runtime/Structure.h`), in-object slots plus a "butterfly" for
more properties and the indexed elements. Each tier collects profiles (types seen, structures seen, array shapes) that
the next tier speculates on; a wrong guess exits to a lower tier (OSR exit).

js2wasi3 can't do the speculation part: a Wasm component's code is fixed at build time (performance.md, "Why not
node's techniques"). What carries over is everything JSC decides **statically** or guards with a **cheap check**:

### 1. Static analyses JSC already runs at bytecode generation

- **`StaticPropertyAnalyzer`** (`bytecompiler/StaticPropertyAnalyzer.h`): "flow-insensitive static analysis of the
  number of properties assigned to an object" — for every `new Object` / object literal and every constructor's
  `this`, it collects the property names stored into it (`putById`) until the value escapes (`kill`), and the
  bytecode generator writes that count into the allocation instruction (`BytecodeGenerator.cpp`:
  `setInlineCapacity(m_propertyIndexes.size())` on `op_new_object` / `op_create_this`). The object is then allocated
  with exactly that many in-object slots (`bytecode/ObjectAllocationProfileInlines.h`: "Try to shrink the object
  based on static analysis").
- Scope resolution: each variable is resolved to a register, a closure slot or a global at bytecode generation; only
  `eval`/`with` make it dynamic. js2wasi3 does this already (`crates/lower/src/closures.rs`).

### 2. DFG phases that don't need a profile

From `dfg/DFGPlan.cpp` and each phase's header:

| phase | what it does | static form for js2wasi3 |
|---|---|---|
| `BackwardsPropagation` | "how nodes are likely to be used": a value used only as an int32, or where `-0`/NaN/overflow can't be observed | `x | 0`, `a[i]`, `i < n`: keep `i32` (F335) |
| `ValueRepReduction` | removes `DoubleRep(ValueRep)` round trips: boxing a double and unboxing it again | the `box_number` → `to_number` pairs (16% of the benchmarks, performance.md) |
| `ObjectAllocationSinking` | an allocation that is only used locally becomes SSA values: no object is made | `{x, y}` returned and destructured, `[a, b] = [b, a]`, iterator results |
| `ArgumentsElimination`, `VarargsForwarding` | no `arguments` object when it doesn't escape, `f.apply(this, arguments)` forwards the values | `arguments.length` / `arguments[i]` / `apply(this, arguments)` without an object |
| `IntegerRangeOptimization` | proves `0 ≤ i < a.length` in `for (i = 0; i < a.length; i++) a[i]`, drops the checks | the same loop shape: no bounds/hole checks |
| `LICM`, `CSE`, `ConstantFolding`, `StrengthReduction`, `CFGSimplification`, `DCE` | the classic ones | none exist in js2wasi3's IR today (a structured tree, `crates/ir/src/lib.rs`); wasmtime's Cranelift does some, but not across our helper calls |

`PredictionPropagation`, `Fixup` and `TypeCheckHoisting` need the value profiles: not applicable as such. Their
static counterpart is js2wasi3's own type inference (`crates/lower/src/lib.rs`: the `Kind` lattice
`Bottom < Num | Bool | Str < Any`, re-run until stable).

### 3. Representation choices

- **Array element kinds** (`runtime/IndexingType.h`): an array is `Int32Shape`, `DoubleShape` (raw unboxed doubles),
  `ContiguousShape` (any values) or `ArrayStorageShape` (sparse/accessors); stores move it one way only, toward the
  general shape. js2wasi3's `$array` keeps its elements in one `jsval` array (`crates/backend/src/array.rs`:
  `F_ELEMS`), so every non-i31 number in an array is a boxed `$number`.
- **Strings** (`runtime/JSString.h`): 8-bit (Latin-1) or 16-bit storage, ropes for concatenation, substrings that
  share their base; property names are atoms (`runtime/PropertyName.h`), so equal names are one pointer. Caches:
  `NumericStrings` (`runtime/NumericStrings.h`: the last 1,024 integers and 4,096 doubles converted to strings),
  `SmallStrings` (`runtime/SmallStrings.h`: every one-character Latin-1 string made once).
- NaN-boxing: not possible in WasmGC (a reference can't carry a double); `i31ref` + a boxed `f64` is the WasmGC form.

### 4. Watchpoints: one check for a whole invariant

`runtime/JSGlobalObject.h` keeps 47 watchpoint sets, each guarding a fact about the built-ins:
`arrayIteratorProtocolWatchpointSet` (for-of over an array may skip the iterator protocol),
`arraySpeciesWatchpointSet` (`map`/`filter`/`slice` make plain arrays), `regExpPrimordialPropertiesWatchpointSet`
(`RegExp.prototype` methods and `flags`/`global`/… untouched), `promiseThenWatchpointSet`/`promiseResolveWatchpointSet`
(`await` without the observable `then` lookup), `mapSetWatchpointSet`/`setAddWatchpointSet`, `numberToStringWatchpointSet`,
`havingABadTimeWatchpointSet` (no indexed accessors anywhere on array prototypes). A write to a guarded property fires
the set once; until then the fast path needs no check at all in JIT code, and one flag test in the interpreter.

### 5. Yarr (regular expressions)

`yarr/YarrPattern.cpp` (`YarrPattern::compile`) optimizes the pattern before matching:
`checkForTerminalParentheses`, `optimizeDotStarWrappedExpressions`, `optimizeBOL` (anchored patterns tried once),
`factorAndWrapAlternatives` (common prefixes out of alternatives), `optimizePossessiveQuantifiers` (no backtracking
into a quantifier that can't give back a useful match), `setupOffsets` (fixed-size alternatives checked by length
first). `YarrJIT.cpp` then compiles the pattern to machine code, with a Boyer-Moore-style bitmap
(`BoyerMooreBitmap`, `BoyerMooreFastCandidates`) to skip start positions; `YarrInterpreter.cpp` runs it as bytecode
when there is no JIT. All of this depends only on the pattern text.

## Proposals for js2wasi3

Ranked by the measured cost they address (performance.md) and how directly the technique carries over. The gains are
estimates until measured.

| # | proposal (from) | where in js2wasi3 | targets (measured) | effort |
|---|---|---|---|---|
| 1 | **Shapes decided at compile time per allocation site** (after JSC `StaticPropertyAnalyzer`, which only counts the properties to size the allocation; JSC's layouts are run-time structures with per-site transition caches: T109 cross-check): for each object literal and each constructor, the property list stored before the value escapes gives the object's initial layout; the object is allocated with it, and reads of `this.x` / `o.x` where the receiver comes from that site check one constant shape id and load a slot | `lower` (analysis per site, like `closures.rs`), `backend/object.rs` (allocate with the layout; keep dictionary mode as the fallback) | property access: 31% of the benchmarks, 22.5% of scriptc's workloads; the object-heavy workloads 4–59× from types under scriptc | large. Unlike T83 there's no transition search on the common path (the cost that made T83 lose on zigbee2mqtt, F362) |
| 2 | **Array element kinds** (`IndexingType`: Int32/Double/Contiguous): arrays that only ever hold numbers keep an `(array (mut f64))`, converted once to the `jsval` form on the first other value | `backend/array.rs` (a second elements field or a second array type), `typed.rs`-like fast paths in `get`/`set` | numbers 16% of the benchmarks (navier-stokes 42%, spectralnorm 40%, crypto 32%) and their GC (boxed numbers are allocation) | medium |
| 3 | **Watchpoint flags for built-in invariants** (JSC's watchpoint sets): one global `i32` per invariant (RegExp built-ins untouched, array iterator untouched, species untouched, `Promise.prototype.then` untouched), cleared by any write to a guarded property; fast paths test it once | `backend` (flags next to `G_EPOCH`; T90's `IS_PROTO` marking finds the writes) | correctness of array/string iteration (a replaced `%ArrayIteratorPrototype%.next` was ignored), the `regexp_plain` test (0.6–4.7% of the regex programs; F349's 9% predates T81's fast paths), for-of, `await` | small (T102) |
| 4 | **Regex literals compiled at build time** (Yarr's passes + YarrJIT's structure, emitting Wasm instead of machine code): anchoring, prefix factoring, possessive quantifiers, first-unit bitmaps, fixed-size checks | `backend` (a code generator per literal) or the prelude's VM (the passes alone) | regex: octane-regexp 81%, regexredux 84%, regex-logs 53%; regex-logs is 6.2× scriptc's Wasm build on the same source (F336) | large (the passes alone: medium) |
| 5 | **Representation passes in the lowering** (`BackwardsPropagation`, `ValueRepReduction`, `IntegerRangeOptimization`): `i32` locals for values used only as integers (F335), definite assignment for `var x;` (F389), no box/unbox round trips between helpers, no bounds checks in `for (i = 0; i < a.length; i++)` | `lower` (the `Kind` lattice gains `Int` and "used as") | mandelbrot 77% box/unbox, fannkuch 20%, spectralnorm 40% | medium |
| 6 | **String caches** (`NumericStrings`, `SmallStrings`) and **Latin-1 strings** (8-bit `JSString`, F352) | `backend/numstr.rs` (number → string cache), `runtime.rs` (one-unit strings made once), strings (F352) | csv-numbers 6.2× and template-render 3.8× scriptc's Wasm build on the same source (string/number built-ins); GC of string data | cache: small; Latin-1: large |
| 7 | **Allocation sinking / arguments elimination** (DFG phases, static form): temporaries that don't escape (`{x, y}` destructured at once, iterator results, `arguments` read by index) become locals | `lower` or an IR pass | GC: 26% of scriptc's workloads, 57–78% of binarytrees/splay (only where the objects don't escape) | medium |
| 8 | **Bun's transpiler passes** (`--define` folding of `process.env.NODE_ENV`, `const`/`enum` inlining across modules, tree shaking of unused module code) | `frontend`/`lower`: `frontend/link.rs` already joins the module graph by scope hoisting (as Bun's bundler does), so module-level `const`s are visible across modules there | build size and start-up work for npm-heavy programs (zigbee2mqtt); little run time | small to medium |

Not taken: NaN-boxing (no such representation in WasmGC), the JIT tiers, OSR and profile-driven speculation (no code
generation at run time), Riptide (JSC's garbage collector: the host owns the GC under WasmGC, D1), store barriers, the
bytecode cache (js2wasi3 compiles ahead of time), and Bun's `sema` (a type checker; js2wasi3 compiles JavaScript, and a
TypeScript source's types are unsound for code generation without checks).

Follow-ups: #1 F392, #2 F391, #3 F390, #4 F336, #5 F335/F389, #6 F393/F352, #7 F394, #8 F395 (`tasks/README.md`).

Suggested order: 3 (small, measurable on zigbee2mqtt's RegExp paths), then 2 and 5 (numbers; benchmarks show them
directly), then 1 (the biggest cost, the biggest change), with 4 as a separate track for regex-heavy programs.
