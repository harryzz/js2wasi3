# T81: RegExp fast paths for unmodified RegExp objects (F349)

Status: done (2026-10-05) | Depends on: none (independent of T80) | Cloud batch: yes (`tasks/CLOUD-BATCH.md`)

## Goal
`RegExp.prototype[Symbol.match]`, `[Symbol.replace]`, `[Symbol.split]` (and `String.prototype.match/replace/
replaceAll/split/search` through them) skip the spec's observable steps when nothing could observe them: the regexp is
a plain RegExp whose prototype chain and own properties are untouched (no own `exec`/`flags`/`lastIndex` accessor, a
writable data `lastIndex`, `RegExp.prototype.exec`/`flags`/the flag getters and `Symbol.*` methods unmodified). Then
read the flags from the regexp's internal data instead of calling the `flags` getter (which reads 8 getters), and call
the matcher directly instead of `$$reExecAny` → `exec` lookups. Same results as node in every case, including when
user code does modify those things (then the slow, spec path runs).

## Why (T79, measured on the replay; don't re-measure here)
zigbee2mqtt at a 1 GiB heap: `$$RegExp_prototype_$match` 4.1% inclusive, the `flags` getter alone 2.5% (called by
`Symbol.match`/`replace`/`split` per spec), `$$RegExp_prototype_$replace` 2.1%: ~9% of the CPU.

## Read first
- `crates/lower/builtins/regexp.js`: `$$RegExp_prototype_flags` (~2709), `$match` (~2846), `$replace` (~2995), `$split`
  (~3074), `$matchAll` (~3170), `$$reExecAny`, `$$reRun`, `$$reSymThis`; how the internal data is read
  (`__intrinsic_regexp_data`).
- How js2wasi3 can tell a built-in is unmodified: e.g. `__intrinsic_builtin_tag`, and how the prelude detects modified
  prototypes elsewhere (search `builtins/` for existing "unmodified"/pristine checks; T34's `G_INDEX_PROTO` is an
  example of a "someone touched this" flag in the runtime).
- test262 `built-ins/RegExp/prototype/Symbol.*` and `built-ins/String/prototype/{match,replace,split}` (in the subset):
  they test the observable steps; the fast path must not change them.

## Plan
1. A cheap "pristine" check (a runtime flag set when `RegExp.prototype` or its `exec`/`flags`/flag getters/`Symbol.*`
   methods are written or redefined, plus per-object: a plain RegExp with no own properties but `lastIndex`).
2. Fast paths in `$match`, `$replace`, `$split` (and `$matchAll`/`$search` if cheap) under that check; the existing
   code stays the slow path.
3. Tests: `tests/fixtures/builtins/regex-*.js` and `regexp-modifiers.js` via `scripts/diff.sh`; a new fixture
   `tests/fixtures/builtins/regex-observable.js` that modifies `exec`, `flags`, `lastIndex` (accessor, non-writable),
   `RegExp.prototype[Symbol.replace]`, subclasses, and checks the observable order like node.
4. A micro-benchmark (sanity check only): `tasks/T81-files/regex-bench.js` (`str.replace(/x/g, …)`, `match`, `split`
   on log-line-like strings), node vs js2wasi3, before/after.
5. test262: only the RegExp/String prototype directories touched, locally (`scripts/test262.mjs` with a filter);
   never the full suite here.

## Acceptance
- [x] Fast paths in place; any modification falls back to the spec path with node's results.
- [x] Fixtures pass `scripts/diff.sh`; the touched test262 directories show no regressions; `wasm-tools validate`;
      a wasmtime smoke run; `cargo test` of the touched crates.
- [x] Micro-benchmark before/after in Notes; F349 ticked in `tasks/README.md`; the replay measurement is T82.

## Notes / Result
**What changed** (`crates/lower/builtins/regexp.js` only). The "nobody changed it" check already existed: T76's
`__intrinsic_regexp_plain(rx)` (a RegExp whose own properties are just `lastIndex`, a writable number, with
`RegExp.prototype` as built: `exec`, `flags`, the flag getters). It is checked after ToString(string), which can run
user code. Under it:
- `Symbol.match`: `$$reMatchPlain`: non-global → `$$reExec` directly; global → `$$reSearch` + substrings (no result
  array per match), the same `lastIndex` writes.
- `Symbol.search`: `$$reSearch`, the index from the captures, no result array.
- `Symbol.replace`: flags from the regexp's data (`code[7]`, same order as the getter), `$$reExec` instead of
  `$$reExecAny` (no `exec` lookup); all matches are found before any replacement function runs, so a function that
  changes things can't observe the difference. (T76's `$$reReplacePlain` still takes plain string replacements.)
