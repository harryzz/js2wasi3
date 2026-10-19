# T137: `f.apply(x, arguments)` forwards the argument list (F442)

Status: done (2026-10-19) | Depends on: T128 | Local (+ a-03 for the gate and test262, after asking)

## Goal
Root R4 (calls through values and argument arrays). A function that uses `arguments` gets its arguments as one
array (`all`) and builds an `arguments` object from it in its prologue (`make_arguments`: an empty object, a
`define` per element with a number → string key, `length`, `Symbol.iterator`, about 2 grows). When the only use is
`f.apply(x, arguments)` (raytrace's `Class.create`: `this.initialize.apply(this, arguments)`), `apply` reads it all
back (`list_from_array_like`). Skip the object: pass `all` straight to the call, as V8 does.

## Read first
- `docs/root-causes.md` R4; `docs/design.md` "Speed work".
- `crates/lower/src/closures.rs` (`uses_arguments`), `crates/lower/src/lib.rs` (the `arguments` prologue,
  `MakeArguments`); `crates/backend/src/object.rs` `make_arguments`; `crates/backend/src/func.rs` `apply`,
  `list_from_array_like`.

## Plan
- [x] Count: `make_arguments` calls in the 18 programs, and which `arguments` uses they serve.
- [x] Design and build; fixtures (`apply` with `arguments` in sloppy/strict code, a replaced `Function.prototype.apply`,
  `apply` on a non-function, `arguments` also used another way, arrows).
- [x] Laptop counts and size; gate on a-03 (ask); test262 (ask).

## Notes / Result
- **What changed:** a function whose every `arguments` is the second argument of `x.apply(y, arguments)` in its own
  body (closure analysis `forward_arguments`; not arrows, not `?.`) no longer builds an `arguments` object in its
  prologue. At the call, if `x.apply` is the built-in `Function.prototype.apply` (inline `IsApply`: its `code` slot),
  `ApplyForward` copies the argument array and calls `x` directly; otherwise (an own or replaced `apply`) the
  object is made on first use and passed as before.
- **Result:** a-03 gate vs 3fb66d4: geomean of the 18 programs **0.9665** (minimum times); octane-raytrace
  6,897 → 3,825 ms (−44.5%, instructions −43.75%); knucleotide −2.9% at equal instructions (placement); the rest
  the same (13 byte-identical); micros 97 of 97 same. `.wasm` +0.01…+0.03%. raytrace: `arguments` objects made fell
  from 3,995,642 to 0, object storage enlargements (`grow.call`) from 7,658,038 to 581,456 (−92%).
- **test262** (a-03, `out/t262-t137.log`, 1,862 s with the machine loaded by other users): no changes; three
  timeouts (Array `toLocaleString` invoke-element, RegExp property escapes Mark/Unassigned) pass when rerun alone
  (`out/t262-t137-rerun.log`).
- **Commands:** `js2wasi3 build --count` / `scripts/bench.sh -c` (counts), `scripts/diff.sh -j 4`,
  `scripts/gate.sh -o out/gate-t137 3fb66d4` (a-03), `node scripts/test262.mjs subset` (a-03).
- **Not done (follow-ups):** `Object.prototype.toString` of an `arguments` object gives `[object Object]` (F471);
  `arguments.length`/`arguments[i]` still build the object (earley-boyer 730,042 per run, F472); octane-regexp's
  736,345 `list_from_array_like` calls from `apply` on arrays (F473).
- **Next smallest step:** count where earley-boyer's `arguments` reads go and read `all` directly for
  `.length`/`[i]` when the object never escapes (F472).

## Progress
- 2026-10-19: started (user: "yes" to the `apply`/`arguments` task); tools unchanged (`check-tools.sh`).
- 2026-10-19: count (`build --count`, two new counters `make_arguments`, `list_from_array_like`; `out/count-t137/`):
  raytrace 3,995,642 `arguments` objects, each read back once by `apply` (`Class.create`'s
  `this.initialize.apply(this, arguments)`, its only `arguments`); earley-boyer 730,042, used by `.length`/`[i]`
  (`sc_list` & co., not `apply`: another follow-up); regexp 736,345 `list_from_array_like` with no `arguments`
  (an `apply`/`Reflect` on an array: not this task); the rest ≤ 84.
- 2026-10-19: design: the closure analysis marks functions whose every `arguments` is the second of two arguments
  of `x.apply(…)` in the function itself (`Analysis::forward_arguments`, no arrows); their prologue sets the
  `arguments` local to HOLE instead of `make_arguments`. At such a call: `x`, `x.apply`, `y` in order, then
  `IsApply(x.apply)` (inline: a `$function` whose `code` is `Function.prototype.apply`'s code slot, so an own
  `apply` or a replaced `Function.prototype.apply` takes the old path) → `ApplyForward(x, y, all)` (new
  `func::Helper::ApplyForward`: copies the argument array's `length` elements and calls `call_value`, as `apply`
  does after `list_from_array_like`), else a call of `x.apply` with `y` and the `arguments` object, made on first
  use and kept (one object per call of the function, as before). No watchpoint needed.
- 2026-10-19: fixture `tests/fixtures/closures/apply-arguments.js` (Class.create, callee's own `arguments`,
  strict/sloppy `this`, own and replaced `apply` (identity kept), not a function, `Math.max.apply`, a class, mixed
  uses, arrows, evaluation order) passes; `closures` 12/12 and `classes` 13/13 pass. Found: `Object.prototype.toString`
  of an `arguments` object gives `[object Object]` (node `[object Arguments]`), before this task too (follow-up).
- 2026-10-19: counts after (`out/count-t137b/`), raytrace: `arguments` objects made fell from 3,995,642 to 0,
  `list_from_array_like` calls from 3,995,642 to 0, object storage enlargements (`grow.call`) from 7,658,038 to
  581,456 (−92%; ~1.77 per `arguments` object); outputs as node's; the other programs unchanged.
- 2026-10-19: all fixtures (`scripts/diff.sh -j 4`): 275 passed, 2 failed (`commonjs/ws`, `node/fetch`: timeouts
  under `-j 4`, both pass alone), 19 skipped. Next: the gate on a-03 (ask).
- 2026-10-19: user: "yes" to the a-03 gate. `scripts/gate.sh -o out/gate-t137 3fb66d4` started on a-03 (`~/gate-t137.log`).
- 2026-10-19: a-03 gate vs 3fb66d4 (`out/gate-t137-a03.log`, `out/gate-t137/`): geomean **0.9665** (minimum
  times); octane-raytrace 6,897 → 3,825 ms (−44.5%, instructions −43.75%); knucleotide −2.9% at −0.00% instructions
  (placement); regexredux, revcomp, earley-boyer same; the other 13 byte-identical; micros 97 of 97 same. `.wasm`
  +0.01…+0.03% (raytrace +154 bytes). Next: full test262 on a-03 (ask).
- 2026-10-19: test262 on a-03 (`out/t262-t137.log`, 1,862 s; load 22–34 from other users): 3 pass → timeout
  (Array `toLocaleString` invoke-element, RegExp property escapes Mark/Unassigned); rerun alone all 3 pass
  (`out/t262-t137-rerun.log`): no changes. Task done.
