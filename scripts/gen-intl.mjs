#!/usr/bin/env node
// Generates the time zone data of `Intl` (T77) from the node that runs it (the oracle: node 24.21.0,
// ICU 78.3, tzdata 2026c) by probing node itself:
//   crates/frontend/node/intl_tz.js   every zone id node accepts (case-insensitive) → node's resolved id,
//                                     and per resolved zone its periods: start, UTC offset (seconds), and
//                                     its en-US names (short, long, shortGeneric, longGeneric)
// How: per zone, the offset is sampled every 12 hours from 1800 to 2101 (`getTimezoneOffset`, with
// `process.env.TZ` switched in-process) and the names monthly (`formatToParts`); each change is bisected
// to the second and the exact offset (with seconds, for LMT) read from `longOffset`. Only the years up to
// a zone's rule-stable window are stored: a later time maps to the same instant of the "equivalent year"
// (same leap-ness, same weekday of Jan 1) inside the 28-year window [base, base + 28), checked here
// against node until 2101. Zone ids: `Intl.supportedValuesOf("timeZone")`, the files of
// /usr/share/zoneinfo and the links of its tzdata.zi, each kept only if node accepts it.
// At the end every zone is checked against node at random instants and around each change.
// And the locale data, from the cldr-json packages of CLDR 48.2 (node 24.21.0 has ICU 78.3, whose data is CLDR 48.2; 48.0 differs,
// e.g. en `Hv`):
//   crates/frontend/node/intl_en.js   en's calendar data for the DateTimePatternGenerator port
//                                     (intl_dtpg.js) and the formatter
//   crates/frontend/node/intl_meta.js node's available locales (each locale node resolves to itself),
//                                     the lists of `Intl.supportedValuesOf`, CLDR's locale aliases
// Usage: node scripts/gen-intl.mjs [tz] [locales]   (default: both)
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const ZONEINFO = "/usr/share/zoneinfo";
const H = 3600_000;
const START = Date.UTC(1800, 0, 1);
const END = Date.UTC(2101, 0, 1);

// ---- zone ids ---------------------------------------------------------------------------------

function zoneinfoNames(dir, prefix = "") {
  const out = [];
  for (const f of readdirSync(dir)) {
    if (/^(posix|right|SECURITY|[a-z].*\.(tab|zi|list)|leap.*|tzdata.*|posixrules|localtime)$/.test(f)) continue;
    const p = join(dir, f);
    if (statSync(p).isDirectory()) out.push(...zoneinfoNames(p, prefix + f + "/"));
    else out.push(prefix + f);
  }
  return out;
}

function resolve(z) {
  try {
    return new Intl.DateTimeFormat("en-US", { timeZone: z }).resolvedOptions().timeZone;
  } catch {
    return null;
  }
}

const candidates = new Set(Intl.supportedValuesOf("timeZone"));
for (const z of zoneinfoNames(ZONEINFO)) candidates.add(z);
for (const l of readFileSync(join(ZONEINFO, "tzdata.zi"), "utf8").split("\n")) {
  const m = /^L (\S+) (\S+)/.exec(l);
  if (m) candidates.add(m[1]).add(m[2]);
}
for (const z of ["UTC", "GMT", "Etc/UTC", "Etc/GMT"]) candidates.add(z);
const aliases = new Map(); // lower-case id → resolved id
for (const z of [...candidates].sort()) {
  const r = resolve(z);
  if (r) aliases.set(z.toLowerCase(), r);
}
const resolved = [...new Set(aliases.values())].sort();
for (const r of resolved) {
  if (resolve(r) !== r) throw new Error(`${r} doesn't resolve to itself`);
  aliases.set(r.toLowerCase(), r);
}

// ---- probing ----------------------------------------------------------------------------------

const STYLES = ["short", "long", "shortGeneric", "longGeneric"];

