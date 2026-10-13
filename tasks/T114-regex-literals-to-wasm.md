# T114: regex literals compiled to Wasm at build time (root R6, F336)

Status: done | Depends on: T27, T39, T78, T81 | Local (+ a-03 only after asking)

## Goal
Root R6 of `docs/root-causes.md`: every regex runs in a backtracking VM written in JS (`crates/lower/builtins/regexp.js`,
`$$reRun`), which our compiler turns into Wasm with all the generic costs (numbers as `f64`, F335; `jsval`s;
helper calls). node compiles each pattern to machine code (irregexp). octane-regexp is the slowest benchmark
(57× node, 21.9 s vs 0.39 s); regexredux is 6.8×.

A regex literal's pattern and flags are known at build time, so it can be compiled to Wasm functions then, the way
JITs compile it at run time. `new RegExp(string)` and anything the compiler doesn't handle keep the VM.

## Measurements (2026-10-13, `99c3b10`)
- a-03 `perf` profile of octane-regexp (gate build, `fixed 0.05`, output = node; `out/octane-regexp-99c3b10.top`,
  `.children`; a-03 `~/re-prof/`). Self: `$$reRun` 61.7%, `$$reInRanges` 7.3%, `$$reSearch` 6.4%, `get` 3.8%,
  `re_alts_find` 3.6%, `box_number` + `get_num` + `to_boolean` 5.4%. With callees: `String.prototype.replace` →
  `$$reReplacePlain` 82%; the benchmark's `runBlock1` 76%, `runBlock0` 14%.
- Single calls, laptop, 2000 calls each (µs per call, node / js2wasi3):
  - `/(^|[^\\])\"\\\/Qngr\((-?[0-9]+)\)\\\/\"/g` replace on a 2.3k string with no match: 0.5 / 935–963 (~1900×);
    every one of ~2,300 start positions enters the VM (no first unit: the pattern starts with an alternative) and
    fails at the second character: ~400 ns per failed start;
  - `/d1/g` on a 4k string: 2.0 / 45.5; `/=/` and `/(\s)+e/` on short strings: ~0 / 1–3.
- All regexes in octane-regexp (~95) and regexredux are literals.

