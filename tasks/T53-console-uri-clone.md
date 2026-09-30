# T53: `console` as a value and spread, URI functions, `structuredClone`

Status: done (2026-09-30) | Depends on: T10 (console output), T21/T25 (prelude,
`console.log` formatting, `builtins/console.js`), T42 (global object)

## Goal
Small, common gaps that stop real programs from building, as in node 24.21.0:
1. `console.log(...xs)` (spread arguments) and `console` as a value: `const log = console.log; log(x)`,
   `arr.forEach(console.log)`, `typeof console`, `console.log.bind(console)`, `globalThis.console`. Methods: `log`,
   `info`, `debug` (stdout), `error`, `warn` (stderr), the ones implemented today.
2. `encodeURIComponent`, `decodeURIComponent`, `encodeURI`, `decodeURI` (UTF-8, node's `URIError: URI malformed`),
   and Annex B `escape`/`unescape`.
3. `structuredClone(v)`: deep copy with cycles and shared references kept, for plain objects, arrays, `Map`/`Set`,
   `Date`, `RegExp`, `ArrayBuffer`/typed arrays, errors, primitives and wrapper objects; node's `DataCloneError` for
   functions and symbols.
Programs that don't use these compile to the same bytes.

## What we know (at handoff)
- **`console`**: `console.log(…)` is special-cased in the lowering (`Lowerer::console_method`, `Lowerer::console`:
  each argument formatted with `Intrinsic::Inspect`, `$$consoleFormat` for `%` format strings, the line written with
  `Intrinsic::ConsoleWrite { stderr }`). A spread argument is "unsupported: spread argument"; `console` anywhere else is
  "unsupported: global `console`". A library can't write to stdout ("`console` in a module with exports"): keep that.
- **URI functions**: "unsupported: global `encodeURIComponent`"; they're in the lowering's list of node's globals
  (`crates/lower/src/lib.rs`, next to `structuredClone`). `built-ins/encodeURI*`/`decodeURI*` aren't in the test262
  subset or the sparse clone.
- **`structuredClone`**: "unsupported: global `structuredClone`"; node throws `DOMException [DataCloneError]: … could
  not be cloned.` (no `DOMException` here: decide how close to get; check what `console.log` of it shows).

## Design questions (decide first, record in docs/design.md D5)
1. `console` as a value: an object (made on first use, like `Math`'s namespace value) whose methods are prelude
   functions `$$Console_log(...args)` & co. that format through the same path (`__intrinsic_inspect`, `$$consoleFormat`)
   and write with a new intrinsic (`__intrinsic_console_write(stderr, text)`)? And `console.log(...xs)`: the same
   function called with a spread (`CallList`), keeping the direct path for ordinary calls (same bytes).
2. URI functions and `escape`/`unescape`: prelude code (`builtins/uri.js`), linked by name like other globals.
3. `structuredClone`: prelude code (a memo `Map` for cycles) over existing intrinsics (`date_data`, `regexp_data`,
   typed arrays, `__intrinsic_define`); the error for functions/symbols: an `Error` named `DataCloneError` with node's
   message, printed as close to node as the inspector allows.
4. test262: add `built-ins/encodeURI`, `encodeURIComponent`, `decodeURI`, `decodeURIComponent` to the subset (sparse
   clone + `SUBSET`)? Check their size and run time first (some are large loops over all code points).

## Plan
1. `console` value + spread; fixture `tests/fixtures/console-value.js` (or under `builtins/`); node is the oracle.
2. URI functions, `escape`/`unescape`; fixture; test262 decision (Q4).
3. `structuredClone`; fixture.
4. Gating check (scratchpad `cmp.sh` against a build of HEAD), test262 targeted run on a-03 (tell the user the
   expected time), docs (design.md D5, keywords.md, README, HISTORY.md), follow-ups.

## Read first
- `crates/lower/src/lib.rs`: `console_method`, `console`, the list of node's globals; how `Math` as a value is made
  (`Intrinsic::Namespace`, `ir::Module::namespace_methods`).
- `crates/lower/builtins/console.js`; `crates/backend/src/host.rs` (`ConsoleWrite`).
- `docs/design.md` D5 (prelude, T25 slice 5: console format strings).

## Tests
Fixtures via `scripts/diff.sh`, `cargo test`, `wasm-tools validate` + wasmtime. Gating: old vs new build of every
fixture byte-identical except those using the new features. test262: targeted `--update` on a-03.

## Acceptance
- [x] `console` as a value and `console.log(...xs)` behave like node (fixture).
- [x] URI functions, `escape`/`unescape`, `structuredClone` behave like node (fixtures); their test262 tests pass or
      have named follow-ups if added to the subset.
- [x] Programs not using them compile to the same bytes; 0 test262 regressions.

## Notes / Result
**Design** (recorded in docs/design.md D5, "T53"):
- `console` as a value is `$$Console()` (`builtins/console.js`), an object made on first use with prelude methods
  `$$Console_log`/`info`/`debug`/`warn`/`error` (rest parameters, the line from `$$consoleFormat`, a lone string as
  it is, new prelude intrinsics `__intrinsic_write_stdout/stderr(text)`). Not an `ir::Namespace`: its global would
  shift every program's global indices. Linked only when `console` appears other than as `console.<method>(…)`
  without spread (`builtins::console_direct`); `console.log(...xs)` calls the object's method. A program that
  writes a property of `console` (`Prelude::console_patched`) calls every method through the object. node's other
  members (`console.table`) are build errors. On the global object when the code names `console`.
- `builtins/uri.js`: the spec's Encode/Decode over UTF-8 (`URIError: URI malformed`), Annex B `escape`/`unescape`;
  linked by name (`$$encodeURI` & co.), added to `GLOBAL_NAMES` in node's order.
- `builtins/clone.js`: `structuredClone` in one pass with a native identity map (cycles, shared references, shared
  buffers): plain objects, arrays (holes, extra keys), `Date`, `RegExp`, `Map`/`Set`, wrappers, errors (`name` picks
  the constructor; own `message`, `cause`), `ArrayBuffer`s, typed arrays. `DataCloneError`: an `Error` whose
  prototype is a small `DOMException.prototype` (`code` 25, own `name`), so it prints and is reported uncaught as
  node's `DOMException [DataCloneError]: … could not be cloned.`; naming `structuredClone` keeps function source
  text for that message. node's `ERR_MISSING_ARGS`/options errors.
- Fixed on the way: the backend's `builtin_tag` had Map/Set iterator tags swapped (`select` operands).

**Tests**: fixtures `tests/fixtures/builtins/console-value.js`, `console-patched.js`, `uri.js`,
`structured-clone.js` (+ `.uncaught`) pass `scripts/diff.sh`; `cargo test` passes (`unsupported.rs`: `console.table`
now says "`console.table`"); `wasm-tools validate` + wasmtime on the new programs. Gating (scratchpad `cmp.sh`, HEAD
vs new, every fixture): 183 identical, 16 differ, all from the iterator-tag fix (identical with it reverted), 4
new. test262 full run on a-03 with `--update`: the 208 new tests 196 pass, 10 fail (F228), 2 timeout (F229); 2
unsupported → pass; 0 regressions; subset 27,824 of 28,799 run pass (96.6%).

**Not supported** (follow-ups): F228 (tests using a script's `this`), F229 (`String.fromCharCode` and string
building speed), F230 (`console` leftovers: other methods, printing `console`, writes through an alias), F231
(`structuredClone` leftovers: `transfer`, `DOMException` global, `stack`, `arguments`).

**Next smallest step**: CommonJS (F194), as the handoff suggested, then node's built-in modules (F200).

## Checklist
- [x] design questions decided and recorded
- [x] `console` value and spread
- [x] URI functions, `escape`/`unescape`
- [x] `structuredClone`
- [x] fixtures, gating check, test262 + baseline
- [x] docs (HISTORY.md, keywords.md, design.md, README), follow-ups

## Progress
- 2026-09-30: handoff written after T52. State: `main` at `1143829`, pushed; test262 subset 27,626 of 28,591 run
  pass (96.6%).
- 2026-09-30: started. Q1 decided: not an `ir::Namespace` (a 4th namespace global would shift every program's
  global indices, so every program's bytes). `console` as a value is `$$Console()` in `builtins/console.js` (an
  object made on first use, like `$$GlobalObject`), methods `$$Console_log`/`warn`/`debug`/`info`/`error` (rest
  parameter, `$$consoleFormat` for the line, new prelude intrinsics `__intrinsic_write_stdout/stderr(text)`).
  Linked only when `console` appears other than as `console.<method>(…)` without spread; `console.log(...xs)` goes
  through the value (the generic spread call). The direct path stays for ordinary calls (same bytes).
- 2026-09-30: part 1 done: `tests/fixtures/builtins/console-value.js` passes (spread, aliases, `forEach(console.log)`,
  `bind`/`call`/`apply`, `typeof`, `name`/`length`, format strings through the value); `globalThis.console` works
  ("console" added to `GLOBAL_NAMES`); a library naming `console` as a value gets the old "unsupported" error.
- 2026-09-30: part 2: `builtins/uri.js` (Encode/Decode over UTF-8, V8's `URIError: URI malformed`; Annex B
  `escape`/`unescape`), linked by name as `$$encodeURI` & co.; the six names added to `GLOBAL_NAMES` (node's order).
  Fixture `tests/fixtures/builtins/uri.js` passes. Q4 decided: the four `built-ins/*URI*` dirs and
  `annexB/built-ins/escape`, `unescape` (208 tests) added to the subset and the sparse clone. Locally 198 pass; the
  10 failures read a script's top-level `this` as the global object (a module's is `undefined`). The slowest
  (`decodeURI*/…A2.5_T1`, ~1M calls) take ~9 s at full opt, ~22 s in the runner: ~4 s is
  `String.fromCharCode(H, L)` and ~4 s the harness's string concatenation, not decoding (follow-up).
- 2026-09-30: part 3: `builtins/clone.js` (`$$structuredClone`, one pass with a native identity map as memo;
  `DataCloneError` is an `Error` re-prototyped to a `DOMException.prototype` (constructor `$$DOMException`, `code`
  25, own non-enumerable `name`), which prints and reduces to node's `Uncaught DOMException [DataCloneError]: …`).
  Functions keep their source when the code names `structuredClone` (the message shows it). `stack` isn't
  cloned (a not-implemented sentinel here; an accessor in node 24). Fixed on the way: the backend's `builtin_tag`
  had Map/Set iterator tags swapped (`select` operands; visible as `Object.prototype.toString.call(map.keys())`).
  Fixture `tests/fixtures/builtins/structured-clone.js` (+ `.uncaught`) passes.
- 2026-09-30: `console.table` & co. (node's other members) are build errors "`console.table`" (was "global
  `console`"; `unsupported.rs` updated). `cargo test` passes. Gating (scratchpad `cmp.sh`, HEAD vs new release
  build, every fixture): 183 same, 16 differ, 3 new; with the iterator-tag fix reverted all 199 are identical, so the
  16 are only that fix (programs with maps).
- 2026-09-30: full test262 subset run started on a-03 (with the code as of the gating check). Meanwhile:
  `console`'s keys in node's order (log, info, debug, warn, error); a program that writes a property of `console`
  (`console.log = f`, `delete console.x`, `Object.assign/defineProperty(console, …)`; not through an alias) has
  every `console.x(…)` go through the object (`Prelude::console_patched`), and may read its own members. Fixture
  `tests/fixtures/builtins/console-patched.js` passes.
- 2026-09-30: docs updated (design.md D5 note, keywords.md rows, README), follow-ups F228–F231 added, F202 updated.
  Gating rerun on the final code: same 183 / 16 (the iterator-tag fix) / 4 new; `cargo test` passes. Waiting for
  the a-03 run.
- 2026-09-30: a-03 full run done (1,937 s): 0 regressions; 1 load timeout (`S13.2.1_A1_T1`) passed alone and was set
  back in the baseline. Subset 27,824 of 28,799 run pass (96.6%). Task done.
