# T26: `Date`

Status: in progress | Depends on: T21 (prelude), T25 slice 4 (built-ins as values) helps

## Goal
`Date` as in ECMAScript: `new Date()`, `new Date(ms)`, `new Date(y, m, d, …)`, `Date.now`, `Date.UTC`,
`Date.parse` (the ISO format plus what V8 accepts for the common `toString` forms), getters/setters (local and
UTC), `toISOString`, `toJSON` (used by `JSON.stringify`), `toString`/`toDateString`/`toTimeString`/`toUTCString`,
`valueOf`, `Symbol.toPrimitive` (hint default = string), `console.log` printing like node (ISO form).

## Design questions (decide first, record in D5)
- **Clock**: `Date.now()` from `wasi:clocks/system-clock` (0.3.1; imported only when used, like `wasi:random` for
  `Math.random`); libraries need it in their world (T20).
- **Time zone**: `wasi:clocks/timezone` is `@unstable(feature = clocks-timezone)` in 0.3.1 (check wasmtime 49 support
  with a probe). Options: UTC only (local = UTC), or the host offset through the unstable interface. The
  differential harness must pin the zone for node (`TZ=UTC` for Date fixtures) either way.
- **Algorithms**: the spec's day/time arithmetic (MakeDay, MakeTime, DaysInYear…) in the prelude, on `f64`.
- `toLocale*String` (needs `Intl`): out of scope; reject or give the non-locale form (decide).

## Acceptance
- [ ] Fixtures in `tests/fixtures/builtins/date-*.js` match node with a fixed `TZ` (`scripts/diff.sh`).
- [ ] Programs without `Date` don't import the clock or link the date code.

## Notes / Result

## Progress
- 2026-09-28: started (after T39). Tools unchanged. Probe (scratchpad `tzprobe/`: a core module importing
  `wasi:clocks/timezone@0.3.1` `utc-offset`, embedded with `--features clocks-timezone`): wasmtime 49.0.1 fails to
  instantiate it ("function implementation is missing"; `-S help` lists no `clocks-timezone`). Decision: local time
  is UTC (offset 0, like node with `TZ=UTC`); Date fixtures compare with node under `TZ=UTC`.
