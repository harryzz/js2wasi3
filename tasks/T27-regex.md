# T27: `RegExp` and regex literals

Status: todo (approved 2026-09-28, not started) | Depends on: T21 (prelude), T25 slice 5 (well-known symbols), T36
(prelude intrinsics pattern: `call_list`, `to_key`, `object_desc`)

## Goal
Regex literals (`/a+/gi`, F78) and `RegExp`: `new RegExp(pattern, flags)`, `exec`, `test`, `lastIndex`, flags
`d g i m s u v y`, `source`/`flags` accessors, `toString`; the string methods that take a regex: `match`,
`matchAll`, `search`, `replace`/`replaceAll` (with `$1`/`$<name>`/`$&` and function replacers), `split` (F87),
dispatched through `Symbol.match`/`replace`/`search`/`split`/`matchAll`. `console.log` prints regexes like node.

## Design questions (decide first, record in D5)
- **Engine**: a backtracking matcher (ECMAScript semantics: ordered alternation, lazy/greedy quantifiers,
  lookahead/lookbehind, backreferences, named groups, `u`/`v` Unicode mode). Options: (a) a pattern compiler +
  interpreter written in the prelude (JS, like the other built-ins); (b) compile literal patterns at build time in
  Rust to Wasm functions (fast, but `new RegExp(string)` still needs (a)); (c) a mix. No new heavy dependencies
  in the backend (user preference).
- **Unicode tables**: `\p{…}` and case folding for `i`+`u` need data: generate compact tables, only linked when used.
- **Early errors**: invalid literal patterns are SyntaxErrors at parse time (oxc checks them); `new RegExp` throws
  node's messages at run time.
- **Catastrophic backtracking**: same behaviour as V8 (no limit), but deep recursion must not become a Wasm stack
  overflow trap (an explicit backtrack stack).

## Decision (agreed with the user, 2026-09-28)
- **Engine: (a)**, a backtracking matcher written in the prelude (a new `crates/lower/builtins/regexp.js`, added to
  `PRELUDE` in `crates/lower/src/builtins.rs` like `reflect.js`): parse the pattern once into a compact program (an
  array of numeric ops, cached on the RegExp object), then run it over the string's code units with an **explicit
  backtrack stack** (a JS array of saved positions/states), never JS recursion, so a catastrophic pattern is slow
  like V8 but can't overflow the Wasm stack. Compiling literals at build time to Wasm (option b) is a later
  optimisation only if a benchmark asks for it (record it as a follow-up).
- The RegExp object: new native storage only if the prelude can't do it with an ordinary object plus internal slots
  kept out of reach (look at how `$map`/`$typed`/`$wrapper` did it: T25/T20/T30); `console.log` of a regex needs
  `inspect.rs` to recognise it (node prints `/a+/gi`). Only programs that use a regex (a literal, `RegExp`, or a
  string method with `Symbol.match`/`replace`/… names) may link the engine (`builtins::Prelude` reachability,
  `ir::Module::names` gating as in T25).

## Plan (slices)
1. **Core engine + literals**: pattern parser (literal chars and escapes, `.`, classes `[...]`/`[^...]` with ranges and
   `\d\w\s` + negations, anchors `^ $ \b \B`, groups: capturing, named `(?<n>…)`, non-capturing, alternation,
   quantifiers `* + ? {n,m}` greedy and lazy); flags `g i m s y` (`i` without `u`: simple case folding of the code
   units the string runtime already knows, F87's case tables); `RegExp(p, f)` / `new RegExp`, `exec` (result array with
   `index`, `input`, `groups`), `test`, `lastIndex`, `source`/`flags`/`global`/… accessors, `toString`; regex
   literals in the lowering (today "unsupported: expression `RegExpLiteral`"; oxc already reports invalid literal
   patterns as early errors; each evaluation of a literal makes a new object); node's SyntaxError texts for
   `new RegExp` (check them with node 24.21.0); `console.log` prints them like node.
2. **String methods**: `match`, `matchAll`, `search`, `replace`/`replaceAll` (`$1`, `$<name>`, `$&`, `` $` ``, `$'`, `$$`,
   function replacers) and `split` with a regex, through `Symbol.match`/`matchAll`/`replace`/`search`/`split`
   (the string versions in `builtins/string.js` today: `$$String_prototype_split`/`replace`/`replaceAll` take strings
   only, F87); `RegExp.prototype[Symbol.*]` in the prelude.
3. **The rest**: lookahead/lookbehind, backreferences (numbered and named), `u`/`v` mode (code points, `\u{…}`,
   `\p{…}` with generated compact Unicode tables linked only when used, `v`'s set operations), case folding for `i`+`u`,
   the `d` flag's `indices`, `RegExp.prototype.flags` details, `lastIndex` edge cases, `RegExp[Symbol.species]`.
4. **test262**: first the non-passing subset tests that use regexes (find them as T36 did: grep the baseline's
   non-passing tests for regex literals/`RegExp`; ~190); then **ask the user** before adding `built-ins/RegExp` (and
   possibly `String.prototype.{match,…}` tests already in the subset) to `SUBSET`/`scripts/fetch-test262.sh` (the
   baseline totals change). New causes → follow-ups.

## Read first
- F78, F87 (and F109: names built at run time) in `tasks/README.md`; T25's Notes (well-known symbols, `names`
  gating, built-ins as values), T36's Notes (prelude intrinsics, `$$isObject`-style helpers in `reflect.js`).
