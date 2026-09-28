# T39: RegExp and string-building speed (F170)

Status: done | Depends on: T27

## Goal
Clear test262's `built-ins/RegExp/property-escapes/generated` timeouts (441) and the 6 `CharacterClassEscapes`
ones, and make regex-heavy programs faster, without changing results. Record the new baseline.

## What we know (measured at handoff, `out/t27/pe*.js`, debug compiler, quiet machine)
Each property-escape test builds its inputs with the harness's `buildString` (`test262/harness/regExpUtils.js`):
up to 1.1M code points, `String.fromCodePoint.apply(null, chunk)` per 10,000 code points, `result += …`; then
`testPropertyEscapes` runs `/^\p{…}+$/u.test(string)` (and, only if that fails, one test per code point).
For `\P{ASCII}` (2,162,560 code units):
- building the string: **7.1 s**, 93% in `concat` inside `$$String_fromCodePoint` (`builtins/string.js`), which
  appends one code point at a time with `+` (strings are flat, so each call is quadratic in its argument count);
  `$$String_fromCharCode` already fills one `str_new` with `str_set`;
- the regex test: **~2 s** (~1.8 µs per code point): in `u` mode a greedy one-character loop takes the general
  REPEAT path (`REPEAT_LOOP`/`REPEAT_START`/`REPEAT_END`: two undo entries and a choice per character), since the
  `STAR` fast path (`builtins/regexp.js`, opcode 18) is limited to non-`u`, forward, one-unit atoms;
- wasmtime compiles a program that links the engine in ~1–2 s (~40K instructions; `$$reRun` is the largest function).
  The test262 runner compiles at opt-level 0 with the single-pass register allocator, so this matters less there.
The test262 timeout is 10 s per test (build + run), with `-j 4` on this laptop.

## Plan
1. **`String.fromCodePoint`**: count the code units first (1 or 2 per code point, validating as now), then one
   `__intrinsic_str_new(n)` filled with `__intrinsic_str_set` (like `fromCharCode`). Keep the RangeError text.
   Measure `out/t27/pe-build.js` again (expect well under 1 s).
2. **A `u`-mode `STAR`**: extend opcode 18 to atoms in Unicode mode: consume one code point per step (width 1 or 2,
   `$$reRead`), and give back one code point per backtrack. The give-back entry `[-16 - next, pos, lowest]` only
   holds a position, so in `u` mode step back over a surrogate pair when the unit before `pos` is a trail preceded
   by a lead (the matched run never splits a pair). Keep the backward (`dir` 1, lookbehind) case on the general
   path unless it's simple to add.
3. Optionally, if a profile still shows it: an inline ASCII/BMP fast path in `$$reOne` for CLASS without `i`
   (avoid the `$$reInRanges` call for small sets), and the per-call argument array of `re.test` (F170's last part:
   calls through prototype methods allocate an argument array; check whether a direct-call path exists).
4. test262: run `built-ins/RegExp` (and `built-ins/String/prototype/fromCodePoint`), then the whole subset with
   `--update` at the end (ask the user before committing the baseline, as in T27).

## Read first
- `tasks/T27-regex.md` (Notes / Result, Progress: the VM's design, the performance work already done and why:
  `+` on array reads, `i32_*` intrinsics, numbers instead of booleans (F14), inline ToInt32).
- `crates/lower/builtins/regexp.js`: `$$reEmitRepeat` (where `STAR` is chosen), `$$reRun` (opcode 18 and the
  `k <= -16` backtrack case), `$$reOne`, `$$reRead`.
- `crates/lower/builtins/string.js`: `$$String_fromCodePoint`, `$$String_fromCharCode`.
- F170, F14, F19 in `tasks/README.md`.

## Tools
- Timing: `out/t27/time.sh` (min of 3 runs); build with `--core` or compile once with `wasmtime compile` and run
  the `.cwasm` with `--allow-precompiled` to separate compile time from run time.
- Profiling: `wasmtime run -Sp3 --profile=guest,prof.json,1ms x.wasm`, then summarize self/inclusive time per
  function from the profile's `threads[0]` tables (T27 used a small Python script in its scratchpad; rewrite it:
  count `samples.stack` leaf frames by `funcTable.name`).

## Tests
`scripts/diff.sh tests/fixtures/builtins/regex-*.js tests/fixtures/builtins/string-methods.js` (results must not
change), `cargo test -p js2wasi3-lower`, `wasm-tools validate` + one wasmtime run; the test262 runs in step 4.
Add a fixture case for `String.fromCodePoint` with many arguments (astral and BMP mixed) and for `u`-mode greedy
loops that backtrack over surrogate pairs (`/^.+😀$/u`, `/(?:😀)*😀/u`, `/.*x/u` on astral text).

## Acceptance
- [x] `String.fromCodePoint` is linear; `buildString` of 1.1M code points well under 1 s.
- [x] `u`-mode greedy one-character loops use `STAR`; `/^\P{ASCII}+$/u` on the 2.1M-unit string well under 1 s.
- [x] test262: the property-escape timeouts mostly pass; baseline updated (user asked first); F170 ticked or narrowed.

## Notes / Result
All 447 RegExp timeouts pass; results unchanged elsewhere. test262 subset: 24,123 of 26,837 run pass (89.9%, was
88.2%); `built-ins/RegExp` 1,635 pass (was 1,188), `property-escapes` 98.9%, 0 timeouts left in RegExp.

What changed (all in the prelude):
- `builtins/string.js`: `$$String_fromCodePoint` converts and checks every argument first (ToNumber once each, in
  order, so a RangeError still comes before later conversions), counts units, fills one `str_new` (was `+` per
  code point: quadratic). 1.1M code points: 7.1 s → 0.4 s.
