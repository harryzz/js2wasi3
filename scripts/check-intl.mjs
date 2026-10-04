#!/usr/bin/env node
// Checks js2wasi3's `Intl` (crates/frontend/node/intl*.js, T77) against the node that runs it: the modules
// are plain JS, loaded here with js2wasi3's module names, and every result is compared with node's `Intl`.
//   components  every DateTimeFormat component option combination (V8's rows) × hour-cycle options × zones ×
//               zone-name styles, at a few instants: resolvedOptions and formatToParts (every <step>-th one)
//   styles      dateStyle × timeStyle × hour cycles × locales (with extensions) × zones, incl. ±8.64e15
//   locales     getCanonicalLocales over CLDR's alias keys and odd tags
//   api         errors, receivers, property shapes
// Usage: TZ=UTC node scripts/check-intl.mjs [components[=step]] [styles] [locales] [api]   (default: all, step 97)
// Prints the first differences and a count per part; exit status 1 if anything differs.
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const DIR = join(dirname(fileURLToPath(import.meta.url)), "../crates/frontend/node");
const cache = new Map();
function load(name) {
  if (cache.has(name)) return cache.get(name).exports;
  const module = { exports: {} };
  cache.set(name, module);
  const src = readFileSync(join(DIR, name.replace(/^internal\//, "") + ".js"), "utf8");
  new Function("require", "module", "exports", src)(load, module, module.exports);
  return module.exports;
}
// intl.js replaces Date.prototype.toLocale*String: keep node's for the comparisons.
const saved = ["toLocaleString", "toLocaleDateString", "toLocaleTimeString"].map((n) => [n, Date.prototype[n]]);
const J = load("internal/intl").Intl;
const ours = Object.fromEntries(saved.map(([n]) => [n, Date.prototype[n]]));
for (const [n, f] of saved) Object.defineProperty(Date.prototype, n, { value: f, writable: true, configurable: true });
const meta = load("internal/intl_meta");

const show = (f) => {
  try {
    return JSON.stringify(f());
  } catch (e) {
    return `${e.constructor.name}: ${e.message}`;
  }
};
let failed = false;
function report(part, n, bad, firsts) {
  for (const f of firsts) console.log(f);
  console.log(`${part}: ${n} checked, ${bad} differ`);
  if (bad) failed = true;
}
function diffAt(a, b) {
  let i = 0;
  while (a[i] === b[i]) i++;
  return `\n  node ${a.slice(Math.max(0, i - 60), i + 100)}\n  ours ${b.slice(Math.max(0, i - 60), i + 100)}`;
}

function* combos(spec) {
  const keys = Object.keys(spec);
  function* rec(i, o) {
    if (i === keys.length) {
      yield o;
      return;
    }
    for (const v of spec[keys[i]]) yield* rec(i + 1, v === undefined ? o : { ...o, [keys[i]]: v });
  }
  yield* rec(0, {});
}

function components(step) {
  const D = [Date.UTC(2026, 0, 5, 15, 4, 6, 789), Date.UTC(2026, 9, 4, 0, 0, 0, 5), Date.UTC(2026, 9, 4, 12, 0, 0), Date.UTC(-50, 2, 3, 23, 59, 59, 999)];
  const dates = [...combos({ weekday: [undefined, "narrow", "short", "long"], era: [undefined, "narrow", "short", "long"],
    year: [undefined, "numeric", "2-digit"], month: [undefined, "numeric", "2-digit", "narrow", "short", "long"], day: [undefined, "numeric", "2-digit"] })];
  const times = [...combos({ hour: [undefined, "numeric", "2-digit"], minute: [undefined, "numeric", "2-digit"], second: [undefined, "numeric", "2-digit"],
    fractionalSecondDigits: [undefined, 1, 2, 3], dayPeriod: [undefined, "narrow", "short", "long"] })];
  const H = [{}, { hourCycle: "h11" }, { hourCycle: "h23" }, { hour12: true }, { hour12: false }, { hourCycle: "h24" }];
  const Z = [undefined, "short", "long", "shortOffset", "longOffset", "shortGeneric", "longGeneric"];
  const zones = ["UTC", "America/New_York", "Asia/Kolkata", "Europe/Sofia"];
  let n = 0, bad = 0, i = 0;
  const firsts = [];
  for (const d of dates) for (const t of times) for (const h of H) {
    if (i++ % step) continue;
    const z = Z[i % Z.length];
    const o = { timeZone: zones[i % 4], ...d, ...t, ...h, ...(z ? { timeZoneName: z } : {}) };
    n++;
    const a = new Intl.DateTimeFormat("en-US", o), b = new J.DateTimeFormat("en-US", o);
    const x = JSON.stringify([a.resolvedOptions(), D.map((t) => a.formatToParts(t))]);
    const y = JSON.stringify([b.resolvedOptions(), D.map((t) => b.formatToParts(t))]);
    if (x !== y && bad++ < 8) firsts.push(JSON.stringify(o) + diffAt(x, y));
  }
  report("components", n, bad, firsts);
}

function styles() {
  const D = [Date.UTC(2026, 0, 5, 15, 4, 6, 789), Date.UTC(2026, 9, 4, 0, 0, 0, 5), Date.UTC(1, 0, 1), -8.64e15, 8.64e15];
  const S = [undefined, "full", "long", "medium", "short"];
  const H = [{}, { hourCycle: "h11" }, { hourCycle: "h23" }, { hour12: true }, { hour12: false }, { hourCycle: "h24" }, { hour12: false, hourCycle: "h11" }];
  const L = [undefined, "en", "en-US", "en-US-u-hc-h23", "en-u-hc-h11", "en-u-ca-gregory", "en-US-u-nu-latn", "en-u-hc-h24-ca-gregory", "xx",
    "en-ZZ", "und", [], "en-US-x-priv", "en-u-co-phonebk"];
  const zones = ["UTC", "America/Los_Angeles", "+05:30", "-00:00", "Asia/Kathmandu", "Australia/Lord_Howe", "Pacific/Chatham", "Europe/Dublin"];
  let n = 0, bad = 0;
  const firsts = [];
  for (const l of L) for (const ds of S) for (const ts of S) for (const h of H) for (const tz of zones) {
    const o = { timeZone: tz, ...h, ...(ds ? { dateStyle: ds } : {}), ...(ts ? { timeStyle: ts } : {}) };
    n++;
    const a = show(() => { const f = new Intl.DateTimeFormat(l, o); return [f.resolvedOptions(), D.map((d) => f.formatToParts(d)), D.map((d) => f.format(d))]; });
    const b = show(() => { const f = new J.DateTimeFormat(l, o); return [f.resolvedOptions(), D.map((d) => f.formatToParts(d)), D.map((d) => f.format(d))]; });
    if (a !== b && bad++ < 8) firsts.push(JSON.stringify(l) + " " + JSON.stringify(o) + diffAt(a, b));
  }
  // Date.prototype.toLocale*String (ToDateTimeOptions' required/defaults).
  for (const name of ["toLocaleString", "toLocaleDateString", "toLocaleTimeString"]) {
    for (const o of [undefined, { timeZone: "Asia/Tokyo" }, { hour: "numeric" }, { year: "numeric" }, { dateStyle: "short" }, { timeStyle: "short" },
      { weekday: "long", timeZone: "America/New_York" }, { dayPeriod: "short" }, { fractionalSecondDigits: 2 }]) {
      for (const t of [0, Date.UTC(2026, 9, 4, 7, 12, 44), NaN]) {
        n++;
        const a = show(() => Date.prototype[name].call(new Date(t), "en-US", o));
        const b = show(() => ours[name].call(new Date(t), "en-US", o));
        if (a !== b && bad++ < 8) firsts.push(`${name} ${JSON.stringify(o)} ${t}` + diffAt(a, b));
      }
    }
  }
  report("styles", n, bad, firsts);
}

function locales() {
  const tags = [];
  for (const e of meta.languageAlias.split(";")) {
    const k = e.split("=")[0];
    tags.push(k, k + "-US", k + "-u-ca-gregory");
  }
  for (const e of meta.territoryAlias.split(";")) {
    const k = e.split("=")[0];
    tags.push("en-" + k, "ru-" + k, "hy-" + k, "und-" + k);
  }
  for (const e of meta.scriptAlias.split(";")) tags.push("en-" + e.split("=")[0]);
  for (const e of meta.variantAlias.split(";")) tags.push("en-" + e.split("=")[0]);
  for (const e of meta.valueAlias.split(";")) {
    const [ext, ...rest] = e.split("=")[0].split("-");
    tags.push(`en-${ext}-${rest.join("-")}`);
  }
  tags.push("en-u-kn-true-kb-yes", "EN-a-AAA-b-bbb-x-Y", "en-US-u-VA-POSIX", "de-DE-1901-1996", "x-abc", "en-x", "en--US", "root", "en-a",
    "en-u", "en-t-zz", "abcd", "en-12345", "en-1234", "art-lojban", "ja-Latn-hepburn-heploc", "en-u-foo-bar-ca-gregory", "en-US-POSIX",
    "i-klingon", "en_US", "", "zh-hant-tw", "sgn-GR", "en-u-attr-ca-gregory-ca-buddhist");
  let bad = 0;
  const firsts = [];
  for (const t of tags) {
    const a = show(() => Intl.getCanonicalLocales(t)), b = show(() => J.getCanonicalLocales(t));
    if (a !== b && bad++ < 12) firsts.push(`${t}: node ${a} ours ${b}`);
  }
  report("locales", tags.length, bad, firsts);
}

function api() {
  const checks = [
    () => new Intl.DateTimeFormat("en", { weekday: "longer" }), () => new Intl.DateTimeFormat("en", { timeZone: "Mars/Base" }),
    () => new Intl.DateTimeFormat("en", { dateStyle: "full", hour: "numeric" }), () => new Intl.DateTimeFormat("en", null),
    () => new Intl.DateTimeFormat("en", { fractionalSecondDigits: 4 }), () => new Intl.DateTimeFormat("en", { fractionalSecondDigits: 0.5 }).resolvedOptions(),
    () => new Intl.DateTimeFormat("en", { calendar: "x" }), () => new Intl.DateTimeFormat("en_US"), () => new Intl.DateTimeFormat([5]),
    () => new Intl.DateTimeFormat().format(NaN), () => new Intl.DateTimeFormat().format(8.64e15 + 1), () => new Intl.DateTimeFormat("en", "abc").resolvedOptions(),
    () => new Intl.DateTimeFormat("en", { localeMatcher: "x" }), () => new Intl.DateTimeFormat("en", { hourCycle: "h25" }),
    () => Intl.DateTimeFormat.prototype.formatToParts.call({}), () => Intl.DateTimeFormat.prototype.resolvedOptions.call(1),
    () => Object.getOwnPropertyDescriptor(Intl.DateTimeFormat.prototype, "format").get.call({}),
    () => Intl.getCanonicalLocales(null), () => Intl.getCanonicalLocales([null]), () => Intl.getCanonicalLocales(5), () => Intl.getCanonicalLocales({ length: 1, 0: "EN" }),
    () => Intl.DateTimeFormat.supportedLocalesOf(["en-US", "en", "en-ZZ", "en-GB", "xx", "en-Latn-US", "en-US-u-ca-buddhist"]),
    () => Intl.DateTimeFormat.supportedLocalesOf("en", { localeMatcher: "x" }),
    () => Intl.supportedValuesOf("calendar"), () => Intl.supportedValuesOf("timeZone"), () => Intl.supportedValuesOf("x"),
    () => Intl.supportedValuesOf("currency"), () => Intl.supportedValuesOf("unit"), () => Intl.supportedValuesOf("numberingSystem"),
    () => new Intl.DateTimeFormat("en", { timeZone: "asia/calcutta" }).resolvedOptions().timeZone,
    () => ["+05", "+0530", "+05:30:00", "-23:59", "+24:00", "+05:3", "Etc/GMT+5", "etc/utc", "GMT", "US/Pacific"].map((tz) => new Intl.DateTimeFormat("en", { timeZone: tz }).resolvedOptions().timeZone),
    () => Intl.DateTimeFormat().format(new Date(0)), () => new Intl.DateTimeFormat("en", { timeZone: "UTC" }).format("2026-01-01"),
    () => Object.prototype.toString.call(Intl), () => Object.prototype.toString.call(new Intl.DateTimeFormat()), () => Object.getOwnPropertyNames(Intl),
    () => Object.getOwnPropertyNames(Intl.DateTimeFormat.prototype), () => [Intl.DateTimeFormat.length, Intl.DateTimeFormat.name, Intl.DateTimeFormat.supportedLocalesOf.length],
    () => { const f = new Intl.DateTimeFormat(); return [f.format === f.format, f.format.name, f.format.length, typeof f.format]; },
    () => Object.getOwnPropertyDescriptor(Intl, "DateTimeFormat"), () => Object.getOwnPropertyDescriptor(Intl.DateTimeFormat, "prototype"),
    () => Intl.DateTimeFormat() instanceof Intl.DateTimeFormat, () => Object.getPrototypeOf(new Intl.DateTimeFormat()) === Intl.DateTimeFormat.prototype,
  ];
  let bad = 0;
  const firsts = [];
  for (const f of checks) {
    const g = new Function("Intl", `return (${f})()`);
    const a = show(() => g(Intl)), b = show(() => g(J));
    if (a !== b && bad++ < 12) firsts.push(String(f).slice(6, 140) + diffAt(a, b));
  }
  report("api", checks.length, bad, firsts);
}

const args = process.argv.slice(2);
const want = (p) => args.length === 0 || args.some((a) => a.split("=")[0] === p);
if (want("components")) components(+(args.find((a) => a.startsWith("components="))?.split("=")[1] ?? 97));
if (want("styles")) styles();
if (want("locales")) locales();
if (want("api")) api();
process.exit(failed ? 1 : 0);