- `crates/lower/src/builtins.rs` (prelude conventions), `crates/lower/builtins/string.js` (string intrinsics:
  `__intrinsic_unit`, `sub`, `str_len`; the current `split`/`replace`), `crates/lower/src/lib.rs` (`intrinsic_call`,
  where `RegExpLiteral` is rejected: `expression_kind`), `crates/backend/src/inspect.rs` (printing),
  `crates/backend/src/mapset.rs` (an example of native storage behind prelude methods).

## Tests
Fixtures `tests/fixtures/builtins/regex-*.js` (`scripts/diff.sh`; node is the oracle; keep each under 2 s),
`cargo test -p js2wasi3-lower` (the `RegExpLiteral` cases in `crates/lower/tests/unsupported.rs` change),
`wasm-tools validate` + one wasmtime run; test262 as in slice 4.

## Acceptance
- [ ] Fixtures in `tests/fixtures/builtins/regex-*.js` match node (`scripts/diff.sh`), including the string methods.
- [ ] Programs without regexes link none of the engine.
- [ ] F78 (regex literals) and the regex parts of F87 ticked.

## Notes / Result

## Checklist
- [x] Slice 1: engine, literals, `RegExp`/`exec`/`test`/flags/accessors, printing (also lookaround, backreferences, basic `u`)
- [x] Slice 2: string methods through `Symbol.*`
- [ ] Slice 3: lookaround, backreferences, `u`/`v`, `\p{…}`, `d`
- [ ] Slice 4: test262 (subset change asked first), follow-ups, baseline, docs (`docs/keywords.md`, README)

## Progress
- 2026-09-28: handoff written after T38 (the user chose RegExp next; engine decision above agreed). Not started.
  State at handoff: `main` = origin at `040bfdc`; test262 subset 22,211 of 24,973 run pass (88.9%); whole
  `diff.sh` 120 pass.
- 2026-09-28: started slice 1. Design: a RegExp is an ordinary `$object` of class `CLASS_REGEXP` (10) with
  prototype `RegExp.prototype` (new `Builtin::RegExp`), its internal data (`[source, flags, compiled]`) under the
  private `G_SYM_STATE` key and an own `lastIndex` (writable, hidden, fixed). Intrinsics `regexp_new(data)` and
  `regexp_data(v)` (the data; `null` for `RegExp.prototype`; `undefined` otherwise). `builtins/regexp.js`: a parser
  to an AST of arrays, a compiler to a numeric program, a VM with an explicit backtrack stack (3 numbers per entry:
  choice `[pc, pos, 0]`, capture/register undo `[-1|-2, i, old]`, lookaround barrier `[-3|-4, pos, pc]`);
  quantifiers use registers (count + iteration start, undo-logged) with the spec's empty check; lookaround is
  atomic (LOOK_END drops the choices above the barrier, keeps the undos). A literal is `$$RegExpCreate(src, flags)`;
  compiled programs are cached by `flags/source`.
- 2026-09-28: plumbing done (IR `Builtin::RegExp`, `Intrinsic::RegExpNew/RegExpData`; lowering of `RegExpLiteral` via
  `prelude_call`, `new RegExp` → `$$RegExp`, prelude root for literals; backend helpers in `object.rs`, getters list
  `REGEXP_GETTERS`, printing in `inspect.rs` gated on the module name `$regexp`). Prelude globals must start as
  `null`/numbers (a string initializer is "not a literal" there). First smoke (`test`/`exec`/printing) matches node.
- 2026-09-28: engine correct on a broad check (alternation order, backtracking into groups, captures reset per
  iteration, empty checks, lookahead/lookbehind, backreferences incl. named/duplicate names, `i`, `m`, `s`, `y`, `g`,
  `d`, astral `u`), V8's SyntaxError texts for `new RegExp`. Performance work (benchmarks in `out/t27/`, min of 3
  runs): the VM's numbers were boxed `jsval`s; now (1) `+` on array reads keeps them `f64`; (2) new prelude
  intrinsics `i32_new/get/set/len` over the runtime's `$i32s` (program, class ranges, captures, registers, a shared
  backtrack stack); (3) the backend inlines ToInt32 when |x| < 2^31 (`i32.trunc_f64_s`, one scratch `f64` local per
  function; was a `$to_int32` call per conversion: 59% of a profile); (4) flags/`ok` as 0/1 numbers (boolean
  locals are boxed, F14); (5) `STAR` instruction for greedy one-character loops; (6) `G_REGEXP_EXEC` + intrinsic
  `regexp_builtin_exec`: `test` skips the match array while `exec` is the built-in. `(a+)+b` on 19 `a`s: 9 s →
  0.68 s; `re.test` 44 µs → 2 µs; a literal's evaluation 14 µs → 4 µs (compile cache).