- `builtins/regexp.js`:
  - `STAR` (opcode 18) works in `u`/`v` mode: one code point per step (`$$reRead`), `lowest` = the position after
    `min` characters; a give-back steps over a whole pair (the run never splits one). Backward loops (lookbehind)
    stay on the general REPEAT path.
  - Unboxed hot helpers (F14): `$$reInRanges` returns 1/0 and takes a number (callers write `+$$reCanon(…)`), a
    class's negate flag in `sets` is 1/0, `$$reRead` takes `unicode` as a number with inline surrogate checks,
    `$$reOne` takes `unicode`; `$$reClassHas(r, neg, …)` lets loops read a class's ranges once.
  - `$$reSearch` skips start positions whose character the program's first instruction rejects (CHAR/CLASS
    forwards, or a leading `STAR` with `min` ≥ 1), without starting the VM; not with `y`.
- Timings (release compiler, the runner's wasmtime flags, alone): Script_-_Adlam 2.9 s → 1.55 s, the `\d`
  negative cases 4.5 s → 1.76 s; `out/t27/pe.js` (`\P{ASCII}`, build + test) ~9 s → 0.8 s.

Commands: `scripts/diff.sh tests/fixtures/builtins/regex-*.js tests/fixtures/builtins/string-methods.js` (6/6),
`cargo test -p js2wasi3-lower`, `wasm-tools validate` + wasmtime on `out/t27/pe.wasm`,
`node scripts/test262.mjs built-ins/RegExp built-ins/String/fromCodePoint`, then `subset --update` (10,372 s).
The one "regression" of that run, `language/statements/function/S13.2.1_A1_T1.js`, compiles to the same bytes as
before this task; it takes 4–5 s alone and timed out under load. Rerun alone with `--update`: pass (F180).

Left for later: F177 (argument array per `re.test` call), F178 (wasmtime compile time of the engine), F179 (class
membership per character: binary search; backward `STAR`), F180 (the slow `S13.2.1_A1_T1`).
Next smallest step: F179's ASCII bitmap for classes, if a regex-heavy benchmark calls for it.

## Checklist
- [x] `String.fromCodePoint` linear
- [x] `u`-mode `STAR`
- [x] profile pass (optional step 3)
- [x] test262 + baseline, follow-ups, docs

## Progress
- 2026-09-28: handoff written after T27 (the user chose F170 next). State at handoff: `main` at `d39dc4a` (local,
  not pushed); test262 subset 23,676 of 26,837 run pass (88.2%); `built-ins/RegExp` 1,188 pass / 447 timeout.
- 2026-09-28: started; tools unchanged. Step 1: `$$String_fromCodePoint` converts and checks every argument first
  (ToNumber once each, written back into the rest array), counts units, then fills one `str_new`.
  `out/t27/pe-build.js` (1.1M code points): 7.1 s → 0.40 s (debug compiler, compile + run).
- 2026-09-28: step 2: `STAR` (opcode 18) now also in `u`/`v` mode: `$$reRun` reads code points with `$$reRead`
  (records `lowest` = position after `min` characters), `$$reOne` takes `unicode` (canonicalizes like CHAR/CLASS);
  the backtrack entry steps back over a pair when the given-back unit is a trail preceded by a lead above `lowest`.
  Fixture cases in `regex-unicode.js` (pairs given back, `{n,m}`, lone surrogates, `iu`, lastIndex mid-pair) and
  `string-methods.js` (3,000-argument `fromCodePoint`, ToNumber order/once, RangeError). diff.sh: 6/6 pass.
  `out/t27/pe.js` (`\P{ASCII}`): build + test 0.78 s total (was ~9 s).
- 2026-09-28: test262 `built-ins/RegExp` + `String/fromCodePoint` (-j 4): 240 timeouts → pass, 0 regressions,
  207 still time out (124 Script, 38 General_Category, 32 Script_Extensions, 3 CharacterClassEscapes negative
  cases, a few binary properties). Alone they pass in 3–4.5 s: `-j 4` load pushes them past 10 s.
  Profile (Script_-_Adlam): boxed booleans/numbers in the hot helpers. Step 3 done:
  `$$reInRanges` returns 1/0 and takes a number (`+$$reCanon(…)` at callers), a class's negate flag is stored
  as 1/0, `$$reRead` takes `unicode` as a number with inline surrogate checks, `$$reClassHas(r, neg, …)` lets
  the STAR loops read a class's ranges once; `$$reSearch` skips start positions whose character the first
  instruction (CHAR/CLASS, or a STAR's atom with min ≥ 1) rejects, without starting the VM.
  `fromCodePoint` uses `>> 10`/`& 1023`. Standalone run (release, runner's wasmtime flags): Script_-_Adlam
  2.9 s → 1.55 s, digit-negative-cases 4.5 s → 1.76 s.
- 2026-09-28: fixture cases for the search prefilter in `regex-engine.js` (unanchored `\d`, `i`, `u` pairs, `g`
  loops, `split`/`replace`/`matchAll`, sticky, lastIndex mid-pair); diff.sh 6/6 pass. test262 `built-ins/RegExp` +
  `String/fromCodePoint` (-j 4, 1,166 s): all 447 former timeouts pass, 0 timeouts left, 0 regressions;
  RegExp 1,635 pass (was 1,188), property-escapes 98.9%. Next: the whole subset with `--update`.
- 2026-09-28: whole subset with `--update` (10,372 s): 447 timeout → pass, one load-flaky timeout
  (`S13.2.1_A1_T1`, same wasm bytes as before; rerun alone: pass, F180). Baseline: 24,123 of 26,837 run (89.9%).
  Notes / Result, follow-ups F177–F180, README written. Commit waits for the user's OK.