function prober(zone) {
  process.env.TZ = zone;
  const fmts = STYLES.map((s) => new Intl.DateTimeFormat("en-US", { timeZone: zone, timeZoneName: s }));
  const exact = new Intl.DateTimeFormat("en-US", { timeZone: zone, timeZoneName: "longOffset" });
  const part = (f, t) => f.formatToParts(t).find((p) => p.type === "timeZoneName").value;
  return {
    zone,
    minutes: (t) => -new Date(t).getTimezoneOffset(),
    names: (t) => fmts.map((f) => part(f, t)).join("\n"),
    offset(t) {
      const s = part(exact, t);
      if (s === "GMT") return 0;
      const m = /^GMT([+-])(\d\d):(\d\d)(?::(\d\d))?$/.exec(s);
      if (!m) throw new Error(`${zone}: longOffset ${s}`);
      return (m[1] === "-" ? -1 : 1) * (+m[2] * 3600 + +m[3] * 60 + +(m[4] ?? 0));
    },
  };
}

// The first instant in (lo, hi] (whole seconds) where key differs from key(lo).
function bisect(key, lo, hi) {
  const k = key(lo);
  lo = Math.floor(lo / 1000);
  hi = Math.ceil(hi / 1000);
  while (hi - lo > 1) {
    const mid = Math.floor((lo + hi) / 2);
    if (key(mid * 1000) === k) lo = mid;
    else hi = mid;
  }
  return hi * 1000;
}

// The instants (ms) where a zone's offset or names change: offsets sampled every 12 hours, names monthly
// and at `extra` (every zone's changes: a generic name like "Eastern European Time (Egypt)" changes where
// the metazone's reference zone changes its offset, in gaps as short as 3 days).
function changes(p, extra = []) {
  const out = new Set();
  let prev = p.minutes(START);
  for (let t = START + 12 * H; t <= END; t += 12 * H) {
    const m = p.minutes(t);
    if (m !== prev) out.add(bisect(p.minutes, t - 12 * H, t));
    prev = m;
  }
  const MONTH = 30 * 24 * H;
  const samples = [];
  for (let t = START; t <= END; t += MONTH) samples.push(t);
  samples.push(...extra);
  samples.sort((a, b) => a - b);
  let pt = samples[0], pn = p.names(pt);
  for (const t of samples.slice(1)) {
    if (t === pt) continue;
    const n = p.names(t);
    if (n !== pn) out.add(bisect(p.names, pt, t));
    pt = t;
    pn = n;
  }
  // Seconds-only offset changes (LMT → a rounded offset) between minute samples.
  let po = p.offset(START);
  for (let t = START + MONTH; t <= Date.UTC(1990, 0, 1); t += MONTH) {
    const o = p.offset(t);
    if (o !== po) out.add(bisect(p.offset, t - MONTH, t));
    po = o;
  }
  return out;
}

// Periods [{start (ms; -Infinity for the first), offset (s), names}] of one zone.
function periods(p, cs) {
  const out = [{ start: -Infinity, offset: p.offset(START), names: p.names(START) }];
  for (const c of [...cs].sort((a, b) => a - b)) {
    const q = { start: c, offset: p.offset(c), names: p.names(c) };
    const last = out[out.length - 1];
    if (q.offset !== last.offset || q.names !== last.names) out.push(q);
  }
  return out;
}

// From UTC 9999-12-31T23:59 ICU has no metazone for a zone: its specific names become GMT formats
// (V8's: `GMT+0` for 0) and its generic names the location's ("Los Angeles Time").
const CUT = Date.UTC(9999, 11, 31, 23, 59);

function gmt(offset, long) {
  const a = Math.abs(offset), h = Math.floor(a / 3600), m = Math.floor((a % 3600) / 60), sec = a % 60;
  const p2 = (n) => String(n).padStart(2, "0");
  let r = "GMT" + (offset < 0 ? "-" : "+") + (long ? p2(h) : String(h));
  if (long || m || sec) r += ":" + p2(m);
  if (sec) r += ":" + p2(sec);
  return r;
}

