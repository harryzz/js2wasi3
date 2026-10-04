# T81: RegExp fast paths for unmodified RegExp objects (F349)

Status: todo (handoff written 2026-10-05) | Depends on: none (independent of T80) | Cloud batch: yes (`tasks/CLOUD-BATCH.md`)

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
- [ ] Fast paths in place; any modification falls back to the spec path with node's results.
- [ ] Fixtures pass `scripts/diff.sh`; the touched test262 directories show no regressions; `wasm-tools validate`;
      a wasmtime smoke run; `cargo test` of the touched crates.
- [ ] Micro-benchmark before/after in Notes; F349 ticked in `tasks/README.md`; the replay measurement is T82.

## Notes / Result
(to be written)

## Progress
- 2026-10-05: handoff written.
