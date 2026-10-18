# T000 (task zero): general analysis: why our code on V8 is ~6.8× slower than node

Status: done (2026-10-17): analysis, type counts and two probes; the design is being built in T131 (type analysis, done), T132 (struct per shape), then `f64` fields and guarded fast paths

## Is "no slower than node on V8" achievable?
The most relevant proven system is **Hopc** (Serrano, Inria; "Of JavaScript AOT Compilation Performance", ICFP 2021).
It compiles full untyped JavaScript ahead of time to native code (no Wasm layer). It's the best published result for
untyped JS without a JIT.
- On many JetStream tests Hopc is **1–2× V8**.
- It's **much slower on the same kinds of programs where we're worst**: floating point (crypto, n-body, raytrace:
  roughly 10–25× V8 in its Figure 8), deltablue (lots of small method calls V8 inlines away), and regexp.
- The paper calls these the main limits of compiling JS ahead of time.

Our current results on V8 (`out/v8bench-now.md`): geometric mean **6.8× node**. Worst: fannkuchredux 20×, raytrace
19.8×, richards 18.9×, crypto 17.9×, deltablue 16.2×; best pidigits 1.5×, mandelbrot 2.2×.

**Honest conclusion:** equal to node on every program isn't achievable for untyped JS without run-time speculation;
even Hopc doesn't get there. A realistic general target is **about 2–3× node geometric mean on V8**. That's 2–3 times
better than now, and it needs different, general techniques rather than per-benchmark fixes.

## What Hopc does in general, and what we lack
| technique (Hopc) | what it does | us |
|---|---|---|
| **Hidden classes plus a per-site cache, checked inline with one compare**, properties stored inside the object itself (one memory load) | every property read or write is compare + load, no call | we call a helper on most reads; we tried hidden classes and reverted them; fields live in a separate array (extra loads) |
| **Caching prototype (method) lookups**: objects on prototype chains are marked; changing a marked object invalidates the caches | `o.method()` is as fast as a field read | not done for reads (tried once, dropped); 11–25M slow method lookups per run in raytrace, deltablue, richards |
| **Type analyses plus code duplication**: a fast version of code for the likely types, guarded, and a slow fallback | arithmetic and comparisons on plain numbers without tests or boxing | we only specialize locals proven to be numbers; boxing and number tests are 16% + operators 7% of our time |
| **Integer range analysis** (int53 representation) | loop indexes and array indexes never box | partly done |
| **Closure analysis** (functions that don't escape become direct calls, no function object) | cheaper calls and allocation | partly done (direct calls exist) |
| **`arguments` / closures stack-allocated** with a guard | no garbage per call | not done |

The first three are the general levers. They match our profile (`docs/performance.md`): property access 30.5% of our
time, numbers 16.3%, operators 7.1%. They'd act on all programs, not one benchmark at a time.

## Suggestion
1. **Measure first, generally:** profile the 18 programs on **V8** (not wasmtime), by category. That shows what of
   the 6.8× is our code without wasmtime's collector in the way. T116 left this undone.
2. **Then one design document**, before more code: object model (hidden classes with inline checks), method-lookup
   caching, and type-guarded fast paths. It would be based on Hopc's paper (plus V8 and JSC), with expected effects
   per category and the size cost stated up front.
3. **Only then build**, in that order, each judged by the 18-program total, `.wasm` size and the V8 numbers.

## Correction (2026-10-17, after discussion with the user)
"Not achievable" was too strong. Parsing and compiling at run time cost node little (its whole run is 94–465 ms);
the gap is the **code we generate**, not the missing last-mile compile. V8 is fast because it compiles late with the
types it has seen; in Wasm we can't generate code at run time, but we can emulate it:
- **pick the type at build time**: whole-program type analysis (we see all the code); where it can't decide, a
  profiling run (profile-guided build) or the common default (number for arithmetic, the constructor's shape for
  field reads);
- **emit two versions**: fast code for that type and the generic code we have today;
- **guard the fast one with one cheap check** (is it a number, is it this shape); a failed check takes the generic
  path. That replaces V8's deoptimization.

When the guess is right (almost always, if most sites see one type) the cost is only the guard, close to V8's own
optimized code. Only sites that really see many types stay slow, and V8 is slow on those too.

What has to change in our design for this to pay:
1. **Values**: every JS value is a generic reference, so a non-small-integer number is an allocated box (numbers 16%,
   part of GC 11%). V8 keeps numbers unboxed. → unboxed number types for locals, fields and arrays wherever analysis
   or a guard allows.
2. **Objects**: ours are a key array + a value array (a small dictionary), read with a key check and a call. V8's are
   a fixed struct with a shape id: one compare + one load. → real WasmGC structs per shape (property access 30%).
3. **Specialization**: we emit one generic version per function. → typed versions with a guard and the generic
   fallback.

Our 130 tasks patched the generic path (caches, guesses, inline fast paths) for 1–5% each, because the base stays
generic; T130 showed a fast path on top of the current layout costs size (+16.9% `.wasm` for −0.3% time).

## Measurement: how many types each site sees
First general step: for the 18 programs, record per site how many different types it sees (property reads/writes,
method-call receivers, element accesses, calls, arithmetic/comparisons), weighted by executions. Result = the share of
executed operations a single-type guarded fast path would cover. Method: the programs rewritten so each site records
its operand types (objects by V8 map, `%HaveSameMap`; numbers as small integer / double), run under node (types are a
property of the program, not of our compiler). Tool in the scratchpad (`typeprobe`, oxc; not in the repo).

### Result (2026-10-17, laptop, node 24.21.0; all 18 outputs = node; full table `out/typeprobe.md`)
3,917M operations recorded over the 18 programs.

| kind | executed | one type per site (exact) | covered by the most common type at each site | lowest program |
|---|---|---|---|---|
| property read `o.x` | 345.2M | 89.9% | 99.0% | 55.8% (splay) |
| property write `o.x = v` | 103.4M | 64.4% | 99.5% | 59.4% (splay) |
| method-call receiver `o.m()` | 88.9M | 90.1% | 96.8% | 89.3% (pidigits) |
| element read `a[i]` | 467.0M | 85.5% | 99.9% | 97.5% |
| element write `a[i] = v` | 245.2M | 95.3% | 100.0% | 98.4% |
| element key `i` | 712.2M | 100.0% | 100.0% | 100.0% |
| call `f()` (callee value) | 69.3M | 100.0% | 100.0% | 100.0% |
| arithmetic `+ - * / % **` | 1149.5M | 71.9% | 99.0% (100.0% if int/double count as one "number") | 88.3% (raytrace) |
| bit operations | 240.1M | 99.1% | 100.0% | 94.2% |
| comparisons `< > <= >=` | 343.1M | 91.3% | 99.7% (100.0% as numbers) | 98.1% |
| equality `== === != !==` | 153.0M | 61.1% | 86.6% | 50.0% |

- "Covered by the most common type" = the share of executions a fast path for one type per site (with a guard)
  would take. For property, element, call, arithmetic and comparison sites it is **97–100%**.
- Equality is lower (86.6%) because sites like `x === null` see two types (an object or null); a guard for that is
  still one compare.
- Polymorphic exceptions: splay's property reads/writes (55.8% / 59.4%: tree nodes of several shapes), richards'
  method receivers (90.6%), raytrace's arithmetic (88.3%: ints and doubles mixed).
- Limits: shapes are V8's maps (`%HaveSameMap`); up to 8 per site are told apart, the rest count as one ("obj+":
  1.0% of all operations, so the coverage of very polymorphic sites is slightly overstated). Types come from one run
  with the benchmark's own input.

