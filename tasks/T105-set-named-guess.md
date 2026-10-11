# T105: a per-site entry guess for writes to existing properties (F372)

Status: done (2026-10-11) | Depends on: T104 | Local

## Goal
`o.x = v` on an existing own property (the common update) runs `find_own_named` (a hash or a scan) every time;
reads (`get_named`, T74) first try the entry their site remembers. Profile, 2026-10-10 (`docs/performance.md`):
writes to existing properties are up to 15% of nbody and 11% of richards (F372; EarleyBoyer `set_named` 4.5% +
`find_own_named` 2.0% after T90).

## Design
- `set_named(v, k, value, hash, site, ic)`: a second site number, a read-style one (`ic`, from the same counter
  as `get_named`'s), so the guess lives in `G_IC` and `ic_store` grows it.
- After the object test: `E = G_IC[ic]`; when `E < count`, `keys[E] == k` (keys are unique, a deleted entry's key is
  null) and `attrs[E] & (ACCESSOR | READONLY | WATCHED) == 0`: `vals[E] = value`, return. That is exactly the own
  writable data hit of today's path, without the lookup.
- On today's own writable hit: remember `E` (`ic_store`).

## Read first
- `crates/backend/src/object.rs`: `set_named`, `get_named`, `ic_store`; `crates/backend/src/lib.rs` (`SetProp`
  with a constant key, `ic_sites`/`set_sites`).

## Plan
- [x] The guess in `set_named`, the extra parameter, the call site.
- [x] Fixtures: `objects/`, `classes/`, `builtins/` (frozen, accessors, watched entries: `watch-iter.js`) = node; a
  new fixture for the cases a wrong guess would break (delete then re-add, the same site on objects of different
  layouts, a property made read-only / an accessor after the site cached it, `Object.freeze` after caching).
- [x] test262 (full run on a-03, ask); a-03 before/after on the 18 benchmarks.

## Acceptance
- Every touched fixture = node; test262 0 regressions; nbody/richards faster, nothing slower beyond noise.

## Notes / Result
**What changed** (backend only): `object::Helper::SetNamed` takes a sixth parameter, `ic` (a read-style site
numbered by `ic_sites`, so its guess lives in `G_IC` and `ic_store` grows it). `set_named` first tries `G_IC[ic]`:
when that entry is below `count`, its key is `k` and its attributes have none of `ACCESSOR | READONLY | WATCHED`, it
writes the value there and returns: the same own writable data hit as before, without `find_own_named`. The
existing own-hit path remembers its entry (`ic_store`). Everything else (new properties: T90's prototype check;
setters, read-only, watched entries: `ordinary_set_named`) is unchanged.

**Result** (a-03, 5 interleaved runs): nbody 6,092 → 5,227 ms (−14.2%), octane-richards 5,079 → 4,542 ms (−10.6%);
the other 16 within noise; outputs the same.

**Tests.** New `tests/fixtures/objects/set-guess.js` = node; `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`:
99 pass; `cargo test -p js2wasi3-backend` ok; `wasm-tools validate`; full test262 on a-03 (`~/t262-t105.log`): no
changes since the baseline (28,105 of 28,984).

**Not done.** Nothing deferred. Writes still pay the `set_named` call itself (F359: inlining the guess at the site).

**Next smallest step.** None here. Next speed items in `docs/performance.md`: F336 (regex literals compiled ahead
of time) and F323/F391 (numbers in fields and arrays without boxing).

## Progress
- 2026-10-11: task created (user: "Next up"; F372 was the first speed item left in `docs/performance.md`).
- 2026-10-11: implemented as designed (`set_named` gets `ic`, a `G_IC` site from `ic_sites`; the guess block before
  `find_own_named`; `ic_store` on the own writable hit). Release build clean. Fixture
  `tests/fixtures/objects/set-guess.js` (one site over three layouts, delete + re-add, read-only / accessor /
  frozen / sealed / non-enumerable after caching, a prototype setter, an array's named property, a class method's
  sites over instances and a plain object, a hashed object): = node (my first version forgot that modules are
  strict: a refused write throws there, in node too). `scripts/diff.sh` on `objects/`, `classes/`, `builtins/`:
  99 pass; `cargo test -p js2wasi3-backend` ok.
- 2026-10-11: a-03 (user: both runs now): `~/js2wasi3-t104` (HEAD) vs `~/js2wasi3-t105` on the 18 benchmarks (`~/t105-bench.sh`, 5 interleaved runs, `~/t105-bench.log`), then the full test262 (`~/t262-t105.log`), one detached script (`~/t105-all.sh`).
- 2026-10-11: a-03 before/after (T104 vs T105, 5 interleaved runs, median ms, every output the same):
  | program | T104 ms | T105 ms | change |
  |---|---|---|---|
  | mandelbrot | 245 | 250 | +2.0% |
  | spectralnorm | 1,261 | 1,271 | +0.8% |
  | fannkuchredux | 9,034 | 9,008 | -0.3% |
  | nbody | 6,092 | 5,227 | -14.2% |
  | binarytrees | 9,189 | 9,178 | -0.1% |
  | fasta | 956 | 942 | -1.5% |
  | knucleotide | 5,148 | 5,112 | -0.7% |
  | pidigits | 978 | 978 | +0.0% |
  | regexredux | 3,046 | 2,997 | -1.6% |
  | revcomp | 506 | 520 | +2.8% |
  | octane-richards | 5,079 | 4,542 | -10.6% |
  | octane-deltablue | 3,759 | 3,752 | -0.2% |
  | octane-raytrace | 7,258 | 7,248 | -0.1% |
  | octane-navier-stokes | 3,333 | 3,329 | -0.1% |
  | octane-crypto | 5,824 | 5,786 | -0.7% |
  | octane-splay | 3,495 | 3,484 | -0.3% |
  | octane-earley-boyer | 10,207 | 10,257 | +0.5% |
  | octane-regexp | 22,217 | 22,060 | -0.7% |
  nbody −14.2% and richards −10.6% (the profile's 15% / 11% for writes to existing properties); the rest within
  ±1.6% except mandelbrot +2.0% and revcomp +2.8%, whose runs overlap (238–259 vs 244–259, 499–521 vs 498–531 ms): noise.
- 2026-10-11: full test262 on a-03 (1,273 s): no changes since the baseline; `baseline.json` copied back (identical).
  Docs: `design.md`, `performance.md`, test262 `HISTORY.md`, `tasks/README.md` (T105 done, F372 ticked), `CLAUDE.md`. Done.
