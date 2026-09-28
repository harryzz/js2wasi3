# T39: RegExp and string-building speed (F170)

Status: todo (handoff written 2026-09-28, not started) | Depends on: T27

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
- [ ] `String.fromCodePoint` is linear; `buildString` of 1.1M code points well under 1 s.
- [ ] `u`-mode greedy one-character loops use `STAR`; `/^\P{ASCII}+$/u` on the 2.1M-unit string well under 1 s.
- [ ] test262: the property-escape timeouts mostly pass; baseline updated (user asked first); F170 ticked or narrowed.

## Notes / Result

## Checklist
- [ ] `String.fromCodePoint` linear
- [ ] `u`-mode `STAR`
- [ ] profile pass (optional step 3)
- [ ] test262 + baseline, follow-ups, docs

## Progress
- 2026-09-28: handoff written after T27 (the user chose F170 next). State at handoff: `main` at `d39dc4a` (local,
  not pushed); test262 subset 23,676 of 26,837 run pass (88.2%); `built-ins/RegExp` 1,188 pass / 447 timeout.