**Conclusion:** the programs are almost entirely monomorphic. One guess per site with a cheap guard would take the
fast path for ~99% of executed operations, which supports the emulation above (build-time types + guarded fast path
+ generic fallback), provided the fast path is really one check + one load/op (new object and value layouts).

## Probe: is the proposed design fast? (2026-10-17, laptop, best of 2 runs; n = 20M)
`probes/layout.js` (constructor, field reads, two prototype methods, double arithmetic, two allocations per
iteration) compiled by hand in `probes/layout.wat` (run: `wasm-tools parse probes/layout.wat -o probes/out/layout.wasm`,
`node probes/layout.mjs probes/out/layout.wasm`, `wasmtime run --invoke run_typed probes/out/layout.wasm 20000000`).
All versions print node's exact result (1.999999933333426e+28).

| version | V8 (node) | × node | wasmtime | × node | `.wasm` |
|---|---|---|---|---|---|
| node JS | 290 ms | 1.0× | | | |
| proposed: struct per shape, `f64` fields, guarded method calls (shape + prototype), generic path present | 440 ms | 1.5× | 606 ms | 2.1× | 1.2 KB (both versions) |
| proposed objects, numbers boxed | 1,141 ms | 3.9× | 1,769 ms | 6.1× | |
| our compiler today | 8,582 ms | 29.6× | 14,519 ms | 50× | 178 KB |

- The proposed design is **19.5× faster on V8 and 24× faster on wasmtime** than our output today on this kernel,
  and 1.5× node on V8 (node inlines `add`/`dot` and removes the allocation of `b`; we keep calls).
- Of that, the object layout (struct per shape, guarded calls) gives ~7.5× (today → boxed), unboxed number fields
  another ~2.6× (boxed → typed).
