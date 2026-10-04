# T77: `Intl` (ECMA-402) and the locale-sensitive built-ins

Status: in progress (started 2026-10-04) | Depends on: T21 (prelude), T26 (`Date`), T27 (Unicode data generation), T40 (BigInt)

## Goal
A full ECMA-402 implementation in js2wasi3, like node 24.21.0 (the oracle: ICU 78.3, CLDR 48, Unicode 17.0, tzdata
2026c, full ICU, 418 time zones): the `Intl` namespace and every constructor node has, and the locale-sensitive
methods of the other built-ins. js2wasi3 is a general JavaScript compiler; zigbee2mqtt is one test program. Same
results as node everywhere (`scripts/diff.sh`, test262 `intl402`). Programs that don't use `Intl` or the
locale-sensitive methods keep their bytes (gated like BigInt, T40).

## Why now
- **A real crash** (2026-10-04, production run of zigbee2mqtt): a Zigbee device read the coordinator's time cluster;
  zigbee-herdsman's `utils/timeService.js` `getTimeClusterAttributes()` calls
  `Intl.DateTimeFormat().resolvedOptions().timeZone` and `@date-fns/tz`'s `tzScan`/`tzOffset`
  (`new Intl.DateTimeFormat("en-US", { timeZone, timeZoneName: "longOffset" }).format(date)`, the text after `GMT`);
  js2wasi3 has no `Intl`: `Uncaught ReferenceError: node's global 'Intl' isn't available in js2wasi3 (planned: T21)`
  ends the program (twice in ~6 minutes of the old build on the production network; F330). The message's
  "planned: T21" is stale (T21 is done): `crates/lower/src/lib.rs` ~line 7929.
- `docs/keywords.md` lists `Intl` first under "What's most needed". Open follow-ups that wait for it: F107
  (`localeCompare` beyond ASCII, locales/options, `normalize`, `Number.prototype.toLocaleString` ≥ 1e21), F182
  (`toLocaleString`/`toLocaleDateString`/`toLocaleTimeString` with locales or options throw a RangeError; also
  `BigInt.prototype.toLocaleString`), F184 (local time is UTC).

## What we know (at handoff)
- **Scope (node 24.21.0)**: `Intl.getCanonicalLocales`, `Intl.supportedValuesOf`, `Intl.Locale`, `Intl.Collator`,
  `Intl.NumberFormat` (units, currencies, compact, rounding options, `formatRange`, `formatToParts`),
  `Intl.PluralRules` (incl. `selectRange`), `Intl.DateTimeFormat` (`format`, `formatToParts`, `formatRange(ToParts)`,
  `resolvedOptions`, time zones, `hourCycle`, `dateStyle`/`timeStyle`, the calendars node supports),
  `Intl.RelativeTimeFormat`, `Intl.ListFormat`, `Intl.DisplayNames`, `Intl.Segmenter` (grapheme, word, sentence),
  `Intl.DurationFormat`; `supportedLocalesOf` on each. node 24.21.0's `Intl` members, exactly:
  `getCanonicalLocales, supportedValuesOf, DateTimeFormat, NumberFormat, Collator, PluralRules, RelativeTimeFormat,
  ListFormat, Locale, DisplayNames, Segmenter, DurationFormat`;
  `Symbol.toStringTag` "Intl" (`Object.prototype.toString.call(Intl)` is `[object Intl]`).
  Locale-sensitive methods: `String.prototype.localeCompare`, `toLocaleUpperCase`/`toLocaleLowerCase` (tr, az, lt),
  `normalize` (all four forms), `Number`/`BigInt`/`Date`/`Array.prototype.toLocaleString`,
  `Date.prototype.toLocaleDateString`/`toLocaleTimeString`.
