# T143: `arguments.length` and `arguments[i]` read the argument array (F472)

Status: done (2026-10-21) | Depends on: T137 | Local (+ a-03 for the gate and test262, after asking)

## Goal
earley-boyer builds 730,042 `arguments` objects per run (T137's count); `sc_plus`, `sc_list`, `sc_append` and the other
Scheme helpers use `arguments` only as `arguments.length` and `arguments[i]`. A function that gets its arguments as one
array (`all`) builds the object in its prologue (`make_arguments`: ~770 ns each on raytrace, T137). Skip it when every
use of `arguments` is such a read, as T137 did for `f.apply(x, arguments)`: the local starts as HOLE and is built on
the first other use.

## Read first
- `tasks/T137-apply-arguments.md` (Notes / Result), `crates/lower/src/closures.rs` (`forward_arguments`,
  `is_apply_argument`), `crates/lower/src/lib.rs` (`forward_all`, `arguments_object`, `apply_forward`).

## Plan
- [x] Count: which earley-boyer functions build the objects and with which uses (`make_arguments` is 730,042).
- [x] Design and build; fixtures (`arguments.length`, `arguments[i]` with numbers/strings/out of range, mixed with other
  uses, arrows, sloppy/strict, a modified parameter).
- [x] Laptop counts and size; gate on a-03 (ask); test262 (ask).

## Notes / Result
- **What changed:** a function whose every use of `arguments` is `x.apply(y, arguments)` (T137), `arguments.length` or
  `arguments[k]` read as a value, or a `var`/`let`/`const a = arguments` whose variable is only read that way after
  its declaration in the same function, makes no `arguments` object up front (the local starts HOLE, built on the first
  other use). `arguments.length` is the argument array's length (`ArgsLen`, an `i32`); `arguments[k]` with `k` an integer
  in `0..length` (`IsArgIndexF` for an `f64` key, `IsArgIndexJs` for an `i31`) reads the array through the inline
  element path, any other key reads the lazily made object, so results are as before. The alias initializer makes
  nothing. Closure analysis: `is_arguments_read`, `arguments_alias` (`closures.rs`).
- **Result:** `make_arguments` calls: earley-boyer 730,042 → 0 (all 18 programs now 0). a-03 gate vs 5f8688a: geomean of the
  18 programs **0.9957** (minimum times); octane-earley-boyer 4,713 → 4,399 ms (**−6.7%**, instructions −7.27%); 14 programs
  byte-identical; `.wasm` earley-boyer +0.36% (+4,370 bytes), others 0.00%. test262 on a-03 (`out/t262-t143.log`, 1,533 s):
  **no changes**, 28,123 of 28,984 run pass (97.0%).
- **Commands:** `scripts/bench.sh -c` (counts), `scripts/diff.sh -j 4`, `scripts/gate.sh -o out/gate-t143 5f8688a` (a-03),
  `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** `Object.prototype.toString.call(arguments)` is still `[object Object]` (F471);
  `arguments` passed whole to other functions (`new sc_Values(arguments)`, `"".concat.apply("", arguments)` is
  T137's form); aliases read from closures; octane-regexp's `list_from_array_like` (F473).

## Progress
- 2026-10-21: started (user: "ok lets first try with earley-boyer's arguments reads"). Tools unchanged.
- 2026-10-21: count before (T137): earley-boyer `make_arguments` 730,042; every use is `arguments.length` / `arguments[i]`
  (Scheme helpers `sc_plus`, `sc_append`, ...) except `sc_list`'s `var a = arguments; a.length; a[i]` (the hot one).
- 2026-10-21: design. Analysis (`closures.rs`): `is_arguments_read` (the object of `arguments.length` or `arguments[k]` used as
  a value: parent a binary/logical/conditional/declarator/return/... or an assignment's right side or a call argument,
  never an assignment target, update, `delete`, callee or tagged template) joins `is_apply_argument` in
  `forward_arguments` (T137: the local starts HOLE, the object is made on the first other use); `arguments_alias`: `var a =
  arguments` whose variable is only read that way after the declaration, in its own function, never reassigned/redeclared
  (`arguments_aliases`, `alias_inits`). Lowering: `arguments.length` = `ArgsLen(all)` (an `i32` as `f64`); `arguments[k]` = `k` in
  `0..length` (`IsArgIndexF` for an `f64` key, `IsArgIndexJs` for an `i31`) → `all[k]` (the inline element path), else the
  lazily made object's `get`; the alias initializer makes nothing (an `undefined` placeholder).
- 2026-10-21: built. `make_arguments` calls (`out/count-t143/`): earley-boyer 730,042 → **0** (output = node);
  raytrace and crypto still 0. Fixture `tests/fixtures/closures/arguments-reads.js` (keys of every kind, mixed uses,
  assignments, arrows, parameters changed after the call, rest/defaults, `arguments[0]()`, `in`, generators, async,
  classes, and the alias forms incl. reassigned/closure/early/twice/assigned/apply) = node; `closures` 13/13.
- 2026-10-21: all fixtures (`scripts/diff.sh -j 4`): 279 passed, 1 failed (node/fetch: timeout under load, passes alone), 19 skipped. `make_arguments` is 0 in all 18 programs (`out/count-t143all/`), outputs = node. Next: the a-03 gate (ask).
- 2026-10-21: user: yes. Synced; `scripts/gate.sh -o out/gate-t143 5f8688a` started on a-03 (`~/gate-t143.log`).
- 2026-10-21: a-03 gate vs 5f8688a (`out/gate-t143-a03.log`, `out/gate-t143/gate.tsv`): geomean **0.9957** (minimum times);
  octane-earley-boyer 4,713 → 4,399 ms (**−6.7%**, instructions −7.27%); 14 programs byte-identical, knucleotide/regexredux/
  revcomp differ by 15 bytes (smaller) and are the same in time; outputs = node. `.wasm` earley-boyer **+0.36%** (+4,370 bytes),
  the rest 0.00%. Micros: no verdicts. Next: full test262 on a-03 (ask).
- 2026-10-21: user: ok to test262. `node scripts/test262.mjs subset` started on a-03 (`~/t262-t143.log`).
- 2026-10-21: test262 on a-03 (`out/t262-t143.log`, 1,533 s): no changes (28,123). Task done.