- 2026-09-28: slice 1 done. Fixtures `builtins/regex-engine.js` (matching semantics) and `builtins/regex-object.js`
  (constructor, flags, accessors, `toString`, printing, V8's error texts, `lastIndex`, a user `exec`) pass;
  `cargo test` (all crates) passes; whole `diff.sh`: 122 pass (120 + the 2 new), 0 fail. Programs without a regex
  link no engine code (only `RegExp.prototype` methods whose names the code uses, as for every prototype).
  Also done early (slice 3 items that came with the engine): lookahead/lookbehind, backreferences (numbered, named,
  duplicate names in alternatives), `u` code points and `\u{…}`, the `d` flag's `indices`.
  Left for later slices / follow-ups: `RegExp(re)` without `new` returns a copy (spec: `re` itself when its
  `constructor` is `RegExp`: `$$RegExp` can't tell a call from `new`); `\p{…}` (a SyntaxError naming js2wasi3
  now); `i`+`u` case folding beyond string.js's case tables; `v` set operations; `lastIndex` inside a surrogate
  pair; subclassing / `Symbol.species`. Next: slice 2 (string methods through `Symbol.*`).
- 2026-09-28: slice 2 implemented: `RegExp.prototype[Symbol.match/matchAll/replace/search/split]` as
  `$$RegExp_prototype_$match` … (the backend installs a prelude method key `$name` under `Symbol.name`), full
  GetSubstitution (`$n`/`$nn`/`$<name>`), function replacers with named groups, `split` with captures and limits
  (a direct matcher loop when the splitter's `exec` is the built-in), `matchAll` as a prelude generator;
  `String.prototype.match/matchAll/search` in regexp.js; `split`/`replace`/`replaceAll` in string.js dispatch
  through `Symbol.split`/`Symbol.replace` first. Linking: the symbol methods hang off `$$reCreate` only when the
  user's code names the method (`REGEXP_SYMBOL_METHODS` in builtins.rs), `Symbol.x` in the prelude no longer counts
  as naming `x`, and `replaceAll`'s `flags` read goes through a variable key, so string-only `split`/`replace`
  programs link 3 small helpers and no engine. A broad check of the string methods (out/t27/h.js) matches node.
- 2026-09-28: slice 2 done. Fixture `builtins/regex-strings.js` passes; `replace` joins its pieces once (repeated
  `+` was quadratic: 90% of a 5,000-match replace); `crates/js2wasi3/tests/build.rs`'s trap test used
  `String.prototype.match` as its unimplemented method: now `anchor`. `cargo test -p js2wasi3-lower -p js2wasi3`
  passes. Known deviation: the `matchAll` iterator is a prelude generator (prints `Object [Generator] {}`, tag
  `Generator`; node: `RegExp String Iterator`).
- 2026-09-28: slice 3 started. oxc didn't check regex literals (the task assumed it did): the frontend now sets
  `ParseOptions::parse_regular_expression` (invalid literals are build errors). `scripts/gen-unicode.mjs` generates,
  from node 24.21.0 (Unicode 17.0) and the UCD 17.0 alias/case-folding files, `builtins/unicode-case.js` (the `i`
  flag's Canonicalize without `u` and simple case folding with `u`, as runs; always in the prelude, 10 KB) and
  `crates/lower/src/unicode_props.rs` (1,715 `\p{…}` texts node accepts → 441 code point sets). `i` is now exact:
  Canonicalize by table; a class under `i` compiles to its case-closed image and matches on the canonical form;
  `\W` with `iu` excludes ſ/K. Exhaustive check of 21 `i` patterns over the BMP (out/t27/k.js) matches node.
- 2026-09-28: `\p{…}`/`\P{…}` (u and v): `builtins::parse` scans the source text for `\p{text}` (regex literals and
  strings alike), keeps the texts in `unicode_props::NAMES` and appends a generated `$$reUnicodeSet(text)` with
  just those sets (encoded like the case runs) to the prelude; other texts are "Invalid property name" (a name
  built at run time too: F109-like). Checked over all code points for 16 properties incl. `\P` and `iu`
  (out/t27/u.js). `v` mode: `$$reClassSetV` (nested classes, `&&`, `--`, `\q{…}` strings; a class with strings is an
  alternation, longest first); under `vi` operands are folded (simple case folding) before the set operations
  and complements (`\P{…}`, `\W` too), where `u` keeps the case-closure approach — both as node does
  (out/t27/v.js). Not done: properties of strings (`\p{RGI_Emoji}` …: Invalid property name), and a V8 quirk:
  under `vi` a one-character `\q{C}` matches only `c` in node (we match `C` too).
- 2026-09-28: `u`-mode `lastIndex` inside a surrogate pair starts at the pair (as V8). Fixture
  `builtins/regex-unicode.js`. The 176 regex-using subset tests: 162 pass, 2 fail (subclassing, F106), 12
  unsupported (BigInt, `eval`, Proxy …). The user approved adding `built-ins/RegExp` to the subset (`SUBSET`,
  `fetch-test262.sh`) and a commit of slices 1–3. Next: the full subset run for the new baseline, then follow-ups.
