# T23: test262 subset runner

Status: in progress (slice 1 done: runner; slice 2 next), run only with approval | Depends on: T12+

## Goal
Run a chosen subset of test262 (by directory or feature list) through js2wasi3 + wasmtime and report the pass rate per directory.
Store the results so a later run only shows the tests whose result changed.

## Rules
Never run it after every edit. Only at the end of a task or on request. Ask before any run expected to take more than 10 minutes.

## Checklist
- [x] Slice 1: runner (`scripts/test262.mjs`, `scripts/fetch-test262.sh`, pin in `TOOLS.md`)
- [ ] Slice 2: first subset run, baseline committed
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

**Next smallest step (slice 2)**: time `built-ins/JSON` and `built-ins/Map` (small), then ask the user before the
full `subset` run (~1.5 h at -j 4); commit `tests/test262/baseline.json` from `--update`.

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
