# T33: holes look up the prototype chain in the native array paths (F159)

Status: done | Depends on: T32

## Goal
A hole in an array is a missing own property, so reading it is `Get`: the prototype chain (`Array.prototype[1]`,
getters with the array as `this`). The prelude's generic methods already do this; the native paths don't:
`join`/`toString`, `pop`, and the iterator's array fast path (spread, `for-of`, destructuring, `Array.from`) read a
hole as `undefined`. `console.log` is right to show `<1 empty item>` (node checks own properties).

## Plan
1. In `join`, `pop` and the iterator's array step: a hole (Wasm null) reads through `object::Helper::Get` with the
   index key (the fast path finds no own element and goes on to the prototype chain; accessors get the array as
   the receiver). Only holes pay for it.
2. Fixture `tests/fixtures/arrays/holes-prototype.js` (outputs collected while `Array.prototype` has index
   properties, printed after they're removed: node's own `console.log` fails with a getter there).
3. test262: `built-ins/Array/prototype/{pop,join,toString,unshift,shift}`, `language/statements/for-of`,
   `language/expressions/array` with `--update`.

## Read first
- F159 in `tasks/README.md`; T32's Notes / Result.
- `crates/backend/src/array.rs` (`join`, `proto_pop`), `crates/backend/src/iter.rs` (`step`),
  `crates/backend/src/object.rs` (`get`).

## Tests
The fixture with `scripts/diff.sh` plus `tests/fixtures/arrays`, `cargo test`, `wasm-tools validate` + one wasmtime run.

## Checklist
- [x] Holes through `Get` in `join`, `pop`, the iterator
- [x] Fixture
- [x] test262 rerun, follow-ups, baseline

## Notes / Result
**What changed**: `array.rs` `join` and `Array.prototype.pop`, and `iter.rs`'s array step read a hole with
`object::Helper::Get` (index key as an `i31`): the prototype chain, accessors with the array as receiver. Only holes
take that path.

**Tests**: `tests/fixtures/arrays/holes-prototype.js` (= node), `tests/fixtures/arrays` and the T32 array fixtures,
`cargo test -p js2wasi3-backend`, `wasm-tools validate`, wasmtime run.

**test262**: `built-ins/Array/prototype/{pop,join,toString,shift,unshift}`, `built-ins/Array/from`,
`language/statements/{for-of,for}`, `language/expressions/array` (1,334 tests, 418 s at -j 4, `--update`): 3 fail →
pass, no regressions; `language/expressions/assignment/dstr`, `language/statements/const/dstr` (461): unchanged.
Subset: 21,719 of 24,820 run pass (87.5%).

**Left**: writes don't consult the prototype chain either: `a[i] = v` / `push` on a hole or past the end define an
own element even when `Array.prototype` has a setter or read-only property at that index (F162; test262
`unshift/set-length-*`).

**Next smallest step**: F162 needs a cheap guard (e.g. a flag set when an index key is defined on any object that
could be a prototype) so `push` stays fast.

## Progress
- 2026-09-27: task written for F159 at the user's request and started (node checked: `join`, spread, `for-of`,
  destructuring, `pop` see `Array.prototype[i]` through holes; its `console.log` throws with an index getter there).
- 2026-09-27: holes through `object::Helper::Get` (index key) in `join`, `pop`, the iterator's array step; fixture `arrays/holes-prototype.js` passes (its log is a string: node's [[Set]] on an array index meets prototype setters, ours doesn't: new follow-up).
- 2026-09-27: test262 +3, no regressions. Task done.
