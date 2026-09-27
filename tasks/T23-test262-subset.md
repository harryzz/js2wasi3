# T23: test262 subset runner

Status: todo, run only with approval | Depends on: T12+

## Goal
Run a chosen subset of test262 (by directory or feature list) through js2wasi3 + wasmtime and report the pass rate per directory.
Store the results so a later run only shows the tests whose result changed.

## Rules
Never run it after every edit. Only at the end of a task or on request. Ask before any run expected to take more than 10 minutes.

## Notes / Result

## Handoff (written 2026-09-28, end of the T20 session)

**State:** T20 done and pushed (`78eeca2`). The user agreed T23 is next; start with slice 1 (the runner). Set the
status to `in progress` in `tasks/README.md` and log each step in `## Progress`. Commit/push/post only when the user
asks; no attribution lines anywhere (commits, issues, comments).

**Readiness check (2026-09-28, harness from test262 `main`, fetched to a scratch dir):**
- `sta.js` + `assert.js` (+ `compareArray.js`) followed by `assert.sameValue(1 + 1, 2);` compile and run as a command.
- `doneprintHandle.js` (async tests): needs a global `print`. Prepend a shim in the runner
  (`function print(x) { console.log(x); }`), no compiler change.
- `propertyHelper.js` fails to build: built-ins as values (`Array.isArray`, `Object.defineProperty`,
  `Object.getOwnPropertyDescriptor`, `Object.getOwnPropertyNames`, `Function.prototype.call.bind(Array.prototype.join)`
  and `.push`, `Object.prototype.hasOwnProperty`/`propertyIsEnumerable`). Many `built-ins/` tests include it
  (`verifyProperty` for `length`/`name`): they'll count as "unsupported" until a later task fixes these (follow-up).

**Suggested slices (agree with the user first):**
1. **Runner** (e.g. `scripts/test262.sh` + a small driver, node is available for scripting): test262 at a pinned
   commit in `TOOLS.md`, cloned into a gitignored dir (~1 GB; ask before cloning). Per test: parse the YAML frontmatter
   (`includes`, `flags`, `negative`, `features`); prepend `sta.js`, `assert.js`, `includes`, the `print` shim
   (`doneprintHandle.js` for `flags: [async]`); build with js2wasi3, run `wasmtime run -Sp3` with a timeout.
   Results: pass / fail / unsupported (build rejected with "unsupported") / timeout / skip. `negative` tests:
   phase parse/early expect a build error (a real SyntaxError, not "unsupported"); phase runtime expect the
   uncaught error type on stderr. Async: pass on `Test262:AsyncTestComplete`. Parallel (`-j`), counts per
   directory, a stored baseline (JSON) and a mode that lists only tests whose result changed.
2. **First subset:** `language/expressions`, `language/statements`, `built-ins/{Array,String,Math,JSON,Promise,Map,Set}`;
   time one small directory first; ask before any run expected over 10 minutes (T23's rule).
3. **Record** the top failure causes as follow-ups (propertyHelper's built-ins as values, `Date`, `RegExp`,
   global-object behaviour); don't fix them in T23.

**Expected noise:** js2wasi3 compiles ES modules only (always strict). Skip `noStrict` (and `raw` tests that need
sloppy mode); run the rest as strict modules. Tests relying on script semantics (top-level `this`, top-level `var`
as a global-object property, `eval`, `Function(...)`) will fail for that reason: give them their own category if
cheap. Each test is a full build + wasmtime compile (~0.3–1 s), so whole-suite runs aren't practical.