// The names after CUT per offset (they follow DST: Europe/Dublin's own "Irish Standard Time"), with ""
// for a name in GMT format, as [[offset, names]], or null (they don't change at CUT).
function lateNames(p, ps) {
  const byOffset = new Map();
  let changes = false;
  for (let k = 0; k < 13; k++) {
    const t = CUT + k * 30 * 24 * H;
    const n = p.names(t).split("\n"), off = p.offset(t);
    if (n.join("\n") !== find(ps, t).names) changes = true;
    const late = [n[0] === gmt(off, false) ? "" : n[0], n[1] === gmt(off, true) ? "" : n[1], n[2], n[3]].join("\n");
    if (byOffset.has(off) && byOffset.get(off) !== late) throw new Error(`${p.zone}: names after 9999 vary`);
    byOffset.set(off, late);
  }
  return changes ? [...byOffset] : null;
}

// ---- the equivalent-year window ---------------------------------------------------------------

const yearStart = (y) => Date.UTC(y, 0, 1);
const isLeap = (y) => (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
const jan1 = (y) => new Date(yearStart(y)).getUTCDay();

// In the window, else (a window ending in 2100, which isn't a leap year) the 28 years before it.
function equivalent(y, base) {
  for (let e = base; e < base + 28; e++) if (isLeap(e) === isLeap(y) && jan1(e) === jan1(y)) return e;
  for (let e = base - 1; e >= base - 28; e--) if (isLeap(e) === isLeap(y) && jan1(e) === jan1(y)) return e;
  throw new Error(`no equivalent year for ${y} in ${base}`);
}

function find(ps, t) {
  let lo = 0, hi = ps.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (ps[mid].start <= t) lo = mid;
    else hi = mid - 1;
  }
  return ps[lo];
}

function mapped(t, base) {
  const y = new Date(t).getUTCFullYear();
  if (y < base + 28) return t;
  return t - yearStart(y) + yearStart(equivalent(y, base));
}

// A year's signature: the period at its start and each change inside it, relative to Jan 1.
function signature(ps, y) {
  const a = yearStart(y), b = yearStart(y + 1);
  const q = find(ps, a);
  let sig = `${q.offset} ${q.names}`;
  for (const p of ps) if (p.start > a && p.start < b) sig += `|${p.start - a} ${p.offset} ${p.names}`;
  return sig;
}

// The first base year whose window predicts every later year until 2100, or null (Africa/Casablanca:
// tzdata lists its Ramadan changes year by year until 2087).
function stableBase(ps) {
  const sig = new Map();
  for (let y = 1901; y < 2101; y++) sig.set(y, signature(ps, y));
  outer: for (let base = 1901; base <= 2072; base++) {
    for (let y = base + 28; y < 2101; y++) if (sig.get(y) !== sig.get(equivalent(y, base))) continue outer;
    return base;
  }
  return null;
}

// ---- encoding ---------------------------------------------------------------------------------

const strings = [];
const stringIdx = new Map();
function str(s) {
  if (!stringIdx.has(s)) {
    stringIdx.set(s, strings.length);
    strings.push(s);
  }
  return stringIdx.get(s);
}
const b36 = (n) => (n < 0 ? "-" + (-n).toString(36) : n.toString(36));

// Distinct name tuples (short, long, shortGeneric, longGeneric), as `.`-joined indexes into `strings`.
const tuples = [];
const tupleIdx = new Map();
function tuple(names) {
  if (!tupleIdx.has(names)) {
    tupleIdx.set(names, tuples.length);
    tuples.push(names.split("\n").map((s) => b36(str(s))).join("."));
  }
  return tupleIdx.get(names);
}
// Seconds as base-36 minutes, or `s` and base-36 seconds when they aren't whole minutes.
const secs = (n) => (n % 60 === 0 ? b36(n / 60) : "s" + b36(n));

