# Root causes of the speed gap (T112)

Every open follow-up of `tasks/README.md`, grouped by the root cause it comes from, with the programs each root
costs. Sources: only the standard benchmarks (the 18 programs of `tests/bench`, `docs/performance.md`'s profiles of
2026-10-10 at `e13d22c`, the per-task benchmark results T103–T110 on a-03) and the fixtures/test262. Written
2026-10-12 at `a52761d` (T110 paused). The profile table predates T103–T110; what those tasks changed is listed per
root. Fresh counts and profiles: T112 steps 2–3.

## Why fixes broke other programs (T106–T110)

| task | gain | broke | root it hit |
|---|---|---|---|
| T106 | spectralnorm −32% | navier-stokes +5% | R8 (code layout; same helpers, different placement) |
| T107 | number arrays | knucleotide +2.3%, deltablue +1.7% | R2's fast path taxes generic arrays (state checks on every append) |
| T108 | navier-stokes −47% | mandelbrot +3.5% (first version) | lost an existing peephole (patched) |
| T109 | splay −27%, binarytrees −26% | raytrace +3% | a static guess (constructor = `this` stores) wrong for `initialize.apply` (patched by a name filter) |
| T110 | nbody −34%, richards −27% | knucleotide +7.2% | R1: inline cast of a Map at a key-name guess calls `is_subtype` |

Common pattern: a **static guess** whose **miss costs more than the old path**. Each patch narrowed the guess for
the program that showed it; the root stayed.

## Roots, in order of attack

The order is by what each root blocks: R1 first because R3's and R2's next steps (T110, T111) both put new casts on
paths that today stay away from them.

### R1. Type tests on `$object` subtypes call wasmtime's `is_subtype`
`$object` isn't final (functions, arrays, Maps, promises, generators, typed arrays, wrappers… extend it), so
wasmtime compiles `ref.test`/`ref.cast (ref $object)` inline only for an exact `$object`; any other value calls its
`is_subtype` libcall, also when the answer is "no". Workarounds so far: `object_or`'s chain of exact tests (T92,
T93, T96: `is_subtype` ≤ 0.4% of every benchmark since T93); T109 chose no new `$object` subtype for that reason.
Measured costs of the chain: T92's first version (10 exact tests at every site) made plain-object reads 65% slower;
T110 with 3 exact tests before its inline cast lost half its gain (laptop: nbody −36% → −18%).
- Costs now: knucleotide +7.2% under T110 (F409). Blocks: T110's inline sites (F405/F409), any inline object path
  (F359 inline caches), T111's cell test is safe (a final type).
