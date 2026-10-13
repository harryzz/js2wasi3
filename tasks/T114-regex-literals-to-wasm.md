# T114: regex literals compiled to Wasm at build time (root R6, F336)

Status: proposed | Depends on: T27, T39, T78, T81 | Local (+ a-03 only after asking)

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

## Design (to decide in step 1, record in D5)
- **Where:** the backend emits Wasm with `wasm-encoder` (no new dependency, no WAT), one pair of functions per
  distinct `(source, flags)` literal; the parse reuses oxc's regex AST (`oxc_regular_expression`, already
  pulled in by `oxc_ast`; the lowering sees the literal in `crates/lower/src/lib.rs` `Expression::RegExpLiteral`) or
  a Rust port of the prelude parser (decide; the prelude parser decides V8's semantics today).
- **Interface to the prelude:** the literal passes a funcref (or an index) to `$$RegExpCreate`; it rides in the
  compiled `code` array (a new slot). `$$reSearch`/`$$reExec` call it when present: `find(s, from) -> start | -1`
  and `match(s, pos, caps) -> end | -1` filling the same capture `i32` array as `$$reRun`. `lastIndex`, flags,
  `exec` results, `replace`/`split` stay in the prelude unchanged (T81's fast paths keep working).
- **Start search** (`find`): anchors (`^` without `m`: one try), a required literal (prefix, then inner, as
  regex-automata) searched with the runtime's string search, a first-unit set as a 128-bit ASCII bitmap plus ranges.
- **Matcher** (`match`): straight-line Wasm per node. Greedy/lazy one-character loops as local counters (walk back
  without a stack); general backtracking (into groups, alternation inside loops) through an explicit `i32` stack of
  resume-point numbers dispatched by one `br_table` in a loop (the .NET switch in IL; irregexp's address stack).
  Auto-atomic loops and first-character alternation switches as in .NET.
- **Classes:** an ASCII bitmap test, ranges by binary search only above 127 (F179).
- **Scope of the first version** (the rest falls back to the VM, decided per pattern at build time): characters,
  classes, `.`, anchors, `\b`, groups and captures, alternation, greedy and lazy quantifiers, flags `g`, `y`, `i`
  (ASCII and the existing case tables), `m`, `s`. Later (follow-ups): backreferences, lookaround, `u`/`v` mode,
  named groups beyond index mapping.
- **Correctness oracle:** the VM and node. A build flag forces the VM (`--regex-vm`) so every regex fixture and the
  RegExp part of test262 can be run with both engines; outputs must be identical.

## Plan
- [ ] 1. Design notes: oxc AST vs prelude parser, the prelude interface, the backtrack scheme; record in D5.
- [ ] 2. A micro-benchmark `tests/bench/micro/regex.js` before any code: per pattern kind (literal prefix, leading
  class loop, alternation start, inner literal only, anchored, captures, `i`, no-match long input, many short
  inputs); node and the VM numbers as the baseline. Not tuned to octane's patterns.
- [ ] 3. Start search only (`find`), the VM still matches: measure (this alone covers the no-match case above).
- [ ] 4. The matcher for the first-version scope; VM fallback for the rest; `--count` counters (compiled / VM).
- [ ] 5. Differential runs: regex fixtures with both engines; test262 `built-ins/RegExp` + `String.prototype`
  regex methods (laptop subset), then full test262 on a-03 (ask).
- [ ] 6. Gate (`scripts/gate.sh`) on a-03 (ask); `.wasm` size and `wasmtime compile` time of the 18 programs
  (F178).

## Acceptance
- `scripts/gate.sh`: no program slower (2% on minimums), no micro case slower (1 ns and 3%); octane-regexp and
  regexredux faster; test262 no changes; the same outputs with `--regex-vm`.
- Compile time and `.wasm` size reported per program; a growth over 10% needs the user's yes.

## Notes / Result

## Progress
- 2026-10-13: created after the octane-regexp profile on a-03 (measurements above). Prior art read: irregexp,
  Yarr (from `docs/bun-jsc-analysis.md`), the .NET 7 source generator, regex-automata's literal prefilters, Rawr,
  Porffor/QuickJS. The user chose "compile literals to Wasm" (2026-10-13) over a cheaper VM or a prefilter alone.
