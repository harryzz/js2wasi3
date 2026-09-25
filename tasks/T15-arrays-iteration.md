# T15: Arrays and iteration

Status: done | Depends on: T14

## Goal
Arrays (a dense GC backing store with holes marked, and length semantics), literals, index get/set, `push/pop/length`,
the iterator protocol, `for...of`, spread, and array/object destructuring.

## Acceptance
- [x] Fixtures `arrays/*.js` match node.

## Notes / Result
**Done 2026-09-25.** Arrays, symbols, the iterator protocol, `for-of`, spread and destructuring.

What changed:
- IR: `Expr::Array`, intrinsics `ArrayPush/ArraySpread/ArgsRest/IterOpen/IterStep/IterValue/IterClose/IterRest/
  CallList/ObjectRest/NewSymbol/SymbolIterator/IsArray/ObjectKeys/ObjectValues/ObjectEntries` (`CheckDestructure`
  exists but the lowering emits node's messages directly), `Builtin::{Array, Symbol, ArrayIterator}`.
- Backend: rec group of 16 types (`$string` non-final + `$symbol`; `$array` ⊂ `$object`; `$iter`);
  `crates/backend/src/array.rs` (dense store, `length` semantics, `push/pop/join/toString`), `iter.rs` (iteration
  state, closing, spread, `values()`/`[Symbol.iterator]()` iterator objects and `%ArrayIteratorPrototype%.next`),
  `inspect.rs` (`console.log` of arrays and plain objects in node's layout); object helpers use a generic `OwnGet`
  and know arrays; symbols in every conversion; `Function.prototype.apply`; `Object.keys/values/entries`.
- Lowering: `crates/lower/src/patterns.rs` (one walker for binding and assignment patterns, array literals, spread
  arguments, `for-of` via the `finally` machinery); parameters with defaults/patterns/rest; top-level command
  pattern declarations become globals; `console.log` evaluates all arguments before formatting.
- Tests: `tests/fixtures/arrays/{basics,iteration,destructuring,symbols}.js`; `arrays_and_iteration` in
  `crates/lower/tests/unsupported.rs` (and T15 rejections there turned into `lowers`); the inspect trap test in
  `build.rs` now uses `Object.create(null)`.

Commands run: `cargo test` (59 passed), `scripts/diff.sh` (32 passed, 0 failed, 9 skipped), `wasm-tools validate`
+ `wasmtime run -Sp3` on `arrays/destructuring.js`, `cargo clippy --all-targets` (clean).

Unsupported / deferred (Follow-ups F49–F56, and F24/F38/F39/F42/F43 updated in `tasks/README.md`): array methods
other than `push/pop/join/toString/values/[Symbol.iterator]` (sentinels, T21); `Array(...)`/`Array.from`;
`arguments` (T21); `bind`; indices ≥ 2^26 trap (no sparse arrays); arrays/strings iterate without looking at
`Symbol.iterator`; `console.log` traps for class instances, null-prototype objects, > 100 elements, extra array
properties, symbol keys and strings needing escapes; a hole after a spread element in a literal; exported
functions with rest parameters.

Next smallest step: T16 (classes); array methods come with T21's self-hosted built-ins.
- 2026-09-25: real `Array.prototype.values`/`[Symbol.iterator]`, `String.prototype[Symbol.iterator]`,
  `%ArrayIteratorPrototype%` (`next`, `[Symbol.iterator]`), iterator state under a private symbol (`G_SYM_STATE`);
  delegating iterables pass (added to `arrays/iteration.js`). Docs (design.md D1 note, lower/backend module docs),
  Notes / Result, README status + follow-ups F49–F56 (F38 done; F24/F39/F42/F43 updated). Task done.
- 2026-09-25: user approved the commit ("T15: arrays and iteration"), no attribution lines, not pushed. Next: T16.