- Follow-ups: F409, F350, F360, F359 (part).
- **Fixed by T113 (2026-10-12):** plain objects are a final `$plain`; every object test is a chain of exact compares
  (`$plain` first), so no object test calls `is_subtype` (a-03: deltablue −8.6%, richards −8.1%, knucleotide −5.2%;
  F409 closed). Left: F410–F413 (the chain's order and costs per kind).
- Root fix (T112 step 6, done in T113): an object-kind test that never calls `is_subtype` and costs one compare on the
  plain-object path (e.g. a final type for plain objects with every generic site going through `object_or`, or a
  kind byte reachable without a cast); measured on hit and miss before use.

### R2. Numbers are boxed outside `f64` locals
A non-i31 number in a `jsval` slot (object field, generic array, parameter, `jsval` local) is a new `$number` per
write and an unbox per use. `docs/performance.md`: numbers 16.3% of js2wasi3's time on average (node 0).
- Fixed since: T103 (mandelbrot 77% → ~0), T106 (`let x;`), T107 (number arrays), T108 (element reads in `+`).
- Left: nbody fields (F406, `box_number` 11.9% from `advance`), crypto `am3` parameters/locals (F404, 18.6%),
  navier-stokes `lin_solve` local (F403), comparisons over elements (F402, ≤ 1.1%), booleans boxed (F14), integer
  loops on `f64` (F335), BigInt allocation per op (F188; pidigits 89% BigInt), conservative definite assignment
  (F398), F323, F334.
- Risk pattern (T107): a number mode adds state checks to the generic path (F400).

### R3. Property access is a helper call on dictionary objects
Objects are key/value arrays with a hash index past 8 keys; constant-key sites guess an entry (T74/T105), T109 adds
layouts at construction, T110 inline slot checks (paused). `docs/performance.md`: 30.5% on average; nbody 70%,
deltablue 77%, richards 74%, raytrace 69%, earley-boyer 39%, fannkuchredux 36% (elements).
- T110 (a-03): nbody −34%, richards −27%, earley-boyer −15%, deltablue −11%, binarytrees −6%; blocked by R1.
- Left: prototype (method) reads (deltablue ~13%, richards ~4%; T99's cache didn't pay; F321 closed with no open
  successor), computed keys (F363 closed by T94/T95, remainder pointed at F321: no open follow-up), fields set by another function (F407: deltablue `superConstructor.call`, raytrace `initialize.apply`),
  polymorphic sites (F357), shapes work parked (F358, F360, F361, F40), elements of generic arrays (fannkuchredux).
- Follow-ups: F40, F357, F358, F359, F360, F361, F405, F407, F96 (JSON.parse builds dictionaries); prototype reads
  and computed keys need new follow-ups (T112 step 6).

### R4. Calls through values and argument arrays
A call through a value builds an argument array and boxes (~230 ns, F92); `apply`/`arguments` copy (raytrace:
`make_arguments` 8.5% + `list_from_array_like` 5% + `own_get` 7%, T109 profile); built-in methods are prelude
functions called through the prototype (F90, F100, F177).
- Costs: raytrace ~20%, arrays/args 4.9% and calls 1.5% on average; Map/Set-heavy code (knucleotide "other
  runtime" 28%).
- Follow-ups: F46, F84, F90, F92, F100, F177, F229, F69, F117, F73, F374.

### R5. GC: wasmtime's copying collector copies all live data
Share of time like node's (~11%), seconds ~13× node's: more allocation and a full copy per collection.
binarytrees 57%, splay 78%, earley-boyer 31% (before T109, which cut construction garbage: splay −27%,
binarytrees −26%). Every allocation saved in R2/R3/R4 lands here too.
- Follow-ups: F304, F386, F394 (allocation sinking), F347's heap size trade-off (done).

### R6. Regex runs in a JS-written VM
octane-regexp 81%, regexredux 84% (node compiles each pattern to machine code).
- Follow-ups: F336, F179, F178, F354, F209 (test262 directory left out for its speed).
- Fixed for literals in the subset by T114 (2026-10-13): build-time programs run by one Wasm interpreter, as Hermes
  (octane-regexp 56.7× → 10.5× node, regexredux 6.8× → 4.5×); left: F415, F416, F417, F419.

### R7. Strings
UTF-16 always (F352), repeated appends to properties/captured variables copy (F337, F365), number → string without a
cache (F393), `fromCharCode` rest arrays (F229), JSON quoting/parsing (F327, F96), SIMD scans (F102),
`TextEncoder`/`TextDecoder` loops (F353). Costs on the 18 programs: strings 1.2% on average; knucleotide (`string_eq`,
Map keys).
- Follow-ups: F102, F327, F337, F352, F353, F365, F393.

### R8. Measurement noise and code layout
Unchanged hot code moved by a few percent between builds (F399: navier-stokes +5.4% in T106); the compiler's output
isn't deterministic for some programs (F186), so two builds of one source can differ. Without a fixed method a
2–3% "regression" can't be told from layout.
- Follow-ups: F186, F399, F400 (to separate from layout).
- T112 step 5 (one gate) uses: median of 5 interleaved runs, the T109/T110 a-03 procedure, and compares `.wasm`
  bytes first (identical `.wasm` = no change by definition).

### R9. Compile speed and output size (not run time)
Build and `wasmtime compile` time, binary size, linking only what's used.
- Follow-ups: F8, F19, F56, F68, F75, F82, F86, F113, F141, F150, F178, F255, F269, F270, F312, F328, F343, F395,
  F408.

## Counts (T112 step 3)

`scripts/bench.sh -c` at `a52761d` + T112's counters, small sizes, laptop, 2026-10-12; all 18 outputs = node.
Columns: `get_named` guess hit / own / prototype-or-absent / not an object; `set_named` guess hit / other; T110
inline reads hit / miss on a subtype / miss on a plain object; inline writes hit / miss plain; `layout_set` add;
`layout_define` fast. Zero columns left out (`layout_set` overwrite, `define` shared dropped: never ran).

| program | gn hit | gn own | gn proto | gn !obj | sn hit | sn other | ir hit | ir miss sub | ir miss plain | iw hit | iw miss plain | ls add | ld fast |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| binarytrees | 0 | 4 | 19 | 3 | 0 | 42 | 29.9M | 0 | 0 | 0 | 0 | 0 | 30.0M |
| fannkuchredux | 0 | 5 | 19 | 3 | 0 | 42 | 0 | 0 | 0 | 0 | 0 | 0 | 14 |
| fasta | 42k | 11 | 19 | 3 | 0 | 42 | 42k | 0 | 0 | 0 | 0 | 0 | 14 |
| knucleotide | 14 | 101 | 17.5M | 8.8M | 3 | 164 | 4 | 17.5M | 5 | 0 | 11 | 0 | 160 |
| mandelbrot | 0 | 6 | 19 | 3 | 0 | 42 | 2 | 0 | 0 | 0 | 0 | 0 | 14 |
| nbody | 0 | 4 | 27 | 5 | 0 | 49 | 85.0M | 0 | 0 | 37.5M | 0 | 28 | 14 |
| pidigits | 0 | 4 | 19 | 203 | 0 | 42 | 0 | 0 | 0 | 0 | 0 | 0 | 14 |
| regexredux | 963k | 100 | 593 | 140 | 963k | 131 | 4 | 83 | 5 | 0 | 10 | 0 | 103 |
| revcomp | 2k | 120 | 2k | 1k | 3 | 168 | 260 | 1k | 5 | 14 | 12 | 8 | 117 |
| spectralnorm | 0 | 5 | 23 | 4 | 0 | 42 | 0 | 0 | 0 | 0 | 0 | 0 | 14 |
| octane-richards | 819 | 68 | 16.6M | 5 | 0 | 137 | 64.1M | 0 | 952k | 18.6M | 7 | 40k | 21 |
| octane-deltablue | 20.7M | 8k | 24.8M | 5 | 225k | 48k | 23.9M | 0 | 5.1M | 3.7M | 198k | 718k | 22 |
| octane-raytrace | 41.0M | 373k | 11.3M | 65 | 2.3M | 11.7M | 4.2M | 0 | 1.9M | 143k | 1.2M | 0 | 71 |
| octane-navier-stokes | 38 | 38 | 44 | 5 | 0 | 93 | 1k | 0 | 1 | 6 | 5 | 245 | 21 |
| octane-crypto | 7.7M | 1k | 2.9M | 401k | 174k | 218 | 12.9M | 0 | 1 | 8k | 8 | 9k | 23 |
| octane-splay | 1.1M | 804k | 188k | 6 | 428k | 115k | 810k | 0 | 1.6M | 4 | 840k | 115k | 1.7M |
| octane-earley-boyer | 347 | 72 | 1k | 575 | 1 | 140 | 74.4M | 0 | 1 | 291k | 15 | 22.7M | 23 |
| octane-regexp | 3.8M | 38 | 1.9M | 786k | 4.3M | 79 | 82 | 0 | 19 | 14 | 6 | 0 | 21 |

## Not speed: correctness and features (no root above)

- Language semantics: F2, F5, F33, F45, F54, F57, F63, F76, F103, F105, F146, F148, F158, F166, F169, F195, F199,
  F204, F206, F211, F226, F232, F257, F260, F370.
- Built-ins conformance: F50, F61, F77, F85, F89, F104, F106, F107, F108, F109, F110, F111, F114, F116, F147, F149,
  F152, F155, F156, F160, F163, F165, F171, F172, F173, F174, F175, F176, F181, F182, F183, F184, F189, F190, F193,
  F198, F201, F202, F208, F212, F213, F214, F215, F219, F220, F221, F222, F223, F225, F227, F233, F248, F256, F258,
  F259, F277, F278, F293, F296, F297, F302, F310, F325, F331, F342, F344, F345, F355, F356, F364, F367, F387, F396, F397.
- Node APIs: F31, F112, F230, F231, F234, F235, F236, F237, F239, F240, F242, F243, F244, F247, F252, F253, F254,
  F271, F275, F279, F280, F281, F284, F285, F287, F288, F289, F290, F291, F292, F298, F301, F305, F306, F307, F308,
  F309, F313, F314, F315, F319, F332, F346, F388.
- Components / libraries / WIT: F6, F12, F13, F15, F26, F34, F55, F60, F83, F118, F119, F120, F121, F122, F123,
  F124, F125, F126, F127, F128, F129, F130, F131, F132, F133, F134, F135, F136, F137, F138, F197, F251.
- Test tooling: F30, F142, F157, F180, F185, F196, F228, F250, F274, F294, F320, F375.
- Embedded engine for dynamic code: F191.
- Outside the standard tests (user rule 2026-10-12, not used): F272, F273, F311, F316, F317, F326, F351, F381.
