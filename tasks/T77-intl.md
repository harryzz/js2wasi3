# T77: `Intl` (ECMA-402) and the locale-sensitive built-ins

Status: todo (handoff written 2026-10-04) | Depends on: T21 (prelude), T26 (`Date`), T27 (Unicode data generation), T40 (BigInt)

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