- **Data, from node itself** (as `scripts/gen-unicode.mjs` does for the RegExp engine): CLDR data for locales,
  number/date/plural/list/relative-time/display-name patterns, collation (root UCA + tailorings), segmentation rules
  (UAX #29; CJK/Thai word breaking needs dictionaries: decide), normalization tables, IANA zone data (tzdata 2026c).
  Options: the `cldr-json` packages of CLDR 48 and the IANA tz sources, pinned in `TOOLS.md`, with a generator that
  checks its output against node's `Intl`; or probing node's `Intl` directly. Decide by size and exactness.
- **Size**: full ICU data is ~30 MB. Gate per program: always the default locale (`en-US`), plus locales named as
  string literals in the source and a build option (e.g. `--intl-locales <list|all>`); time zones likewise (`UTC` and
  `--intl-time-zones`). Record sizes per slice.
- **Default locale and time zone**: node takes them from the environment (`LANG`/`LC_*`, `TZ`). WASI gives the
  environment (`wasi:cli/environment`): honouring `TZ` with embedded zone data would make local time correct
  (F184) without `wasi:clocks/timezone`. Decide with `Date` (T26 assumes local = UTC; `scripts/diff.sh` runs node
  with `TZ=UTC`).
- **Reference values** (node 24.21.0, `TZ=UTC`, for 2026-10-04T07:12:44Z):
  `new Intl.DateTimeFormat().resolvedOptions()` → `{"locale":"en-US","calendar":"gregory","numberingSystem":"latn",
  "timeZone":"UTC","year":"numeric","month":"numeric","day":"numeric"}`;
  `new Intl.DateTimeFormat("en-US",{timeZone:"UTC",timeZoneName:"longOffset"}).format(d)` → `"10/4/2026, GMT+00:00"`
  (`Europe/Sofia`: `"10/4/2026, GMT+03:00"`); `Intl.DateTimeFormat().format(d)` → `"10/4/2026"`;
  zigbee-herdsman's `getTimeClusterAttributes()` → `{"time":844414474,"timeStatus":13,"timeZone":0,
  "dstStart":4294967295,"dstEnd":4294967295,"dstShift":0,…}` (time fields vary with the clock).
- **test262**: `test/intl402` (~3,000 files) isn't in the sparse checkout (`scripts/fetch-test262.sh`: add it) nor in
  `SUBSET` (`scripts/test262.mjs`); `built-ins/String/prototype/{localeCompare,normalize,toLocale*}` and the
  `toLocaleString` tests are in the subset now (check which pass today). Runs on `a-03`; ask the user first.
- **The zigbee2mqtt check** (one test of many): `tasks/T62-files` + `tasks/T71-files` build tree
  (`~/js2wasi3-prod-run/zp/patched`, outside the repo); a program calling
  `require("zigbee-herdsman").getTimeClusterAttributes()` under node and js2wasi3 (`TZ=UTC` and another zone) must
  agree, and a live run must survive a device's time read. Production runs need the user (they stop production);
  the test rig: `reference` memory (test coordinator, test topics only).

## Findings 2026-10-04 (session 1, before any code)
- **How to bind `Intl`**: like node's other JS globals: add `("Intl", "node:internal/intl", "Intl")` to
  `GLOBALS` in `crates/frontend/src/node.rs` and the module to its `MODULES` (`include_str!("../node/intl.js")`).
  The linker loads the module only where a program names `Intl`, so other programs keep their bytes (the gating the
  task asks for). Remove `Intl` from the "node's global … isn't available (planned: T21)" path (`lower/src/lib.rs`,
  `NODE_GLOBALS` / `lacks_global`). The locale-sensitive methods of other built-ins (`toLocaleString`, …) are in the
  prelude (`crates/lower/builtins/`): they need a separate way to reach the module (decide in slice 2/3).
- **What zigbee-herdsman needs** (`dist/utils/timeService.js`, `@date-fns/tz` 1.x in the zp tree):
  `Intl.DateTimeFormat().resolvedOptions().timeZone`; `tzScan(zone, {start, end})` → `tzOffset(zone, date)`, which
  caches `new Intl.DateTimeFormat("en-US", { timeZone, timeZoneName: "longOffset" }).format` **as an unbound
  function**: `format` must be the spec's getter returning a bound function (node: the same function each read;
  reading the getter on `Intl.DateTimeFormat.prototype` itself throws `TypeError: Method UnwrapDateTimeFormat called
  on incompatible receiver`); then `format(date).split("GMT")[1]`.
- **Default time zone**: js2wasi3's local time is UTC (T26, F184), so `resolvedOptions().timeZone` must be `"UTC"`
  (node with `TZ=UTC` gives that) to agree with `Date`. Honouring `TZ` is F184's decision, later.
- **node reference** (`TZ=UTC`, d = 2026-10-04T07:12:44Z, and 2026-01-15T00:00Z), `en-US`, `longOffset`:
  UTC `"10/4/2026, GMT+00:00"` / `"1/15/2026, GMT+00:00"`; Europe/Sofia `GMT+03:00` / `GMT+02:00`;
  America/New_York `"10/4/2026, GMT-04:00"` / `"1/14/2026, GMT-05:00"`; Asia/Kolkata `GMT+05:30`;
  Australia/Lord_Howe `GMT+11:00` both; Etc/GMT+5 `GMT-05:00`. `new Intl.DateTimeFormat().format(d)` →
  `"10/4/2026"`; `new Intl.DateTimeFormat("en-US").format !== another instance's .format`;
  `Intl.supportedValuesOf("timeZone").length` 418; `Object.getOwnPropertyNames(Intl)`: getCanonicalLocales,
  supportedValuesOf, DateTimeFormat, NumberFormat, Collator, PluralRules, RelativeTimeFormat, ListFormat, Locale,
  DisplayNames, Segmenter, DurationFormat.
