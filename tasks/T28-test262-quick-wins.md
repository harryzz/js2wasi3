# T28: test262 quick wins

Status: done | Depends on: T23

## Goal
Fix the cheapest test262 causes found by T23 slice 3, in this order:
1. F139: a read of an undeclared name throws `ReferenceError: x is not defined` at run time (node's message);
   `typeof x` is `"undefined"`. Names that are real globals in node (`Reflect`, `WeakMap`, …) stay "unsupported".
2. F140: `await using` → the "unsupported" diagnostic, not a panic.
3. F143: WIT package/world names from file names with `__` (empty words).
4. F144: what test262's `propertyHelper.js` needs: `Array.isArray`, `Object.defineProperty`/`getOwnPropertyDescriptor`/
   `getOwnPropertyNames` as values, `Array.prototype.join`/`push`, `Object.prototype.hasOwnProperty`/
   `propertyIsEnumerable` as values, `Function.prototype.call.bind(f)`.

## Read first
- `tasks/T23-test262-subset.md` (slice 3 table), `docs/design.md` "Testing" (test262 runner).
- `crates/lower/src/lib.rs`: `expr` (Identifier), `resolve`, `tdz_check`; `crates/lower/src/builtins.rs`.

## Tests
Unit/snapshot tests for each change, fixtures in `tests/fixtures` checked with `scripts/diff.sh`, then the test262
runner on the affected directories only (compare with the baseline; `--update` when better).

## Checklist
- [x] F139 undeclared reads throw
- [x] F140 `await using`
- [x] F143 WIT names
- [x] F144 propertyHelper

## Notes / Result
**What changed**
- F139 (`crates/lower/src/lib.rs`): `Lowerer::undeclared(id)`: no symbol, not `arguments`, not an intrinsic, not in
  `NODE_GLOBALS` (node 24's globals in an ES module, sorted; unit test). A read throws
  `ReferenceError: x is not defined` (`not_defined`); `typeof x` is `"undefined"`, also in the `typeof x === "tag"`
  fast path (`typeof_compare` falls back to the general comparison). node's globals js2wasi3 lacks (`Reflect`, …)
  stay "unsupported". Writes (`x = 1`) are still rejected (F154).
- F140: `global_inits` returns early for `using`/`await using` (already reported; no globals were declared).
- F143 (`crates/component/src/lib.rs`): `WorldName::from_path` maps every non-alphanumeric character to `_` first.
- F144: `Object.getOwnPropertyNames` (`Intrinsic::GetOwnPropertyNames` → `object::Helper::Keys(3)` over the new
  `OwnNames` = `own_keys(all)`: non-enumerable keys, an array's `length`, a string's indices + `length`). Statics
  lowered to intrinsics as values: `$$Object_<static>$value` / `$$Array_isArray$value` wrappers in
  `builtins/object.js` (suffix `builtins::VALUE`), rooted only for non-call reads (programs that only call them
  compile as before), `Lowerer::builtin_static_value`, named without the suffix, node's lengths, and installed as
  statics of constructor values (`verifyProperty(Array, "isArray")`).

**Tests**: fixtures `exceptions/undeclared.js`, `exceptions/undeclared-uncaught.js`, `objects/own-property-names.js`,
`builtins/statics-as-values.js` (diff.sh: node = js2wasi3); `unsupported.rs` (`Reflect` stays unsupported; `x`
lowers; `await using` is reported); `world_names`; `node_globals_are_sorted`. `cargo test` ok; `diff.sh`
builtins/objects/exceptions: 45 pass.

**test262** (user-approved targeted rerun of the 4,816 non-passing tests with a fixed cause, 1,795 s at -j 4, then the
16 "is not defined" failures after the `typeof_compare` fix): 3,136 unsupported/fail → pass, 9 crash → unsupported,
0 regressions. Baseline: 19,040 pass, 1,760 fail, 3,999 unsupported, 0 crash, 23 timeout, 1,808 skip:
**76.7%** of 24,822 run (was 64.1%). See `tests/test262/HISTORY.md`.

**Left** (follow-ups): writes to undeclared names, 46 tests (F154); `A.hasOwnProperty(…)` on a built-in constructor
that isn't otherwise a value, 6 tests (F155); `verifyProperty(this, "Array")` needs a global object (34 tests,
F151). Unmasked by the fixes: private accessors now 351, computed instance keys 229 (F58).

**Next smallest step**: by test count: `yield*` in async generators (F115, 1,128), wrapper objects (F106, ~800),
classes (F58, ~580), `defineProperty` on arrays (F145, 202). Needs a task and approval.

## Progress
- 2026-09-27: task created (user approved "test262 quick wins"). F139 first: test262's undeclared names are almost all reads (`x + 1`, destructuring tests reading `w`/`u`/`s`, `...unresolvableReference`), often in `assert.throws(ReferenceError, …)`.
- 2026-09-27: F139 done in `lower`: `Lowerer::undeclared` (no symbol, not `arguments`, not in `NODE_GLOBALS` = node 24's ESM globals, sorted) → a read is `throw ReferenceError("x is not defined")`; `typeof x` → `"undefined"`. node globals js2wasi3 lacks stay "unsupported". Fixtures `exceptions/undeclared.js`, `exceptions/undeclared-uncaught.js` pass `diff.sh`; `unsupported.rs` now checks `Reflect` for the rejection; `cargo test -p js2wasi3-lower` ok; validate ok. Writes to undeclared names (`x = 1`) are still rejected.
- 2026-09-27: F140 done: `global_inits` returns early for `using`/`await using` (already reported by `declare_command_globals`, no globals declared → the panic). Test in `unsupported.rs` (`command_rules`).
- 2026-09-27: F143 done: `WorldName::from_path` maps every non-alphanumeric character of the file stem to `_` before `names::to_wit` (a `-` stayed inside a word: `obj-prop--proto-dup`). Tests in `world_names`.
- 2026-09-27: F144 part 1: `Object.getOwnPropertyNames`: `Intrinsic::GetOwnPropertyNames` → `object::Helper::Keys(3)` over a new `OwnNames` helper (`own_keys` with `all`: non-enumerable keys, an array's `length` after its indices; a string's indices then `length`). Fixture `objects/own-property-names.js` passes `diff.sh`.
- 2026-09-27: F144 part 2: statics lowered to intrinsics as values: prelude wrappers `$$Object_<static>$value` / `$$Array_isArray$value` (`builtins/object.js`, suffix `builtins::VALUE`), rooted only when `A.b` is read and not called (calls stay intrinsics: IR of existing programs unchanged); `Lowerer::builtin_static_value`; `func_value` names them without the suffix; lengths in `builtins::length`; installed as statics on constructor values (`verifyProperty(Array, "isArray")`). Fixture `builtins/statics-as-values.js`. `cargo test` ok; `diff.sh` builtins/objects/exceptions: 45 pass. test262 spot check (Math/abs, Array/isArray, Map/prototype/get, JSON/stringify/length): 9 unsupported → pass, 0 regressions.
- 2026-09-27: user approved a targeted test262 rerun of the 4,816 affected tests (started in the background, --update --causes 25).
- 2026-09-27: targeted rerun done (3,120 unsupported → pass, 94 → fail). 16 new fails were `typeof x === "undefined"` through `typeof_compare` (read the name): fixed, rerun, all pass. Baseline 76.7%. T28 done.