## Prior art (consulted 2026-10-13)
- **V8 irregexp** ([regexp-compiler.h](https://searchfox.org/mozilla-central/source/js/src/irregexp/imported/regexp-compiler.h),
  [regexp-nodes.h](https://cobalt.googlesource.com/cobalt/+/master%5E/v8/src/regexp/regexp-nodes.h); also SpiderMonkey's
  engine): parser → AST → a graph of `RegExpNode`s → native code or bytecode. Start positions: a Boyer-Moore-like
  lookahead table (the set of characters possible at each distance, mod 128), and a "quick check" (a wide load of
  several characters, a mask, one compare) before a node's full test. Backtracking: an explicit backtrack stack of
  code addresses. Bytecode first, native code after some executions (tier-up).
- **JSC Yarr** (`docs/bun-jsc-analysis.md` §5): pattern passes before code generation: `optimizeBOL` (an anchored
  pattern is tried once), `factorAndWrapAlternatives` (common prefixes out of alternatives),
  `optimizePossessiveQuantifiers` (no backtracking into a loop that can't give back a useful match), `setupOffsets`
  (fixed-size alternatives checked by length first); the JIT skips start positions with a `BoyerMooreBitmap`; an
  interpreter when there is no JIT.
- **.NET 7 regex source generator** ([blog](https://devblogs.microsoft.com/dotnet/regular-expression-improvements-in-dotnet-7/),
  [docs](https://learn.microsoft.com/en-au/dotnet/standard/base-types/regular-expression-source-generators)): the
  closest model: a backtracking engine compiled **ahead of time** per pattern into code without computed jumps (C# /
  IL, like Wasm). Two functions per pattern:
  - `TryFindNextPossibleStartingPosition`, in priority order: anchors; a leading literal prefix (vectorized
    `IndexOf`); sets at fixed offsets from the start (`IndexOfAny` on the rarest); a literal after a leading loop
    whose set can't match it (search the literal, then walk back); small sets with `IndexOfAny`;
  - `TryMatchAtCurrentPosition`: straight-line code; a backtrack point is a label with gotos; loops keep their start
    position in a local and give back one character at a time (or `LastIndexOf` to the next useful one).
  - Analyses: auto-atomicity (a loop followed by something its set can't match never backtracks), alternations whose
    branches start with different characters become a `switch` (no backtracking), common prefixes extracted,
    captures tracked only where needed. "Eliminating unnecessary work" over faster backtracking.
- **Rust `regex` / `regex-automata`**
  ([reverse_inner](https://docs.rs/crate/regex-automata/0.4.16/source/src/meta/reverse_inner.rs)): literal
  extraction: prefix, suffix and **inner** literals that every match must contain, used as a prefilter (memchr,
  SIMD); "reverse inner": find the inner literal, then match the prefix backwards to find the start. Linear engines,
  so no backreferences or lookaround, but the prefilter idea depends only on the pattern.
- **Rawr** (EPFL master project, 2024,
  [infoscience](https://infoscience.epfl.ch/entities/publication/d4e2dcb1-370b-43e1-ab42-7f952e74230e)): a subset of JS
  regexes compiled to Wasm, linear time, faster than V8's linear engine. A subset only; no backtracking semantics.
- **Porffor** (JS → Wasm AOT, `compiler/builtins/regexp.ts`) and QuickJS (`libregexp`): bytecode interpreters, like
  ours (Porffor: ~30 opcodes, a 5-`i32` backtrack entry). No JS → Wasm compiler we found compiles regex literals.

Conclusion: the structure is proven (.NET source generator: AOT, backtracking, no computed goto; irregexp/Yarr:
the same passes at run time); what is new is only the target (Wasm, `br_table` for backtrack dispatch).

## What proven systems ship (step 1 redone, 2026-10-13)
The first survey above looked only at matching technique. What each system actually ships:

| system | form of a regex | when | native code for |
|---|---|---|---|
| V8 (node) | bytecode, then native code | run time; native only after repeated use (tier-up, to save memory) | used regexes only |
| SpiderMonkey | the same (irregexp) | run time, interpreter first, native when used repeatedly | used regexes only |
| JSC (Bun) | Yarr: native code, bytecode as fallback | run time | run-time JIT |
| Hermes (ahead-of-time JS, React Native) | regex bytecode | **build time**, embedded in the bytecode file | none (interpreter) |
| QuickJS, Porffor | regex bytecode | run time | none (interpreter) |
| .NET | interpreter by default; IL at run time with `Compiled`; C# at build time with `[GeneratedRegex]` | build time only when the programmer opts in | opted-in regexes only |
| Rust `regex` | automata + literal prefilters | run time | none; the build-time `regex!` macro was dropped (slower than the run-time engine) |
| Go, RE2 | automata (NFA/DFA, one-pass, small backtracker) | run time | none |
| PCRE2 | bytecode interpreter, JIT optional | run time | only when the program asks |
| Java, Python | node tree / bytecode interpreter | run time (Python caches) | none |

No system compiles every regex to native code ahead of time. The closest to js2wasi3 (an ahead-of-time JS
engine without run-time code generation) is Hermes: build-time regex bytecode and an interpreter. Every fast
engine also skips start positions before matching (literal prefilters, first-unit sets, Boyer-Moore lookahead).

Sources: [Hermes RegExp](https://github.com/facebook/hermes/blob/main/doc/RegExp.md),
[V8 tier-up](https://v8.dev/blog/regexp-tier-up),
[SpiderMonkey's new engine](https://hacks.mozilla.org/2020/06/a-new-regexp-engine-in-spidermonkey),
[JSC regexp](https://trac.webkit.org/wiki/JSCRegExpProcessingAndJSCGoals),
[Rust `regex!`](https://docs.rs/regex/0.1.53), [PCRE2 JIT](https://man7.org/linux/man-pages/man3/pcre2jit.3.html),
.NET and regex-automata: links above.

## Design (approved by the user, 2026-10-13, after "What proven systems ship")
As Hermes: each literal in the subset is compiled at build time into a program of 16-bit words (a string
constant: header with the search data, ops, classes; format in `lower/src/regex.rs`), run by one interpreter in
Wasm (`backend/src/regex.rs`, `re_exec`) instead of regexp.js's JS VM. No code per literal. The search (`^`, a
required literal, the units at fixed offsets) is data in the header. The rest of the section below is the first
design (native code per literal): built, measured (octane-regexp −82.7% but `.wasm` +48.6%), and replaced.

## First design (native code per literal; replaced)
- **Parser: oxc's regex AST.** The frontend already parses every literal's pattern (`parse_regular_expression:
  true`, `crates/frontend/src/lib.rs`), so `RegExpLiteral.regex.pattern.pattern` is a full tree
  (`oxc_regular_expression::ast`: `Disjunction`/`Alternative`/`Term`, quantifiers with `min`/`max`/`greedy`,
  classes with ranges and escapes, groups, references, lookarounds, modifiers). No second parser in Rust. The
  semantics (what a class contains, case folding, Annex B quirks) stay the prelude's: anything whose meaning isn't
  obviously the same falls back to the VM, and `--regex-vm` differential runs check the rest.
- **Crates:** the backend depends only on `ir`, not on oxc. The lowering converts oxc's tree into a small
  normalized `ir::Regex` (sequence, alternation, char, class as sorted `[lo, hi]` ranges, group, quantifier,
  anchors, `\b`) or `None` (unsupported: the VM). `ir::Module::regexes` lists distinct `(source, flags)` ones; the
  backend's new `regex.rs` emits one function per entry.
- **Interface (as T78's `re_alts_find`):** `re_<k>(s: ref $string, from: i32, mode: i32, caps: ref $i32s) -> i32`:
  mode 0 searches from `from`, mode 1 tries `from` only (sticky, and `split`'s anchored runs); the result is the
  match end or −1, and `caps` gets every slot as `$$reRun` leaves it (−1 for unset). The literal becomes
  `$$RegExpCreate(source, flags, k + 1)` (0: none); `$$reCompile`'s `code` array gets a slot 12 with `k + 1`
  (set also on a cache hit from an earlier `new RegExp` of the same source). `$$reSearch` and the anchored
  `$$reRun` callers call `__intrinsic_re_compiled(k, s, from, mode, caps)` (a `br_table` over direct calls) when the
  slot is set. Everything else (`lastIndex`, `exec` arrays, `replace`, `split`, named groups, T81's fast paths)
  stays in the prelude. The prelude still compiles the pattern once per run (fallback paths and `code`'s other
  slots need it).
- **Start search** (.NET's order): `^` without `m` tries `from` only; a leading literal: scan for its first unit,
  then compare the rest; else the first-unit set as an ASCII bitmap (constants in the code) plus ranges; and a
  **required literal** (regex-automata's inner literal: a string every match contains, found from the tree): one
  search for its next occurrence `q`; none means no match from here on, and starts past `q` search again.
- **Matcher:** compiled per node with continuations (irregexp, .NET): a node's code runs, then its successor's.
  Failure backtracks: pop a resume number from an explicit `i32` stack (a global `$i32s`, grown like `$$reStack`)
  and jump there through one `loop` + `br_table` over the resume points (Wasm has no computed jump; .NET's IL
  `switch` does the same). Captures and loop registers are locals, restored from undo entries on the stack (the
  VM's scheme). Without a stack: a greedy one-character loop scans ahead and gives back one unit per backtrack
  (one entry holding its start); with **auto-atomicity** (the next node can't start with a unit the loop takes,
  or the loop ends the pattern) it pushes nothing. A quantified group keeps the VM's count/start registers and the
  spec's empty check.
- **Classes:** ASCII part as a 128-bit bitmap in `i64` constants, units above 127 by ranges (unrolled compares
  when few, else a binary search: F179).
- **First version's scope** (decided per pattern at build time; the rest runs in the VM): characters, classes and
  escapes (`\d\w\s` and negations), `.`, `^`/`$` (with `m`), `\b`/`\B`, capturing and non-capturing groups,
  alternation, greedy and lazy quantifiers; flags `g`, `y`, `d`, `m`, `s`, and `i` when every character and class
  in the pattern is ASCII (as T78: then only ASCII letters fold; a non-ASCII unit can't fold into ASCII without `u`).
  Not yet (follow-ups): `u`/`v`, backreferences, lookaround, modifier groups, `i` with non-ASCII.
- **Correctness:** the VM and node are the oracles: `js2wasi3 build --regex-vm` keeps every literal in the VM, so
  the regex fixtures and the RegExp part of test262 run with both engines and must give the same output.

## Plan
- [x] 1. Design notes (above): oxc AST, `ir::Regex`, T78-style interface, required literal, `br_table` backtracking.
- [x] 2. A micro-benchmark `tests/bench/micro/regex.js` before any code: per pattern kind (literal prefix, leading
  class loop, alternation start, inner literal only, anchored, captures, `i`, no-match long input, many short
  inputs); node and the VM numbers as the baseline. Not tuned to octane's patterns.
- [x] 3. Merged into 4 (2026-10-13): a separate search would call the VM per candidate and rescan for the required
  literal each time; the search lives in the compiled function.
- [x] 4. The matcher for the first-version scope; VM fallback for the rest. (`--count` counters: not done; the
  prelude has no counter intrinsic; `wasm-tools print | grep '$re_'` counts the compiled literals.)
- [x] 5. Differential runs: regex fixtures with both engines; test262 `built-ins/RegExp` + `String.prototype`
  regex methods (laptop subset), then full test262 on a-03 (ask).
- [x] 6. Gate (`scripts/gate.sh`) on a-03 (ask); `.wasm` size and `wasmtime compile` time of the 18 programs
  (F178).

## Acceptance
- `scripts/gate.sh`: no program slower (2% on minimums), no micro case slower (1 ns and 3%); octane-regexp and
  regexredux faster; test262 no changes; the same outputs with `--regex-vm`.
- Compile time and `.wasm` size reported per program; a growth over 10% needs the user's yes.

## Notes / Result
- What changed (the approved design, after the first, native-code design was measured and dropped):
  - `lower/src/regex.rs`: a regex literal's oxc tree → `ReNode` (subset: no `u`/`v`, backreferences, lookarounds,
    modifier groups; `i` only when folding is ASCII letters; `\c` without a letter stays out: oxc 0.151 misreads
    Annex B's `[\c_]`) → a program of 16-bit words: header (slots, captures, flags, first op, the search data:
    up to 4 sets of units at fixed offsets, a required literal and its offset), ops (MATCH, CHAR, CLASS, STR,
    BOL, EOL, WORDB, JMP, SPLIT, SAVE, CLEAR, STAR, LAZY, REP_INIT/LOOP/ITER/END), classes (ASCII bitmap +
    ranges). Analyses: auto-atomic greedy loops (.NET), fixed-offset sets (irregexp, .NET), required literals
    (regex-automata).
  - `ir`: `Module::regexes` (the programs), `regex_vm`; intrinsics `RegexId(k)` (the program as a string
    constant, `null` with `build --regex-vm`) and `ReExec`.
  - `backend/src/regex.rs`: one interpreter `re_exec(prog, s, from, mode, caps)`: the search (`^` → 0 only; the
    required literal, scanned by its first unit, and a jump to it at a known offset; the smallest set as an i64
    mask pair in a tight loop, the other sets at its hits), then a `br_table` dispatch over the ops with an
    explicit backtrack stack (`G_RE_STACK`, kept between calls; entries choice / give-back / lazy-more / undo).
    Helpers `re_class_wide`, `re_grow`. Slots (captures, loop registers) live in `caps[2..]`.
  - Prelude (`regexp.js`): `$$RegExpCreate(src, flags, prog)` keeps the program in `code[12]` (and grows `caps`);
    `$$reSearch` and `split`'s anchored runs (`$$reRunAt`) call `__intrinsic_re_exec` when it's set.
  - `tests/bench/micro/regex.js`: 13 cases by pattern kind. CLI: `build --regex-vm`.
- Results (a-03, `scripts/bench.sh -r 3`, outputs = node): octane-regexp 21.90 → 4.33 s (56.7× → 10.5× node),
  regexredux 2.98 → 1.94 s (6.8× → 4.5×). `.wasm`: octane-regexp 579,052 → 623,220 bytes (+7.6%), regexredux +0.5%. Laptop
  instructions per micro case vs the VM: 12 of 13 lower by 5–98%, `cat|dog|bird` +0.4%.
- Checks: differential fuzz (random patterns from the subset × 15–20 inputs, `exec`/`replace`/`split`) 0
  differences from node; the regex fixtures pass; full test262 on a-03 (`~/t262-t114b.log`): no changes (28,105
  pass, 97.0%).
- Lesson (user): "consult proven implementations first" means the research decides the design. The first
  survey looked only at matching technique and missed that no proven system compiles every regex to native code
  ahead of time (table "What proven systems ship").
- Not done / next: F415 (patterns outside the subset), F416 (the JS around each call), F417 (pruning impossible
  branches), F419 (leftovers, SIMD scan); a native matcher for a few hot literals within a size budget only if a
  benchmark asks for it.

## Progress
- 2026-10-13: created after the octane-regexp profile on a-03 (measurements above). Prior art read: irregexp,
  Yarr (from `docs/bun-jsc-analysis.md`), the .NET 7 source generator, regex-automata's literal prefilters, Rawr,
  Porffor/QuickJS. The user chose "compile literals to Wasm" (2026-10-13) over a cheaper VM or a prefilter alone.
- 2026-10-13: started (status in progress). Step 1 done: design above. Read: `$$reSearch`/`$$reRun`/`$$reExec`
  (the contract: end or −1, `caps` filled), `re_alts_find` (T78, the same contract in Wasm), oxc 0.151's
  `oxc_regular_expression::ast` (already parsed by the frontend), the crate split (backend sees only `ir`).
- 2026-10-13: step 2: `tests/bench/micro/regex.js`, 13 cases (start search: literal, inner literal only, leading
  class loop, alternation, anchors; matcher: captures, lazy, `i`, many matches, `split`; short inputs), ~40 ms a case
  in the VM (2.7 s a run). Baseline, laptop, ns per call, VM (node, first version with 5–100× the counts: Date.now
  resolution, so coarse): literal match at the end 11,735 (49); literal no match 10,985 (49); inner literal only
  759,985 (~0); `\d+px` 65,985 (499); `cat|dog|bird` 16,485 (499); trim replace 199,985 (5,199); captures exec
  135,985 (1,999); `/GAMMA/i` 1,785 (99); lazy replace 45,985 (999); `/a/g` replace 135,985 (6,999); `split`
  169,985 (17,999); short `/=/` 785 (29); short `^(\w+)=(\w*)$` 1,485 (34).
- 2026-10-13: steps 3+4, first version built. `ir::Regex`/`ReNode`, `Module::regexes`/`regex_vm`, intrinsics
  `RegexId`/`ReCompiled`; `crates/lower/src/regex.rs` (oxc tree → `ReNode`; `oxc_regular_expression` added as a
  direct dependency, already in the tree through `oxc_ast`, same version); `crates/backend/src/regex.rs` (search
  with `^`, required literal, first-unit set; states + `br_table` backtracking, auto-atomic greedy loops, undo
  log); `re_dispatch`, `re_grow`; prelude: `$$RegExpCreate(src, flags, id)`, `code[12]`, `$$reSearch` and
  `split` (`$$reRunAt`) call it; `build --regex-vm`. oxc deviation found: `[\c_]` / `[\c1]` (Annex B) read
  wrongly: a `\c` without a letter stays in the VM. Checks: the smoke test = node; `regex-*` fixtures,
  `regexp-modifiers`, `watch-iter`, `string-normalize`, `value-kinds`, `node/util`, `commonjs/ws` pass;
  differential fuzz (`gen.mjs` in the scratchpad: random patterns from the subset, 15 inputs each, `exec`, and
  `replace`/`split` for `g`): 10 seeds × 400 patterns, ~85% compiled, 0 differences from node.
- 2026-10-13: micro, laptop, ns per call, `--regex-vm` / compiled: literal at the end 13,235 / 9,235; literal no
  match 11,735 / 3,485; inner literal only 839,985 / ~0; `\d+px` 85,985 / 7,985; alternation 24,985 / 10,485;
  trim replace 349,985 / 34,985; captures 223,985 / 7,985; `/GAMMA/i` 2,885 / 1,785; lazy 71,985 / 29,985;
  `/a/g` replace 199,985 / 127,985; split 244,985 / 124,985; short `/=/` 1,160 / **1,235 (slower)**; short
  `^(\w+)=(\w*)$` 2,235 / 1,635. Next: why short `/=/` is slower; what's left per call (literal creation,
  `exec` arrays, `replace`'s pieces).
- 2026-10-13: laptop timings, one run each, outputs = node. octane-regexp (`fixed 0.05`): VM 35,650 ms →
  compiled 6,293. regexredux (500k): VM 3,979 → 3,826 (its 9 `agggtaaa|tttaccct`-style alternations over DNA:
  the first-unit set lets every other position into the matcher; profile: `re_*` 62%). Added the units at fixed
  offsets (irregexp's Boyer-Moore lookahead, .NET's fixed-offset sets: up to 8 offsets from the pattern's
  fixed-width start, the 4 smallest sets tested): regexredux 2,305, octane-regexp 6,261. Then `$$reSearch` takes
  the compiled branch before reading the VM's filter data (`code[0]`, `code[10]`, …; generic `get`s were 10.9% of
  octane-regexp): octane-regexp 5,868, regexredux 1,871. Fuzz (10 seeds) and the 12 regex fixtures still pass.
  octane-regexp's profile now: `re_*` ~25%, the rest the JS around them (`get` 10.9%, `$$reSearch` 5.6%,
  `to_boolean`, `exec` arrays, `$$RegExpCreate` per evaluation 4.8%).
- 2026-10-13: `cargo test`: `js2wasi3-backend --test emit` fails 4 tests ("WebAssembly translation error") with
  and without T114's changes (checked with them stashed): not T114's; recorded as a follow-up.
- 2026-10-13: required literal at a fixed offset k of every match (in the fixed-width start): the search jumps to
  its next occurrence minus k (micro "literal at the end" 11,986 → 8,227 ns; the scan is ~3 ns a unit, F102's SIMD
  would cut it). Fuzz with five 30–80-unit inputs (`gen2.mjs`): found that `i` rejected `.`/`[^a]`/`\W`/`\S`
  (the closure check looked at the gaps of the set before negation; the spec folds the set, then inverts):
  fixed, both "only caseless above 127" and "all but caseless above 127" are allowed. Then 0 differences on
  every finished run (short seeds 1–10; long seeds 11, 16, and 18's 221 results before its stall). Seeds 12–15
  and 17 time out in node itself (exponential patterns). Seed 18 stalls on `/(?: [^]{0,2}?b|1{0,2}.{1,3}?)*?c^/g`:
  exponential in both of our engines (compiled ~4× faster than the VM), node 0 ms: V8 sees that `c^` can't match;
  not T114's (follow-up).
- 2026-10-13: the long-fuzz background loop left node/wasmtime processes running a full core (seeds 12–14) during
  the timings after 07:50: those laptop timings (octane-regexp 5.9–7.6 s, regexredux 1.9–3.1 s) were inflated;
  stopped (TaskStop + kill by PID). Quiet machine: regexredux 1,704 / 1,906 ms, octane-regexp 4,135 / 5,388 ms
  (VM: 3,979 and 35,650 earlier, also possibly inflated; the a-03 gate decides). Micro, quiet, `--regex-vm` /
  compiled: every case faster (short `/=/` 1,159 / 1,084; captures 195,984 / 3,984; inner literal 839,984 / ~0).
  Next: the a-03 gate and test262 (asked).
- 2026-10-13: gate on a-03 against `58dc1b0` (`out/gate-t114-a03.log`): octane-regexp 21,634 → 3,745 ms (−82.7%,
  instructions −82.9%), regexredux 2,978 → 1,527 (−48.7%, −63.3%), knucleotide/revcomp/earley-boyer same, the
  other 13 programs byte-identical `.wasm`; outputs = node. Micro: every regex case faster (short `/=/` 859 → 759,
  captures 155,984 → 3,984, inner literal 879,984 → ~0), object-kinds/primitives/property-paths all same.
  Size and `wasmtime compile` (min of 3) base → new: octane-regexp 579,052 → 860,544 bytes (+48.6%), 398 → 464 ms
  (+16.6%); regexredux +1.4%, 629 → 651 ms; knucleotide/revcomp/earley-boyer ≤ +0.3%, compile same. Over 10% for
  octane-regexp (235 compiled literals): the user's call (acceptance).
- 2026-10-13: full test262 on a-03 (`~/t262-t114.log`, 1,424 s): no changes since the baseline (28,105 pass,
  97.0%). The laptop RegExp subset run was replaced by the differential fuzz (4,000+ patterns, 0 differences)
  and the full run. Open: the user's call on octane-regexp's size (+48.6%) and compile time (+16.6%).
- 2026-10-13: size investigation (user: "579 KB to 861 KB, this is really huge"). octane-regexp has 282 distinct
  literals (4,954 pattern characters, 17.6 on average), 235 compiled: the `re_<k>` functions are 271 KB of the
  282 KB growth (1,155 bytes average, median 824, smallest 213, largest 11,247: the 167-character `/(^|.)(ronl|
  qri-ehf3.wbg)(…)…$/i` with ~45 alternatives). Fixed part per function ~300 bytes (`/a/` 298: search loop,
  dispatch, backtracking state, captures out); then ~50 bytes per pattern character: a unit test ~25 bytes (bounds
  check, read, compare, fail), an inline push ~55, a capture write ~80 (its undo push). Search parts are small:
  required literal 18 KB, fixed-offset sets 9 KB (measured with temporary switches, removed). Two changes tried
  in the working tree: one failure exit per state (`br_if` instead of `if`/set/`br`/`end`; a jump to the next state
  is a `br` out of its blocks) and pushes through a shared `re_push`: 861 → 810 KB (−6%), instructions (laptop
  `perf stat`) octane-regexp +2.8%, regexredux +1.1%; fuzz and fixtures pass. Not yet decided (asked the user).
- 2026-10-13: user: the design must come from what proven systems ship, not from my reading of the plan. Step 1
  redone (table "What proven systems ship"): none compiles every regex to native code ahead of time; Hermes
  (the closest) ships build-time bytecode and an interpreter. Recommendation sent to the user; no code changes.
- 2026-10-13: user approved the Hermes-like design; implemented. Lowering: oxc tree → `ReNode` (moved from `ir`
  into `lower/src/regex.rs`) → program (header: slots, captures, flags, first op, need, up to 4 fixed-offset sets,
  required literal and its offset; ops MATCH, CHAR, CLASS, STR, BOL, EOL, WORDB, JMP, SPLIT, SAVE, CLEAR, STAR,
  LAZY, REP_INIT/LOOP/ITER/END; classes as an ASCII bitmap plus ranges); `Module::regexes` holds the programs;
  `RegexId(k)` is the program's string constant (`null` with `--regex-vm`). Backend: `re_exec` (one function:
  search, `br_table` op dispatch, backtrack stack `G_RE_STACK` kept between calls, slots in `caps[2..]`),
  `re_class_wide`, `re_grow`; the per-literal functions are gone. Prelude: `code[12]` is the program (null: VM);
  `caps` grows to the program's slots. Bug found by a fixture: `single()` used `then_some(r[0].0)` (eager) on an
  empty class `[]`: fixed.
- 2026-10-13: tuning the search, measured by instructions per micro case (laptop `perf stat`, VM vs interpreter):
  the sets loaded into locals once per call and the smallest set tested first in a tight loop (as an i64 mask
  pair), the others only at its hits; the end limit computed once; the required literal scanned by its first unit.
  Result per micro case: literal at the end −40%, no match −42%, inner literal −98%, `\d+px` −88%,
  `cat|dog|bird` +0.4%, trim −56%, captures −77%, `/GAMMA/i` −5%, lazy −45%, `/a/g` −19%, split −44%, short `/=/`
  −7%, short captures −40%. (A first try testing all 4 sets at every position without branches helped random DNA
  but cost +21% on `cat|dog|bird`.)
- 2026-10-13: laptop, quiet, outputs = node: octane-regexp VM 31,164 ms / 205.1G instructions → 7,456 ms / 42.5G
  (−76%); regexredux 4,232 / 26.9G → 3,054 / 17.8G (−28%). Size: octane-regexp 623,220 bytes (base 579,052:
  +7.6%; the native version was 860,544), regexredux 1,605,688 (+0.5%). Fuzz (10 short seeds, long 11 and 16)
  0 differences; the 12 regex fixtures pass. Next: the a-03 gate and test262 (to ask).
- 2026-10-13: a-03 `scripts/bench.sh -r 3` (`~/bench-t114`, for the compatibility page; user asked): all 18 outputs
  = node; octane-regexp 4.33 s (10.5× node; page: 21.90 s, 56.7×), regexredux 1.94 s (4.5×; page 2.98 s). Full
  test262 on a-03 (`~/t262-t114b.log`, 1,425 s): no changes since the baseline (28,105 pass, 97.0%). Done; commit
  approved by the user.