- `Symbol.split`: no `flags` getter and no splitter object: the RegExp's own code runs anchored at each position
  (`$$reRun` ignores `y`), read before ToUint32(limit). New for every direct split (plain or a fresh splitter with the
  built-in exec): positions whose unit no match starts with are skipped with the first-unit filter (`code[10]`, as
  `$$reSearch`; null in Unicode mode or when a match can be empty).
- `Symbol.matchAll`: flags from the data.
Anything modified (own `exec`, prototype `exec`/`flags`/flag getters, a non-number or non-writable `lastIndex`, another
prototype) takes the existing spec path.

**Tests.** New `tests/fixtures/builtins/regex-observable.js` (plain cases, own/prototype `exec`, logging `flags`, a
lying `global`, `lastIndex` objects / read-only, ToString and limit side effects, a replacement function that changes
`exec`/`lastIndex`, a replaced `Symbol.replace`, another prototype; then the plain cases again after the restores);
it and `regex-{case,engine,object,strings,unicode}.js`, `regexp-modifiers.js` pass `scripts/diff.sh`. test262
(release compiler): `built-ins/RegExp/prototype/Symbol.{match,replace,split,search,matchAll}` and
`built-ins/String/prototype/{match,matchAll,replace,replaceAll,split,search}`: 555 tests, no changes from the
baseline. `cargo test -p js2wasi3-lower` passes; `wasm-tools validate` and wasmtime runs of the benchmark.
Found along the way, not fixed (out of scope): `class extends RegExp` instances get `RegExp.prototype` (F355);
node's `Symbol.replace` reads `global`/`unicode`, not `flags` (F356: following node broke two test262 tests, so the
spec behaviour stays); node 24 loops forever on `'a😀b'.replace(/(?:)/gv, '-')` (not a js2wasi3 issue).

**Micro-benchmark** (`tasks/T81-files/regex-bench.js`, 20,000 iterations each on a 140-unit log line; wasmtime 49.0.2
`-O gc-heap-initial-size=536870912`, cloud sandbox; ms, before/after as the mean of two alternating runs):

| op | node | before | after |
|---|---|---|---|
| replace /"/g, string | 17 | 236 | 242 |
| replace /[{}]/g, '<$&>' | 21 | 257 | 180 |
| replace /\d+/g, fn | 29 | 775 | 672 |
| replace /^\[[^\]]*\] / once | 6 | 87 | 93 |
| match /\w+/g | 19 | 877 | 585 |
| match /topic '([^']*)'/ | 4 | 121 | 100 |
| split /[,:]\s*/ | 15 | 844 | 260 |
| search /payload/ | 2 | 53 | 51 |
| test /info/ | 2 | 26 | 26 |

Gains where the getters, `exec` lookups and arrays were the cost (match /g 1.5×, split 3.2×, `$&` 1.4×); the rest is
the matcher itself and the replacement function's arrays (F354). The replay measurement is T82 (local, a-03).

**Next smallest step.** T82 (local): one replay per change against T79's numbers.

## Progress
- 2026-10-05: handoff written.
- 2026-10-05: started (cloud batch, after T80). The pristine check already exists: T76's
  `__intrinsic_regexp_plain` (a RegExp with only its own `lastIndex`, a writable number, and `RegExp.prototype`'s
  `exec`/`flags`/flag getters as built). Benchmark before (tasks/T81-files/regex-bench.js): see Notes.
- 2026-10-05: fast paths under `regexp_plain` in `$match` (`$$reMatchPlain`: non-global → `$$reExec`, global →
  `$$reSearch` and substrings, no result arrays), `$search` (`$$reSearch`, no array), `$replace` (flags from the data,
  `$$reExec` instead of `$$reExecAny`; matches are all found before any replacement function runs), `$split` (no
  `flags` getter, no splitter object: the RegExp's own code runs anchored), `$matchAll` (flags from the data).
- 2026-10-05: new fixture tests/fixtures/builtins/regex-observable.js. Found and fixed: node's `Symbol.replace` reads
  `global` then `unicode`, not `flags` (we read `flags`: 5 getters). Found, not fixed: `class extends RegExp`
  instances get `RegExp.prototype` (F355; subclass cases left out of the fixture); node 24 loops forever on
  `'a😀b'.replace(/(?:)/gv, '-')` (left out). All regex fixtures pass.
- 2026-10-05: `split` with the matcher run directly skips positions outside the first-unit filter (`code[10]`, as
  `$$reSearch`): split 840 → 260 ms in the benchmark. Running cargo test (lower) and test262 (release build).
- 2026-10-05: reading `global`/`unicode` in `replace` like node regressed two test262 tests (`Symbol.replace/flags-tostring-error.js`, `get-unicode-error.js`): reverted, F356. test262 on the touched directories: no changes from the baseline. Done.