// A zone: `base` (and `|offset:tuple:…`: the names from 9999-12-31T23:59 per offset, see CUT) then `;`-separated
// periods `delta,offset,tuple`: the delta is from the previous period's
// start (the second period's from 1970; the first period's start is -Infinity and has no delta).
function encode(ps, base, late) {
  const keep = ps.filter((q) => q.start < yearStart(base + 28));
  let prev = 0;
  const items = keep.map((q, i) => {
    const d = i === 0 ? "" : secs((q.start - prev) / 1000);
    if (i > 0) prev = q.start;
    return `${d},${secs(q.offset)},${b36(tuple(q.names))}`;
  });
  const l = late === null ? "" : "|" + late.map(([off, names]) => `${secs(off)}:${b36(tuple(names))}`).join(":");
  return `${b36(base)}${l};${items.join(";")}`;
}

// ---- locale data (CLDR) -----------------------------------------------------------------------

const CLDR = "https://raw.githubusercontent.com/unicode-org/cldr-json/48.2.0/cldr-json/";

async function cldr(path) {
  const res = await fetch(CLDR + path);
  if (!res.ok) throw new Error(`${CLDR}${path}: ${res.status}`);
  return res.json();
}

const DAYS = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

async function localeData(locale) {
  const g = (await cldr(`cldr-dates-full/main/${locale}/ca-gregorian.json`)).main[locale].dates.calendars.gregorian;
  const fields = (await cldr(`cldr-dates-full/main/${locale}/dateFields.json`)).main[locale].dates.fields;
  const numbers = (await cldr(`cldr-numbers-full/main/${locale}/numbers.json`)).main[locale].numbers;
  const timeData = (await cldr("cldr-core/supplemental/timeData.json")).supplemental.timeData;
  const rules = (await cldr("cldr-core/supplemental/dayPeriods.json")).supplemental.dayPeriodRuleSet;
  const months = (ctx) => Object.fromEntries(["abbreviated", "narrow", "wide"].map((w) => [w, Array.from({ length: 12 }, (_, i) => g.months[ctx][w][i + 1])]));
  const days = (ctx) => Object.fromEntries(["abbreviated", "narrow", "short", "wide"].map((w) => [w, DAYS.map((d) => g.days[ctx][w][d])]));
  const periods = (ctx) => Object.fromEntries(["abbreviated", "narrow", "wide"].map((w) => [w, Object.fromEntries(Object.entries(g.dayPeriods[ctx][w]).filter(([k]) => !k.includes("-alt-")))]));
  const plain = (o) => Object.entries(o).filter(([k]) => !k.includes("-alt-") && !k.includes("-count-"));
  const region = { en: "US" }[locale];
  const hours = timeData[`${locale}_${region}`] ?? timeData[region] ?? timeData["001"];
  const dp = rules[locale] ?? rules.root;
  const hhmm = (s) => +s.slice(0, 2) * 60 + +s.slice(3);
  return {
    months: { format: months("format"), standalone: months("stand-alone") },
    days: { format: days("format"), standalone: days("stand-alone") },
    dayPeriods: { format: periods("format"), standalone: periods("stand-alone") },
    // [name, from, before] in minutes, or [name, at] for midnight/noon.
    dayPeriodRules: Object.entries(dp).map(([k, v]) => (v._at ? [k, hhmm(v._at)] : [k, hhmm(v._from), hhmm(v._before)])),
    eras: { abbr: [g.eras.eraAbbr[0], g.eras.eraAbbr[1]], names: [g.eras.eraNames[0], g.eras.eraNames[1]], narrow: [g.eras.eraNarrow[0], g.eras.eraNarrow[1]] },
    dateFormats: ["full", "long", "medium", "short"].map((k) => g.dateFormats[k]),
    timeFormats: ["full", "long", "medium", "short"].map((k) => g.timeFormats[k]),
    dateTimeFormats: ["full", "long", "medium", "short"].map((k) => g["dateTimeFormats-atTime"].standard[k]),
    // ICU adds availableFormats in its resource order (keys sorted by code unit).
    availableFormats: plain(g.dateTimeFormats.availableFormats).sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    appendItems: g.dateTimeFormats.appendItems,
    fieldNames: Object.fromEntries(Object.entries(fields).filter(([k]) => !k.includes("-")).map(([k, v]) => [k, v.displayName])),
    hours: { preferred: hours._preferred, allowed: hours._allowed.split(" ") },
    decimal: numbers["symbols-numberSystem-latn"].decimal,
  };
}

