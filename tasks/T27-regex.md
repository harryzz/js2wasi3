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
- [ ] Slice 1: engine, literals, `RegExp`/`exec`/`test`/flags/accessors, printing
- [ ] Slice 2: string methods through `Symbol.*`
- [ ] Slice 3: lookaround, backreferences, `u`/`v`, `\p{…}`, `d`
- [ ] Slice 4: test262 (subset change asked first), follow-ups, baseline, docs (`docs/keywords.md`, README)

## Progress
- 2026-09-28: handoff written after T38 (the user chose RegExp next; engine decision above agreed). Not started.
  State at handoff: `main` = origin at `040bfdc`; test262 subset 22,211 of 24,973 run pass (88.9%); whole
  `diff.sh` 120 pass.
