# T23: test262 subset runner

Status: in progress (slices 1–2 done; slice 3 next), run only with approval | Depends on: T12+

## Goal
Run a chosen subset of test262 (by directory or feature list) through js2wasi3 + wasmtime and report the pass rate per directory.
Store the results so a later run only shows the tests whose result changed.

## Rules
Never run it after every edit. Only at the end of a task or on request. Ask before any run expected to take more than 10 minutes.

## Checklist
- [x] Slice 1: runner (`scripts/test262.mjs`, `scripts/fetch-test262.sh`, pin in `TOOLS.md`)
- [x] Slice 2: first subset run, baseline committed
- [ ] Slice 3: top failure causes recorded as follow-ups

## Notes / Result

### Slice 1: runner (2026-09-27)
**What changed**
- `scripts/fetch-test262.sh`: blobless sparse clone of test262 into `test262/` (gitignored) at the commit in
  `TOOLS.md` (`7ab7fafa…`, `main` on 2026-09-27): `harness/` + the slice 2 directories (178 MB, 26,630 files).
- `scripts/test262.mjs` (node, no deps): collects tests under `test262/test/<path>` (or `subset`), parses the
  frontmatter (`includes`, `flags`, `negative`), prepends the harness, builds with the **release** `js2wasi3` and runs
  `wasmtime run -Sp3 -C cache=n -C parallel-compilation=n -O opt-level=0 -O regalloc-algorithm=single-pass`.
  Statuses and rules: see the header of the script and "Testing" in `docs/design.md`. Reasons are one line: the
  `unsupported:` message, the uncaught error, a trap with its two innermost frames, a panic with its location, or the
  async failure. `--causes N` counts the most common reasons (quoted values normalised). Baseline:
  `tests/test262/baseline.json` (one `"path": "status"` per line, sorted, diffs well); not created yet (slice 2).
- `docs/design.md` "Testing": the test262 row and paragraph. `TOOLS.md`: the test262 pin.

**Checked** (no compiler change, so no `cargo test`/`wasm-tools validate` needed beyond the runs):
`built-ins/Math` (327: 162 pass, 165 unsupported, 74 s at -j 8, 72 s at -j 4), `built-ins/Math/{abs,max}` +
`JSON/stringify` (84: 41 pass, 13 fail, 28 unsupported, 2 skip; the same with and without the single-pass allocator),
`language/statements/async-function` (74: 56 pass, 2 fail, 6 unsupported, 10 skip), early-error negatives (pass),
runtime negatives (`const`/`let` TDZ: pass), `flags: [module]` tests, a crash (`await using`, F140), a timeout
(`--timeout 0.4`), the baseline: `--update`, changed-status listing, exit 1 on a regression (the test baseline was
deleted again).

**Speed**: build 0.12 s (release; debug 0.66 s); wasmtime spends ~1.2 s compiling a ~120 KB component (running is
ms); the flags above make that ~0.6 s. On this i7-8565U (4 cores, 8 threads) 8 jobs are no faster than 4, so the
default `-j` is half the logical CPUs: ~4.5 tests/s, i.e. the whole subset takes over an hour (F141).

**Early failure causes seen** (for slice 3): propertyHelper's `Array`/`Object` globals as values (most `length`/
`name`/`prop-desc` tests), `Reflect` (`not-a-constructor` tests), `Proxy`, BigInt, regex literals, wrapper objects
(`new Number(1)` traps, F106), undeclared globals rejected at build time (F139), `using` (F80).