- 2026-09-28: design (read RegExp's wiring as the template):
  - A Date is a `$object` of class `CLASS_DATE` (object.rs) whose data `[tv]` (a prelude array, mutated by the
    setters) is under the private `G_SYM_STATE` key: intrinsics `date_new(data)`/`date_data(v)` like
    `regexp_new`/`regexp_data`. `Builtin::Date` (ctor length 7) gives `Date.prototype` and the constructor value.
  - Everything else is prelude JS (`builtins/date.js`): `$$Date` is the construct path (`new Date(…)`, and the
    value's construct entry); `$$DateCall` is the call path (`Date()` → a string), wired like `STRING_TO_NUMBER`
    (`Module::date_call`) and used as the value's call entry. `$$dateInspect(d)` gives `console.log`'s text
    (`Module::date_inspect`, called by inspect.rs). Statics `$$Date_now/UTC/parse`, methods
    `$$Date_prototype_*`, `$$Date_prototype_$toPrimitive`.
  - Clock: `__intrinsic_date_now()` → `wasi:clocks/system-clock@0.3.1` `now` (imported by commands only when
    used, like `wasi:random`; the record comes back through memory at 16, `abi::HEAP_BASE` 8 → 32).
    Libraries: rejected where the code names `Date` (follow-up).
  - New intrinsic `to_primitive(v, hint)` (0 default, 1 number, 2 string) over `object::Helper::ToPrimitive`;
    the runtime's ToPrimitive turns hint default into string for a Date (spec: `Date.prototype[@@toPrimitive]`).
  - `Object.prototype.toString` → `[object Date]` (props.rs), `console.log` → ISO text or `Invalid Date`.
- 2026-09-28: implemented the design: `Builtin::Date`, intrinsics `date_new`/`date_data`/`date_now`/`to_primitive`,
  `CLASS_DATE`, `Import::SystemClockNow` (system-clock result at memory 16; `abi::HEAP_BASE` 32), `Module::date_call`
  / `date_inspect`, `builtins/date.js` (constructor, statics, getters/setters, formats, ISO + legacy parse,
  `toJSON`, `@@toPrimitive`, en-US `toLocale*String()` without arguments). First smoke test (`out/t26/a.js`: all
  string forms, negative/5-digit years, Invalid Date, printing in arrays/objects, `Date()`, `Date.length`) matches
  `TZ=UTC node`.
- 2026-09-28: `scripts/diff.sh` runs node with `TZ=UTC`. Fixtures `date-basic.js` (construction, getters, every
  string form, ranges, rollover, printing, `Date()`), `date-set.js` (setters: present arguments, conversion order,
  NaN, V8's receiver messages per method: getters "this is not a Date object.", the rest "Method
  Date.prototype.X called on incompatible receiver …") pass. `toGMTString` is `toUTCString` (an alias, like
  `Set.prototype.keys`).
- 2026-09-28: `date-parse.js` (ISO + legacy; V8 reads `10:61` as 10:00: not chased, left out) and `date-convert.js`
  (ToPrimitive, JSON, `@@toPrimitive`, reflection, lengths, Date as a value) pass after fixes:
  `Object.prototype.toString` → `[object Date]` (object.js), `$toPrimitive` methods linked by naming
  `toPrimitive`, `toLocale*String.length` 0, legacy parse rejects a leading sign and `.` without digits.
  Subclassing: `$$Date` reads the new target from `this` (the construct entry passes it; `new Date` passes
  `undefined`) and sets the prototype; `$$dateInspect` prefixes a subclass's name (`date-subclass.js` passes).
  No-Date programs: no clock import, no date code; Date programs don't link the regex engine; a library using
  `Date` gets "unsupported: `Date` in a module with exports".
- 2026-09-28: `cargo test` all pass; `scripts/diff.sh tests/fixtures/builtins` 44/44 (+1 library skip).
  test262: `built-ins/Date` added to the subset and the sparse checkout (594 files). First run 574 of 591 run
  (97.1%); then: `inherited_prop` also covers `Object.prototype` methods of constructor values
  (`Date.hasOwnProperty(…)`, 4 tests; also helps `Array.hasOwnProperty` & co.), `Date.prototype[Symbol.toPrimitive]`
  read-only, `Temporal` added to the runner's NODE_MISSING (node 24 lacks it: 8 `toTemporalInstant` tests skip).
  Now 579 of 583 run (99.3%); the 4 failures use top-level `this` as the global object (F151, not Date).
  Next: the whole subset with `--update`.
- 2026-09-28: found a silent miscompile: in a library `date_now` returned `Err(Unsupported)` inside prelude code
  and the lowering dropped the statement (`new D()` through a value gave NaN, `$$DateCall` an empty body).
  Now `Date` works in libraries and reading the clock throws an Error ("the current time (the system clock)
  isn't available in a module with exports yet"); the `check_builtin` rejection is gone. Test: `tests/fixtures/
  builtins/lib.js` `isoDay`/`now` in `build.rs::builtins_in_a_library`. The same hole exists for `Math.random`
  through a value in a library (`const M = Math; M.random()` builds and runs): follow-up.
  Docs: design.md (T26 note under D5: local time is UTC), keywords.md (Date 🟡).
- 2026-09-28: the whole-subset run was stopped at the user's request (to run tonight); `baseline.json` untouched.
  **To finish T26:** run `node scripts/test262.mjs subset --update --causes 5` (~3 h at -j 4; the release
  binary must be rebuilt first: `cargo build --release`), check regressions (HEAP_BASE 8 → 32, ToPrimitive,
  `inherited_prop` for constructor values touch everything), then: `tests/test262/HISTORY.md` entry, Notes /
  Result, README status, follow-ups (planned: `Math.random` through a value in a library silently loses its
  statement (same hole as `date_now` had); `Date` clock in libraries (import `wasi:clocks/system-clock` in
  library worlds); `toLocale*String` with locales/options (`Intl`); V8 legacy `Date.parse` quirks not matched
  (`10:61` → 10:00); host time zone once `wasi:clocks/timezone` is stable and in wasmtime), ask before commit.
- 2026-09-28: fixed the `Math.random` hole found above: `random_seed` in a library no longer returns a bare
  `Err(Unsupported)` (which dropped the statement with no diagnostic; the build succeeded). Reached through a
  value (`const M = Math; M.random()`) it now throws an Error at run time, like the clock (shared
  `library_throws` in lower); a direct `Math.random` in a library is still a build error. Test: `lib.js` `rand`
  in `build.rs::builtins_in_a_library`. The other bare `Err(Unsupported)` returns in lower follow a pushed
  diagnostic. `cargo test` all pass.
