# T27: `RegExp` and regex literals

Status: todo (later) | Depends on: T21 (prelude), T25 slice 5 (well-known symbols)

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

## Acceptance
- [ ] Fixtures in `tests/fixtures/builtins/regex-*.js` match node (`scripts/diff.sh`), including the string methods.
- [ ] Programs without regexes link none of the engine.
- [ ] F78 (regex literals) and the regex parts of F87 ticked.

## Notes / Result

## Progress