- What the probe does **not** prove: that the compiler can derive these types. It assumes (a) `Vec`'s fields are
  always numbers (analysis or a guard at the constructor), (b) Vec objects never get other properties (a real design
  needs a fallback for added/deleted keys, e.g. an overflow dictionary field in every object, and generic code
  such as `for-in` reading structs). Those are the next probe's questions.

## Probe 2: the same with what the language needs (2026-10-17, laptop, best of 3; n = 20M; noisy: runs got slower round by round)
`probes/layout2.{js,wat}`: probe 1 plus (1) every object has a header (`$flags`: a struct field deleted / given a
non-number / prototype changed; `$extra`: overflow dictionary for added keys), every guard checks shape + flags;
(2) every 1000th object gets `b.tag = i` (into `$extra`, the shape stays); (3) `Object.keys(b).length` reads struct
objects generically; (4) `new Vec(i, half, 1)` with `half` read from a plain object, so the constructor checks it's
a number (else a generic object). Results = node's at n = 1, 1001, 3000 and 20M.

| version | probe 1 V8 | probe 2 V8 | probe 1 wasmtime | probe 2 wasmtime |
|---|---|---|---|---|
| node JS | 238 ms | 561 ms | | |
| proposed design | 476 ms | 545 ms | 533 ms | 891 ms |
| proposed objects, numbers boxed | 1,156 ms | | 1,838 ms | |
| our compiler today | 8,582 ms | 10,537 ms | 14,519 ms | 20,006 ms |

`.wasm`: probe 2 hand-written 1.8 KB (incl. its generic paths); our compiler today 180 KB (with the runtime).

- The language's costs (header, flags check, generic paths) cost +14% on V8 and +67% on wasmtime (larger objects:
  5 fields instead of 3; wasmtime's allocation/collection cost grows with size).
- Node gets 2.4× slower on probe 2 (the objects with `tag` change their hidden class, so `add`/`dot` see two shapes);
  our design keeps one shape (added keys go to `$extra`), so on V8 it's **as fast as node** (545 vs 561 ms).
- Our compiler today: 19× node on V8, 36× on wasmtime here; the proposed design: 1.0× and 1.6×.
- Still unproven: that the compiler's analysis finds these types in real programs (the 18), and the size of the
  generated code with two versions per function.

## Sources
- [Of JavaScript AOT Compilation Performance (Serrano, ICFP 2021), PDF](https://www.cs.tufts.edu/~nr/cs257/archive/manuel-serrano/aot-js.pdf), DOI 10.1145/3473575
- [Strange Loop 2022 talk: Of JavaScript Ahead-of-Time Compilation Performance](https://thestrangeloop.com/2022/of-javascript-ahead-of-time-compilation-performance.html)
- [JavaScript sealed classes in hopc (ECOOP 2022)](https://drops.dagstuhl.de/entities/document/10.4230/LIPIcs.ECOOP.2022.24)
- [Porffor (AOT JS→Wasm): README](https://github.com/CanadaHonk/porffor)
- [Static Hermes: optimizing with types (talk slides)](https://speakerdeck.com/tmikov2023/optimizing-with-static-hermes-chain-react-2024?slide=6)
- [Type Inference for Static Compilation of JavaScript (OOPSLA 2016)](https://arxiv.org/pdf/1608.07261)

## Progress
- 2026-10-17: analysis written from `out/v8bench-now.md`, `docs/performance.md` and the Hopc paper (user: "write this
  down in task_zero").
- 2026-10-17: correction section added; typeprobe written (scratchpad), all 18 programs run under node with recording (outputs = node), results above and in `out/typeprobe.md`.
- 2026-10-17: probe `probes/layout.{js,wat,mjs}` written and run (laptop): proposed design 440 ms on V8 / 606 ms on wasmtime vs today 8,582 / 14,519 ms, node 290 ms.
- 2026-10-17: probe 2 `probes/layout2.{js,wat}` (header, flags, overflow dictionary, Object.keys, unknown-type constructor argument): V8 545 ms vs node 561 ms; wasmtime 891 ms; today 10,537 / 20,006 ms.
- 2026-10-18: after T131–T133 (`main` at cb80dae; laptop, `scripts/v8bench.sh -o out/v8bench-t133`,
  `out/v8bench-t133.md`; a-03 has no jco): our components on V8 **6.43× node** (was 6.84×), on wasmtime 9.58×
  (was 10.06×). Worst: raytrace 19.2×, fannkuchredux 19.0×, crypto 18.7×, richards 13.9× (was 18.9×), deltablue
  12.3× (was 16.2×), revcomp 10.5×; nbody 9.9× → 7.0×.