- **Size of the task**: full ECMA-402 like node (every locale's CLDR patterns, ICU's skeleton matching, collation,
  segmentation) is weeks of work. Delivered in stages (all T77):
  - **Stage A** (first, fixes F330): the `Intl` global; BCP 47 parsing/canonicalization, `getCanonicalLocales`,
    `supportedValuesOf`; `DateTimeFormat` for `en-US` (`format`, `formatToParts`, `resolvedOptions`, the bound
    `format` getter, `timeZoneName` styles, `dateStyle`/`timeStyle`); **all 418 IANA zones** (+ aliases) with
    offsets and DST transitions **generated from node itself** (`scripts/gen-intl.mjs`: probe node's `Intl` per
    zone, e.g. daily from 1900 to 2100 then bisect to the second; check the output against node; future years by
    the zone's yearly rule; decide the data format by size: F246 warns about large string constants in a module).
    Unsupported locales/options throw a clear error, never wrong output.
  - **Stage B**: `NumberFormat`, `PluralRules`, `Number/BigInt/Date.prototype.toLocale*` for `en-US`.
  - **Stage C**: other locales from CLDR data (cldr-json 48, pinned in TOOLS.md), `Collator`/`localeCompare`/
    `normalize`, `RelativeTimeFormat`, `ListFormat`, `DisplayNames`, `DurationFormat`, `Segmenter`, test262 intl402.
- Also found: `node:process`'s named-export list (`crates/frontend/src/node.rs`, `exports("node:process")`) lacks
  `memoryUsage`/`cpuUsage` (added in F340): `import { memoryUsage } from "node:process"` would fail. Small fix.

## Plan (slices; each with fixtures vs node, `wasm-tools validate`, a wasmtime run, Progress entries)
1. **Infrastructure**: the `Intl` namespace (a global the linker binds when named; gated), BCP 47 parsing and
   canonicalization (aliases, likely subtags), locale negotiation (`lookup`/`best fit` as node resolves),
   `getCanonicalLocales`, `supportedValuesOf`, `Intl.Locale`, the data generator (`scripts/gen-intl.mjs`) and the
   per-program data gating; fix the stale "(planned: T21)" message.
2. **`NumberFormat` + `PluralRules`**, `Number`/`BigInt.prototype.toLocaleString` (F107's ≥ 1e21, F182's BigInt).
3. **`DateTimeFormat`** with zone data (offsets, DST transitions), `Date.prototype.toLocale*` (F182); the
   zigbee-herdsman time cluster check; the `TZ` decision (F184).
4. **`Collator`**, `localeCompare`, `normalize`, `toLocaleUpperCase`/`LowerCase` (F107).
5. **`RelativeTimeFormat`, `ListFormat`, `DisplayNames`, `DurationFormat`**, `Array.prototype.toLocaleString`.
6. **`Segmenter`** (grapheme/word/sentence; dictionary-based languages: decide).
7. **test262 `intl402`** in the checkout and `SUBSET`, full run on a-03 (ask first), baseline; docs (`keywords.md`
   rows, `design.md` D5 note, `tests/test262/HISTORY.md`, README), follow-ups.

## Acceptance
- [ ] Every `Intl` constructor and locale-sensitive method node 24.21.0 has, with node's results for the locales
      and time zones included in a build; programs without them keep their bytes.
- [ ] test262 `intl402` in the subset, numbers recorded, no regressions elsewhere.
- [ ] zigbee-herdsman's `getTimeClusterAttributes()` equals node's (UTC and one DST zone); F330 closed.
- [ ] F107, F182 resolved or narrowed; F184 decided; data sizes and the gating documented.

## Notes / Result
(to be written)

## Progress
- 2026-10-04: handoff written (after the zigbee2mqtt production runs of 2026-10-04 found the `Intl` crash, F330).
- 2026-10-04: started (user: Intl first, then a task to profile zigbee2mqtt's real workload). Order: slice 1
  (namespace, locales, generator, gating), then slice 3 (DateTimeFormat + time zones: the production crash, F330),
  then 2, 4-7, grouped as stages A-C (see Findings). T78 paused. Tools unchanged (check-tools.sh). No code yet:
  findings and node reference values recorded above; next session starts with Stage A (gen-intl.mjs first).
- After T77: a new task to profile zigbee2mqtt's real workload (user, 2026-10-04): record a production run's
  traffic, replay it offline under node and js2wasi3, profile where the ~12× CPU per event goes (F329, F334).
- 2026-10-04 (session 2): Stage A begun. Probed node: zone ids resolve to ICU's canonical (Asia/Kolkata → Asia/Calcutta, US/Eastern → America/New_York, utc/Etc/UTC/GMT/Zulu → UTC; offsets `+05:30`, `-0800` → `-08:00`); names change with the metazone over time (New York 1850: short `GMT-4:56:02`, generic `New York Time`); LMT offsets have seconds (Sofia `GMT+01:33:16`). Generator plan: sample offsets by `getTimezoneOffset` (TZ switched in-process, 0.08 µs/call) every 12 h 1800–2100, names monthly via `formatToParts` (3.3 µs), bisect each change to the second with `Intl`; store per zone up to its rule-stable window, project later years by the equivalent year (same leap, same Jan 1 weekday). Alias candidates: /usr/share/zoneinfo + tzdata.zi links, each checked by node.
- 2026-10-04: `scripts/gen-intl.mjs` written and run. `tz`: 445 resolved zones + 152 aliases, every zone checked
  against node at 248,403 instants (random 1779–2286 and ±1 s around each change); 483 KB (`intl_tz.js`; the
  per-period en names are most of it). Generic names change at *other* zones' transitions (ICU compares with the
  metazone's golden zone: "Eastern European Time (Egypt)"), so names are sampled at every zone's change instants;
  Asia/Gaza and Asia/Hebron have no repeating rule (Ramadan) and are stored to 2100, later years by the
  equivalent year (not node's beyond 2100 for those two). `locales`: `intl_en.js` (CLDR 48 en calendar data, 4 KB),
  `intl_meta.js` (948 locales node resolves to themselves, supportedValuesOf lists, CLDR aliases, 17 KB).
  Decision: port ICU 78's DateTimePatternGenerator (`intl_dtpg.js`, from dtptngen.cpp) and V8 13.6's
  JSDateTimeFormat option → skeleton → pattern steps, rather than a pattern table copied from node (date×time
  combos don't compose for ~1.3% of option sets, and a table would be ~90 KB per locale).
- 2026-10-04: `intl.js` (Intl, DateTimeFormat, locales, zones), `intl_dtpg.js` (the DTPG port) written; a node
  harness loads them as plain JS and compares with node's `Intl`. Findings: node 24.21.0's ICU 78.3 data is CLDR
  **48.2** (cldr-json 48.0.0 differs: en `Hv` is `HH'h' v` there, `HH v` in ICU 78.3): the generator now uses
  cldr-json 48.2.0. V8 prints zero offsets `GMT+0`/`GMT+00:00`; an offset zone `+00:00` takes the GMT zone's names;
  `format()` (and `toLocale*String`) turn U+202F into a space, `formatToParts` keeps it; from UTC
  9999-12-31T23:59 ICU has no metazone (specific names → GMT format, generic → location): generator records the
  names after it. Sample of 23,088 option sets (every 97th of all component combinations × 6 hour-cycle options
  × 4 zones × 7 zone-name styles): 0 differ from node. First wasmtime run of the fixture: only the U+202F and the
  intended "not supported yet" lines differed; compile 15 s (debug compiler), 1.4 MB component, run 1.9 s.
- 2026-10-04: more node findings: `-u-hc-` makes ICU build a style's time pattern with the DTPG from
  `jmmsszzzz`/`jmmssz`/`jmmss`/`jmm` (createInstanceNoStdPat, no hour-length option); `+00:00` is ICU's GMT zone
  (GMT metazone names only from 1970 to 9999-12-31T23:59); after 9999 Europe/Dublin keeps its own "Irish Standard
  Time" (names stored per offset); ICU's language aliases differ from CLDR's (`bh`, `tw` stay; `sgn-NO` → `nsl`):
  the generator stores node's results for CLDR's rule tags, the regions node picks for split territories
  (`hy-SU` → `hy-AM`, 208 rules) and -u-/-t- value aliases (335). `scripts/check-intl.mjs` (all parts, components
  every 97th): 23,088 + 19,681 + 4,335 + 43 checks, 0 differ; components every 5th (447,898 option sets): 0 differ.
- 2026-10-04: wired in (`node.rs` MODULES + GLOBALS `Intl`; the stale "(planned: T21)" dropped from the missing
  global message). `tests/fixtures/builtins/intl-datetimeformat.js` passes `scripts/diff.sh` (slow: 11 s with the
  debug compiler, F343); `wasm-tools validate` ok; zigbee check `tasks/T77-files/zigbee-time.cjs` (built in
  `~/js2wasi3-prod-run/zp/patched`): js2wasi3 + wasmtime output identical to node (F330 fixed offline; live run on
  the test rig still to do). Related fixtures (locale, date-basic, date-convert, globals/*) pass; cargo test
  frontend + lower pass. Size/compile: +1.3 MB, +2.75 s (release compiler) per program naming `Intl` (F343).
  Follow-ups F342-F346. **Stage A done except `formatRange` (F342).** Next: Stage B (`NumberFormat`, `PluralRules`,
  `Number`/`BigInt`/`Date.prototype.toLocale*` for en-US, F344), or the zigbee2mqtt profiling task first (user's
  order: Intl, then profiling; stage A covers the crash).

## Stage A result (2026-10-04)
- Files: `crates/frontend/node/intl.js` (Intl, locales, DateTimeFormat, zones, Date toLocale*), `intl_dtpg.js`
  (ICU DTPG port), generated `intl_en.js` (4 KB), `intl_meta.js` (25 KB), `intl_tz.js` (493 KB);
  `scripts/gen-intl.mjs` (`node scripts/gen-intl.mjs [tz] [locales]`; tz ~6 min, checks itself against node),
  `scripts/check-intl.mjs` (`TZ=UTC node scripts/check-intl.mjs [components[=step]] [styles] [locales] [api]`).
- Supported: `Intl.getCanonicalLocales`, `Intl.supportedValuesOf`, `Intl.DateTimeFormat` (constructor with/without
  `new`, `supportedLocalesOf`, `format` getter, `formatToParts`, `resolvedOptions`) for `en`/`en-US` (+ `-u-hc-`,
  `-u-ca-gregory`, `-u-nu-latn`), all zones; `Date.prototype.toLocale*String` where `Intl` is named.
- Unsupported (clear errors): other locales (RangeError "The locale 'de' isn't supported by js2wasi3 yet (T77)"),
  calendars, numbering systems, `formatRange*` (F342), the other constructors (TypeError).