async function genLocales() {
  for (const locale of ["en"]) {
    const d = await localeData(locale);
    const out = `// CLDR 48.2's calendar data of \`${locale}\` for \`Intl\` (T77), generated by scripts/gen-intl.mjs: don't edit by hand.
'use strict';

module.exports = {
${Object.entries(d).map(([k, v]) => `  ${k}: ${JSON.stringify(v)},`).join("\n")}
};
`;
    writeFileSync(join(ROOT, `crates/frontend/node/intl_${locale}.js`), out);
    process.stderr.write(`intl_${locale}.js: ${out.length} bytes\n`);
  }
}

async function genMeta() {
  const tags = (await cldr("cldr-core/availableLocales.json")).availableLocales.full;
  // ICU also has locales CLDR doesn't list (empty aliases like en_US, de_DE): try every language
  // (and language-script) of the list with every region.
  const cands = new Set();
  const regions = new Set();
  for (const t of tags) {
    const p = t.split("-");
    for (let i = 1; i <= p.length; i++) cands.add(p.slice(0, i).join("-"));
    for (const x of p) if (/^([A-Z]{2}|\d{3})$/.test(x)) regions.add(x);
  }
  for (const r of Object.keys((await cldr("cldr-core/supplemental/territoryInfo.json")).supplemental.territoryInfo)) regions.add(r);
  for (const c of [...cands]) {
    if (/^[a-z]{2,3}(-[A-Z][a-z]{3})?$/.test(c)) for (const r of regions) cands.add(`${c}-${r}`);
  }
  const available = [...cands].filter((t) => new Intl.DateTimeFormat(t).resolvedOptions().locale === t).sort();
  const a = (await cldr("cldr-core/supplemental/aliases.json")).supplemental.metadata.alias;
  const canon = (t) => {
    try {
      return Intl.getCanonicalLocales(t)[0];
    } catch {
      return null;
    }
  };
  const list = (o) => Object.entries(o).map(([k, v]) => `${k.replace(/_/g, "-")}=${v._replacement.replace(/_/g, "-")}`).join(";");
  // Language rules as node (ICU) applies them: the replacement is node's canonical form of the rule's tag
  // (ICU's data differs from CLDR's here and there: \`bh\` and \`tw\` stay, \`sgn-NO\` is \`nsl\`).
  const languageAlias = [];
  for (const k of Object.keys(a.languageAlias)) {
    const tag = k.replace(/_/g, "-");
    const r = canon(tag);
    if (r !== null && r !== tag) languageAlias.push(`${tag}=${r}`);
  }
  // A territory that became several regions: the one node picks for a language other than the first
  // (from the language's likely subtags).
  const likely = (await cldr("cldr-core/supplemental/likelySubtags.json")).supplemental.likelySubtags;
  const langs = new Set(Object.keys(likely).map((k) => k.split("-")[0]).filter((l) => /^[a-z]{2,3}$/.test(l)));
  const territoryRegion = [];
  for (const [k, v] of Object.entries(a.territoryAlias)) {
    const regions = v._replacement.split(" ");
    if (regions.length < 2) continue;
    for (const l of langs) {
      const r = canon(`${l}-${k}`);
      const region = r && r.split("-").find((x) => /^[A-Z]{2}$|^\d{3}$/.test(x));
      if (region && region !== regions[0]) territoryRegion.push(`${l}-${k}=${region}`);
    }
  }
  // -u-/-t- values with an alias (cldr-bcp47), as node canonicalizes them.
  const valueAlias = new Set();
  for (const f of ["calendar", "collation", "measure", "timezone", "transform", "number"]) {
    const j = await cldr(`cldr-bcp47/bcp47/${f}.json`);
    for (const [ext, keys] of Object.entries(j.keyword)) {
      for (const [key, types] of Object.entries(keys)) {
        for (const [type, info] of Object.entries(types)) {
          if (type.startsWith("_") || typeof info !== "object") continue;
          const cands = [type, ...(info._alias ?? "").split(" ").filter((x) => /^[a-z0-9-]+$/.test(x))];
          for (const c of cands) {
            const tag = ext === "u" ? `en-u-${key}-${c}` : `en-t-${key}-${c}`;
            const r = canon(tag);
            if (r !== null && r !== tag) valueAlias.add(`${ext}-${key}-${c}=${r.slice(5)}`);
          }
        }
      }
    }
  }
  for (const [k] of Object.entries(a.subdivisionAlias)) {
    for (const key of ["rg", "sd"]) {
      const tag = `en-u-${key}-${k}`;
      const r = canon(tag);
      if (r !== null && r !== tag) valueAlias.add(`u-${key}-${k}=${r.slice(5)}`);
    }
  }
  const values = Object.fromEntries(["calendar", "collation", "currency", "numberingSystem", "unit"].map((k) => [k, Intl.supportedValuesOf(k).join(" ")]));
  const out = `// node ${process.version}'s locale metadata for \`Intl\` (T77), generated by scripts/gen-intl.mjs: don't edit by hand.
'use strict';

// The locales node's \`Intl.DateTimeFormat\` resolves to themselves (ICU's available locales).
exports.available = ${JSON.stringify(available.join(" "))};
// \`Intl.supportedValuesOf\` (the time zones are in intl_tz.js).
exports.values = ${JSON.stringify(values)};
// CLDR 48.2's aliases (\`from=to;…\`, \`-\` separated subtags; a territory may map to several regions,
// space separated). Language rules carry node's result.
exports.languageAlias = ${JSON.stringify(languageAlias.join(";"))};
exports.scriptAlias = ${JSON.stringify(list(a.scriptAlias))};
exports.territoryAlias = ${JSON.stringify(list(a.territoryAlias))};
exports.variantAlias = ${JSON.stringify(list(a.variantAlias))};
// \`lang-territory=region\` where node takes another region than the first of a territory alias.
exports.territoryRegion = ${JSON.stringify(territoryRegion.join(";"))};
// -u-/-t- values: \`u-key-value=\` node's canonical extension (\`u-ca-islamicc=u-ca-islamic-civil\`).
exports.valueAlias = ${JSON.stringify([...valueAlias].join(";"))};
`;
  writeFileSync(join(ROOT, "crates/frontend/node/intl_meta.js"), out);
  process.stderr.write(`intl_meta.js: ${available.length} available locales, ${out.length} bytes\n`);
}

