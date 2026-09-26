# T26: `Date`

Status: todo (later) | Depends on: T21 (prelude), T25 slice 4 (built-ins as values) helps

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