**Unsupported / known noise**: sloppy-mode tests are skipped; tests relying on script semantics fail as modules;
unhandled rejections are fatal (node's behaviour; `async-function/evaluation-body.js`).

### Slice 2: first subset run (2026-09-27)
`node scripts/test262.mjs --update --causes 25 subset` (user-approved; 26,630 files, 6,500 s at -j 4, ~4.1 tests/s),
then two reruns of the tests the runner had misclassified (below). Baseline: `tests/test262/baseline.json`
(26,630 entries, 2.3 MB). Full log with the top 25 reasons per status: `out/test262/subset-2026-09-27.log`
(gitignored; the counts there predate the reclassification).

| directory | pass | fail | unsupported | crash | timeout | skip | pass% |
|---|---|---|---|---|---|---|---|
| built-ins/Array | 2047 | 494 | 478 | 0 | 11 | 52 | 67.6% |
| built-ins/JSON | 87 | 22 | 53 | 0 | 0 | 3 | 53.7% |
| built-ins/Map | 101 | 25 | 74 | 0 | 2 | 2 | 50.0% |
| built-ins/Math | 162 | 0 | 165 | 0 | 0 | 0 | 49.5% |
| built-ins/Promise | 367 | 181 | 173 | 0 | 7 | 4 | 50.4% |
| built-ins/Set | 260 | 13 | 107 | 0 | 1 | 2 | 68.2% |
| built-ins/String | 616 | 278 | 323 | 0 | 0 | 6 | 50.6% |
| language/expressions | 6642 | 593 | 3191 | 0 | 0 | 738 | 63.7% |
| language/statements | 5840 | 139 | 2861 | 9 | 1 | 500 | 66.0% |
| **total** | **16122** | **1745** | **7425** | **9** | **22** | **1307** | **63.7%** |

(pass% = pass / run, skips excluded.) Run history with commits: `tests/test262/HISTORY.md` (add an entry per full run).

**Runner fixes found by the run**
- 110 "build errors" were tests valid only as scripts (`function f(await)`, `var x; function x() {}`, reserved
  words): js2wasi3 is right to reject them as modules. On a build error the runner now asks `node --check` on the
  same source as `.mjs`; if node rejects it too, the test is `skip` ("script only").
- 10 tests failed with "generated WIT is invalid" because the temp file name (`…__proto__…`) became the WIT
  package/world name (`--proto-`): a compiler bug (F143). The runner now names temp files `t<n>.js`; 9 of them pass.

**Top reasons** (before slice 3 turns them into follow-ups): unsupported: global `Array` (2,302, mostly
propertyHelper), `yield*` in an async generator (1,050), dynamic `import()` (469), `eval` (400), undeclared globals
(`x`, `w`, `u`, `s`, `unresolvableReference`, …: ~1,200, F139), `globalThis` (256), private accessors (201),
`Reflect` (197), BigInt (191), `Function` (155), computed instance field keys (144), regex literals (100);
fail: wrapper objects (722 traps, F106), `Test262Error` assertions (448), `Object.defineProperty` traps (202), a
recursion in `strict_eq < float` (27 "call stack exhausted"); crash: `await using` (9, F140); timeouts: 22.

**Next smallest step (slice 3)**: group the top reasons in `out/test262/subset-2026-09-27.log` (and `results.tsv`)
into follow-ups with counts; check a sample of each large `fail` group (Test262Error, `defineProperty` traps,
`strict_eq` recursion, timeouts) for its cause.

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

## Progress
- 2026-09-27: started slice 1 (runner). `check-tools.sh`: all tools unchanged. test262 `main` is at `7ab7fafa0003f73fc85c1b95d88094d33f7eb8bd`.
- 2026-09-27: test262 fetched as a blobless sparse clone (user's choice; harness + the slice 2 dirs, 178 MB, 26,630 test files) by `scripts/fetch-test262.sh`; the commit is pinned in `TOOLS.md`; `/test262` gitignored.
- 2026-09-27: runner `scripts/test262.mjs` (node): frontmatter → harness prepended → release `js2wasi3 build` → `wasmtime run -Sp3`; pass/fail/unsupported/crash/timeout/skip, counts per dir, `--list`, `--causes N`, baseline `tests/test262/baseline.json` (`--update`), changes vs baseline, `out/test262/results.tsv`.
- 2026-09-27: speed: a test is ~0.12 s build (release; debug 0.66 s) + ~1.2 s wasmtime *compiling* the ~120 KB component (running is ms). wasmtime's compile cache is on by default and would grow ~1 MB per test (`~/.cache/wasmtime` is already 1.2 GB from diff.sh runs), so `-C cache=n`. `-O opt-level=0 -O regalloc-algorithm=single-pass -C parallel-compilation=n` roughly halves it; same results on Math/abs, Math/max, JSON/stringify (84 tests: 18 s at -j 8, was 30 s). With 8 jobs a test takes ~2x its solo time (WSL, 8 threads).
- 2026-09-27: slice 1 done (notes above); F139–F142 recorded.
- 2026-09-27: slice 2 started. `built-ins/JSON` + `built-ins/Map` (369 files): 188 pass, 48 fail, 127 unsupported, 2 timeout, 4 skip; 104 s at -j 4 (~3.5 tests/s, so the whole subset ≈ 2 h). The 2 timeouts are real: `new Map(iterable)` ignores a user-replaced `Map.prototype.set`, so it never throws and loops over an infinite iterator. Also seen: `getOrInsert`/`getOrInsertComputed` missing, `WeakMap`, `JSON.rawJSON`.
- 2026-09-27: user approved the full `subset` run (~2 h, -j 4); started in the background with --update --causes 25.
- 2026-09-27: full subset run done (6,500 s). Reclassified 110 script-only build errors as skips (runner checks `node --check` as a module) and fixed 10 WIT-name failures from temp file names (F143); baseline 16,122 pass / 1,745 fail / 7,425 unsupported / 9 crash / 22 timeout / 1,307 skip (63.7%). Slice 2 done.