const parts = process.argv.slice(2);
const want = (p) => parts.length === 0 || parts.includes(p);
if (want("locales")) {
  await genLocales();
  await genMeta();
}
if (want("tz")) genTz();

// ---- main (time zones) ------------------------------------------------------------------------

function genTz() {

const t0 = performance.now();
const data = new Map(); // resolved id → encoded
const inexact = []; // zones whose years after 2100 aren't predicted
const global = new Set();
for (const z of resolved) for (const c of changes(prober(z))) global.add(c);
const extra = [...global];
const all = new Map();
for (const z of resolved) {
  const probe = prober(z);
  const ps = periods(probe, changes(probe, extra));
  let base = stableBase(ps);
  if (base === null) {
    base = 2073; // keeps every year until 2100
    inexact.push(z);
  }
  const late = lateNames(probe, ps);
  all.set(z, { ps, base, probe, late });
  data.set(z, encode(ps, base, late));
}
process.stderr.write(`probed ${resolved.length} zones in ${((performance.now() - t0) / 1000).toFixed(1)} s\n`);

// Check against node.
let checks = 0, seed = 1;
const rand = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
for (const [z, { ps, base, probe, late }] of all) {
  process.env.TZ = z;
  const ts = [];
  for (let i = 0; i < 300; i++) {
    const t = Math.floor(-6e12 + rand() * 1.6e13);
    if (t < END || !inexact.includes(z)) ts.push(t);
  }
  for (let i = 0; i < 40; i++) ts.push(Math.floor((rand() * 2 - 1) * 8.64e15));
  ts.push(-8.64e15, 8.64e15, CUT - 1, CUT);
  for (const q of ps) if (q.start !== -Infinity) ts.push(q.start - 1000, q.start);
  for (const t of ts) {
    if (inexact.includes(z) && t >= END) continue;
    const q0 = find(ps, mapped(t, base));
    let names = q0.names;
    if (t >= CUT && late !== null) {
      const l = new Map(late).get(q0.offset).split("\n");
      names = [l[0] || gmt(q0.offset, false), l[1] || gmt(q0.offset, true), l[2], l[3]].join("\n");
    }
    const q = { offset: q0.offset, names };
    const want = { offset: probe.offset(t), names: probe.names(t) };
    if (q.offset !== want.offset || q.names !== want.names) {
      throw new Error(`${z} at ${new Date(t).toISOString()}: ${q.offset} ${q.names} vs node ${want.offset} ${want.names}`);
    }
    checks++;
  }
}
process.stderr.write(`checked ${checks} instants; not predicted after 2100: ${inexact.join(", ") || "none"}\n`);

// Zones with the same data share one entry.
const entries = [];
const entryIdx = new Map();
const zoneEntry = new Map();
for (const [z, d] of data) {
  if (!entryIdx.has(d)) {
    entryIdx.set(d, entries.length);
    entries.push(d);
  }
  zoneEntry.set(z, entryIdx.get(d));
}
const ids = resolved;
const aliasList = [...aliases].filter(([l, r]) => l !== r.toLowerCase()).map(([l, r]) => `${l}=${ids.indexOf(r)}`);

const out = `// node ${process.version}'s time zones (ICU ${process.versions.icu}, tzdata ${process.versions.tz}) for \`Intl\` (T77),
// generated by scripts/gen-intl.mjs: don't edit by hand.
'use strict';

// Resolved zone ids (node's \`resolvedOptions().timeZone\`), each with its entry in \`zones\`.
exports.ids = ${JSON.stringify(ids.map((z) => `${z}=${zoneEntry.get(z)}`).join(" "))};
// Other ids node accepts, lower-case: \`alias=index into ids\`.
exports.aliases = ${JSON.stringify(aliasList.join(" "))};
// The canonical ids of \`Intl.supportedValuesOf("timeZone")\`.
exports.supported = ${JSON.stringify(Intl.supportedValuesOf("timeZone").join(" "))};
// en-US time zone names, and the tuples (short.long.shortGeneric.longGeneric, base 36) indexed from \`zones\`.
exports.names = ${JSON.stringify(strings.join("\n"))};
exports.tuples = ${JSON.stringify(tuples.join(" "))};
// Per entry: the base year of the 28-year window (and \`|offset:tuple:…\`: the names from 9999-12-31T23:59
// per offset, "" for a GMT format), then
// periods \`delta,offset,tuple\` (base 36; times
// in minutes, or seconds after an \`s\`).
exports.zones = [
${entries.map((e) => `  ${JSON.stringify(e)},`).join("\n")}
];
`;
writeFileSync(join(ROOT, "crates/frontend/node/intl_tz.js"), out);
process.stderr.write(`${ids.length} zones, ${aliasList.length} aliases, ${entries.length} entries, ${strings.length} names, ${tuples.length} tuples, ${out.length} bytes\n`);
}
