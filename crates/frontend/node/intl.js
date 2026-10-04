// node:internal/intl (T77): the `Intl` global (ECMA-402), bound by the linker where a program names `Intl`
// (`node::GLOBALS`), so other programs keep their bytes. Follows V8 13.6 (node 24.21.0) step by step:
// option reading order, error messages, and ICU 78's patterns (the DateTimePatternGenerator port in
// intl_dtpg.js, CLDR 48 data in intl_<locale>.js, zone data probed from node in intl_tz.js).
// Stage A: locales `en`/`en-US`, the Gregorian calendar, `latn` digits; `DateTimeFormat` without
// `formatRange`. Anything else node would format differently throws a RangeError/TypeError naming
// js2wasi3, never a wrong result. Plain JS: scripts/gen-intl.mjs runs it under node against node's `Intl`.
'use strict';

const tzData = require('internal/intl_tz');
const meta = require('internal/intl_meta');
const { Generator, patternSkeleton, MATCH_HOUR_FIELD_LENGTH } = require('internal/intl_dtpg');

const LOCALE_DATA = { en: () => require('internal/intl_en') };
const DEFAULT_LOCALE = 'en-US';
// js2wasi3's local time is UTC (T26, F184).
const DEFAULT_TIME_ZONE = 'UTC';

function unsupported(what) {
  return new RangeError(`${what} isn't supported by js2wasi3 yet (T77)`);
}

// ---- options ------------------------------------------------------------------------------------

function coerceOptions(options, service) {
  if (options === undefined) return Object.create(null);
  if (options === null) throw new TypeError(`${service} called on null or undefined`);
  return Object(options);
}

function toObject(v) {
  if (v === undefined || v === null) throw new TypeError('Cannot convert undefined or null to object');
  return Object(v);
}

function getOption(options, property, type, values, fallback, service) {
  let value = options[property];
  if (value === undefined) return fallback;
  if (type === 'boolean') return Boolean(value);
  value = String(value);
  if (values !== undefined && !values.includes(value)) {
    throw new RangeError(`Value ${value} out of range for ${service} options property ${property}`);
  }
  return value;
}

function getNumberOption(options, property, min, max, fallback) {
  const value = options[property];
  if (value === undefined) return fallback;
  const n = Number(value);
  if (Number.isNaN(n) || n < min || n > max) throw new RangeError(`${property} value is out of range.`);
  return Math.floor(n);
}

// ---- language tags (UTS 35 unicode_locale_id, ECMA-402 6.2) --------------------------------------

const ALPHA = /^[a-z]+$/, DIGIT = /^[0-9]$/, ALNUM = /^[a-z0-9]+$/;
const isAlpha = (s, min, max) => s.length >= min && s.length <= max && ALPHA.test(s);
const isAlnum = (s, min, max) => s.length >= min && s.length <= max && ALNUM.test(s);
const isVariant = (s) => isAlnum(s, 5, 8) || (s.length === 4 && DIGIT.test(s[0]) && isAlnum(s, 4, 4));
const isRegion = (s) => isAlpha(s, 2, 2) || (s.length === 3 && /^[0-9]{3}$/.test(s));

// The parts of a structurally valid tag (lower case), or null.
function parseTag(tag) {
  const p = tag.toLowerCase().split('-');
  let i = 0;
  const t = { language: p[0], script: '', region: '', variants: [], ext: [], pu: '' };
  if (!(isAlpha(t.language, 2, 3) || isAlpha(t.language, 5, 8))) return null;
  i = 1;
  if (i < p.length && isAlpha(p[i], 4, 4)) t.script = p[i++];
  if (i < p.length && isRegion(p[i])) t.region = p[i++];
  while (i < p.length && isVariant(p[i])) {
    if (t.variants.includes(p[i])) return null;
    t.variants.push(p[i++]);
  }
  const singletons = new Set();
  while (i < p.length && p[i].length === 1 && p[i] !== 'x') {
    const s = p[i++];
    if (!ALNUM.test(s) || singletons.has(s)) return null;
    singletons.add(s);
    const start = i;
    if (s === 'u') {
      while (i < p.length && isAlnum(p[i], 3, 8)) i++; // attributes
      while (i < p.length && p[i].length === 2 && /^[a-z0-9][a-z]$/.test(p[i])) {
        i++;
        while (i < p.length && isAlnum(p[i], 3, 8)) i++;
      }
    } else if (s === 't') {
      if (i < p.length && (isAlpha(p[i], 2, 3) || isAlpha(p[i], 5, 8))) {
        i++;
        if (i < p.length && isAlpha(p[i], 4, 4)) i++;
        if (i < p.length && isRegion(p[i])) i++;
        while (i < p.length && isVariant(p[i])) i++;
      }
      while (i < p.length && p[i].length === 2 && /^[a-z][0-9]$/.test(p[i])) {
        const k = i++;
        while (i < p.length && isAlnum(p[i], 3, 8)) i++;
        if (i === k + 1) return null;
      }
    } else {
      while (i < p.length && isAlnum(p[i], 2, 8)) i++;
    }
    if (i === start) return null;
    t.ext.push([s, p.slice(start, i)]);
  }
  if (i < p.length && p[i] === 'x') {
    const start = ++i;
    while (i < p.length && isAlnum(p[i], 1, 8)) i++;
    if (i === start) return null;
    t.pu = p.slice(start, i).join('-');
  }
  return i === p.length ? t : null;
}

let aliasMaps = null;
function aliases() {
  if (aliasMaps === null) {
    const map = (s) => new Map(s.split(';').map((e) => e.split('=')));
    // Language rules by their language: { region, variants, repl } (most specific first).
    const language = new Map();
    for (const e of meta.languageAlias.split(';')) {
      const [from, to] = e.split('=');
      const f = parseTag(from);
      const rule = { region: f.region, variants: f.variants, repl: parseTag(to) };
      if (!language.has(f.language)) language.set(f.language, []);
      language.get(f.language).push(rule);
    }
    const rank = (r) => r.variants.length * 2 + (r.region ? 1 : 0);
    for (const rules of language.values()) rules.sort((x, y) => rank(y) - rank(x));
    aliasMaps = {
      language,
      script: map(meta.scriptAlias),
      territory: map(meta.territoryAlias),
      territoryRegion: map(meta.territoryRegion),
      variant: map(meta.variantAlias),
      value: map(meta.valueAlias),
    };
  }
  return aliasMaps;
}

function matchLanguageRule(t, lang) {
  const rules = aliases().language.get(lang);
  if (rules === undefined) return null;
  for (const r of rules) {
    if (r.region && r.region !== t.region) continue;
    if (r.variants.every((v) => t.variants.includes(v))) return r;
  }
  return null;
}

// UTS 35 alias replacement (languages, scripts, regions, variants), with node's rules (intl_meta.js).
function replaceAliases(t) {
  const a = aliases();
  for (let round = 0; round < 8; round++) {
    let changed = false;
    const rule = matchLanguageRule(t, t.language) ?? matchLanguageRule(t, 'und');
    if (rule !== null) {
      const r = rule.repl;
      t.variants = t.variants.filter((v) => !rule.variants.includes(v));
      if (r.language !== 'und') t.language = r.language;
      if (r.script && !t.script) t.script = r.script;
      if (rule.region) t.region = r.region;
      else if (r.region && !t.region) t.region = r.region;
      for (const v of r.variants) if (!t.variants.includes(v)) t.variants.push(v);
      changed = true;
    }
    if (!changed && t.script) {
      const r = a.script.get(t.script[0].toUpperCase() + t.script.slice(1));
      if (r !== undefined) {
        t.script = r.toLowerCase();
        changed = true;
      }
    }
    if (!changed && t.region) {
      const up = t.region.toUpperCase();
      const r = a.territory.get(up);
      if (r !== undefined) {
        t.region = (a.territoryRegion.get(`${t.language}-${up}`) ?? r.split(' ')[0]).toLowerCase();
        changed = true;
      }
    }
    if (!changed) {
      for (let i = 0; i < t.variants.length; i++) {
        const r = a.variant.get(t.variants[i]);
        if (r !== undefined) {
          t.variants[i] = r;
          changed = true;
          break;
        }
      }
    }
    if (!changed) break;
  }
}

// A -u-/-t- keyword's canonical subtags (`ca islamicc` → `ca islamic-civil`, `kn true` → `kn`).
function canonicalKeyword(ext, key, value) {
  if (value.length === 0) return [key];
  const r = aliases().value.get(`${ext}-${key}-${value.join('-')}`);
  return r === undefined ? [key, ...value] : r.split('-');
}

function canonicalize(tag) {
  const t = parseTag(tag);
  if (t === null) {
    const last = tag.slice(tag.lastIndexOf('-') + 1);
    if (tag.includes('-') && last.length === 1) throw new RangeError(`Invalid language tag: ${tag}`);
    throw new RangeError('Incorrect locale information provided');
  }
  // en-US-POSIX: CLDR's variant `posix` → -u-va-posix.
  if (t.variants.includes('posix')) {
    t.variants = t.variants.filter((v) => v !== 'posix');
    const u = t.ext.find((e) => e[0] === 'u');
    if (u) {
      if (!u[1].includes('va')) u[1].push('va', 'posix');
    } else {
      t.ext.push(['u', ['va', 'posix']]);
    }
  }
  replaceAliases(t);
  let s = t.language;
  if (t.script) s += '-' + t.script[0].toUpperCase() + t.script.slice(1);
  if (t.region) s += '-' + t.region.toUpperCase();
  for (const v of t.variants.slice().sort()) s += '-' + v;
  for (const [single, subtags] of t.ext.slice().sort((x, y) => (x[0] < y[0] ? -1 : 1))) {
    let body;
    if (single === 'u') body = canonicalUnicodeExtension(subtags);
    else if (single === 't') body = canonicalTransformedExtension(subtags);
    else body = subtags;
    if (body.length) s += '-' + single + '-' + body.join('-');
  }
  if (t.pu) s += '-x-' + t.pu;
  return s;
}

// -u-: attributes (sorted, deduplicated), then keywords by key; the first of a repeated key wins;
// a value `true` is dropped.
function canonicalUnicodeExtension(subtags) {
  const attrs = [];
  let i = 0;
  while (i < subtags.length && subtags[i].length !== 2) {
    if (!attrs.includes(subtags[i])) attrs.push(subtags[i]);
    i++;
  }
  const keywords = [];
  while (i < subtags.length) {
    const key = subtags[i++];
    const value = [];
    while (i < subtags.length && subtags[i].length !== 2) value.push(subtags[i++]);
    if (!keywords.some((k) => k[0] === key)) keywords.push([key, value.join('-') === 'true' ? [] : value]);
  }
  keywords.sort((x, y) => (x[0] < y[0] ? -1 : x[0] > y[0] ? 1 : 0));
  return [...attrs.sort(), ...keywords.flatMap(([k, v]) => canonicalKeyword('u', k, v))];
}

// -t-: the source language tag, then fields by key.
function canonicalTransformedExtension(subtags) {
  let i = 0;
  const lang = [];
  while (i < subtags.length && !/^[a-z][0-9]$/.test(subtags[i])) lang.push(subtags[i++]);
  const fields = [];
  while (i < subtags.length) {
    const key = subtags[i++];
    const value = [];
    while (i < subtags.length && !/^[a-z][0-9]$/.test(subtags[i])) value.push(subtags[i++]);
    fields.push([key, value]);
  }
  fields.sort((x, y) => (x[0] < y[0] ? -1 : 1));
  const l = lang.length ? canonicalize(lang.join('-')).toLowerCase().split('-') : [];
  return [...l, ...fields.flatMap(([k, v]) => canonicalKeyword('t', k, v))];
}

function canonicalizeLocaleList(locales) {
  if (locales === undefined) return [];
  const seen = [];
  const o = typeof locales === 'string' ? [locales] : toObject(locales);
  const len = Math.min(Math.max(Math.trunc(Number(o.length)) || 0, 0), Number.MAX_SAFE_INTEGER);
  for (let k = 0; k < len; k++) {
    if (!(k in o)) continue;
    const v = o[k];
    if (typeof v !== 'string' && (typeof v !== 'object' || v === null)) {
      throw new TypeError('Language ID should be string or object.');
    }
    const tag = canonicalize(String(v));
    if (!seen.includes(tag)) seen.push(tag);
  }
  return seen;
}

let availableSet = null;
function available() {
  if (availableSet === null) availableSet = new Set(meta.available.split(' '));
  return availableSet;
}

// BestAvailableLocale over node's available locales, without extensions.
function bestAvailable(tag) {
  let candidate = tag;
  for (;;) {
    if (available().has(candidate)) return candidate;
    let pos = candidate.lastIndexOf('-');
    if (pos < 0) return undefined;
    if (pos >= 2 && candidate[pos - 2] === '-') pos -= 2;
    candidate = candidate.slice(0, pos);
  }
}

function removeExtensions(tag) {
  const p = tag.split('-');
  const out = [];
  for (let i = 0; i < p.length; i++) {
    if (p[i].length === 1) break;
    out.push(p[i]);
  }
  return out.join('-');
}

function unicodeKeywords(tag) {
  const p = tag.split('-');
  const i = p.indexOf('u');
  const kw = new Map();
  if (i < 0 || p.slice(0, i).includes('x')) return kw;
  let j = i + 1;
  while (j < p.length && p[j].length > 2) j++;
  while (j < p.length && p[j].length === 2) {
    const key = p[j++];
    const v = [];
    while (j < p.length && p[j].length > 2) v.push(p[j++]);
    if (!kw.has(key)) kw.set(key, v.join('-'));
  }
  return kw;
}

// ResolveLocale (lookup): the matched locale and the requested -u- keywords.
function resolveLocale(requested) {
  for (const tag of requested) {
    const found = bestAvailable(removeExtensions(tag));
    if (found !== undefined) return { locale: found, keywords: unicodeKeywords(tag) };
  }
  return { locale: DEFAULT_LOCALE, keywords: new Map() };
}

// The data of a resolved locale js2wasi3 has, else the error.
function localeData(locale) {
  const lang = locale.split('-')[0];
  if ((locale !== 'en' && locale !== 'en-US') || !LOCALE_DATA[lang]) throw unsupported(`The locale '${locale}'`);
  return LOCALE_DATA[lang]();
}

function supportedLocales(requested, options, service) {
  const o = coerceOptions(options, service);
  getOption(o, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit', service);
  return requested.filter((tag) => bestAvailable(removeExtensions(tag)) !== undefined);
}

// ---- time zones -------------------------------------------------------------------------------

let zoneIds = null;
function zones() {
  if (zoneIds === null) {
    const byLower = new Map();
    const entry = new Map();
    const ids = tzData.ids.split(' ').map((s) => s.split('='));
    for (const [id, e] of ids) {
      byLower.set(id.toLowerCase(), id);
      entry.set(id, +e);
    }
    for (const a of tzData.aliases.split(' ')) {
      const [from, to] = a.split('=');
      byLower.set(from, ids[+to][0]);
    }
    zoneIds = { byLower, entry, parsed: new Map(), names: null, tuples: null };
  }
  return zoneIds;
}

// An offset time zone (`+05:30`, `-0800`, `+05`) as minutes, or null.
function offsetZone(s) {
  const m = /^([+-])([0-9]{2})(?::?([0-9]{2}))?$/.exec(s);
  if (!m || (m[3] === undefined && s.length !== 3) || (m[3] !== undefined && s.length !== 6 && s.length !== 5)) return null;
  const h = +m[2], min = m[3] === undefined ? 0 : +m[3];
  if (h > 23 || min > 59) return null;
  return (m[1] === '-' ? -1 : 1) * (h * 60 + min);
}

const pad2 = (n) => (n < 10 ? '0' : '') + n;

// The resolved id of a time zone option, or throws.
function resolveTimeZone(value) {
  const s = String(value);
  const off = offsetZone(s);
  if (off !== null) {
    const a = Math.abs(off);
    return { id: `${off < 0 ? '-' : '+'}${pad2(Math.floor(a / 60))}:${pad2(a % 60)}`, offset: off * 60 };
  }
  const id = zones().byLower.get(s.toLowerCase());
  if (id === undefined) throw new RangeError(`Invalid time zone specified: ${s}`);
  return { id, offset: null };
}

const b36 = (s) => (s[0] === '-' ? -parseInt(s.slice(1), 36) : parseInt(s, 36));
const secs = (s) => (s[0] === 's' ? b36(s.slice(1)) : b36(s) * 60);

// A zone's periods: { base, late, starts (ms), offsets (s), tuples }; `late`: offset → the name tuple from
// LATE_NAMES on (no metazone; "" for a name in GMT format), or null.
function zonePeriods(id) {
  const z = zones();
  let p = z.parsed.get(id);
  if (p === undefined) {
    const items = tzData.zones[z.entry.get(id)].split(';');
    const starts = [], offsets = [], tuples = [];
    let t = 0;
    for (let i = 1; i < items.length; i++) {
      const [d, o, n] = items[i].split(',');
      if (i === 1) starts.push(-Infinity);
      else starts.push((t += secs(d) * 1000));
      offsets.push(secs(o));
      tuples.push(parseInt(n, 36));
    }
    const [base, l] = items[0].split('|');
    let late = null;
    if (l !== undefined) {
      late = new Map();
      const x = l.split(':');
      for (let i = 0; i < x.length; i += 2) late.set(secs(x[i]), parseInt(x[i + 1], 36));
    }
    p = { base: parseInt(base, 36), late, starts, offsets, tuples };
    z.parsed.set(id, p);
  }
  return p;
}

const isLeap = (y) => (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0;
const jan1 = (y) => new Date(Date.UTC(y, 0, 1)).getUTCDay();

function equivalentYear(y, base) {
  for (let e = base; e < base + 28; e++) if (isLeap(e) === isLeap(y) && jan1(e) === jan1(y)) return e;
  for (let e = base - 1; e >= base - 28; e--) if (isLeap(e) === isLeap(y) && jan1(e) === jan1(y)) return e;
  return y;
}

// The period index of time t (ms) in a named zone.
function periodAt(p, t) {
  const y = new Date(t).getUTCFullYear();
  if (y >= p.base + 28) {
    const e = equivalentYear(y, p.base);
    const s = new Date(t);
    s.setUTCFullYear(e);
    t = s.getTime();
  }
  let lo = 0, hi = p.starts.length - 1;
  while (lo < hi) {
    const mid = (lo + hi + 1) >> 1;
    if (p.starts[mid] <= t) lo = mid;
    else hi = mid - 1;
  }
  return lo;
}

function zoneNames(tuple) {
  const z = zones();
  if (z.names === null) {
    z.names = tzData.names.split('\n');
    z.tuples = tzData.tuples.split(' ');
  }
  return z.tuples[tuple].split('.').map((i) => z.names[parseInt(i, 36)]);
}

// GMT formats of an offset in seconds: short `GMT+5:30`, long `GMT+05:30` (V8: `GMT+0`, `GMT+00:00` for 0).
function gmtFormat(offset, long) {
  const a = Math.abs(offset);
  const h = Math.floor(a / 3600), m = Math.floor((a % 3600) / 60), s = a % 60;
  let r = 'GMT' + (offset < 0 ? '-' : '+') + (long ? pad2(h) : String(h));
  if (long || m !== 0 || s !== 0) r += ':' + pad2(m);
  if (s !== 0) r += ':' + pad2(s);
  return r;
}

// ICU's metazone mappings end at UTC 9999-12-31T23:59.
const LATE_NAMES = 253402300740000;

// { offset (s), names: [short, long, shortGeneric, longGeneric] } of a zone at t.
function zoneAt(zone, t) {
  // `+00:00` is ICU's GMT zone: the GMT metazone from 1970 until LATE_NAMES.
  if (zone.offset === 0 && t >= 0 && t < LATE_NAMES) return { offset: 0, names: ['GMT', 'Greenwich Mean Time', 'GMT', 'Greenwich Mean Time'] };
  if (zone.offset !== null) {
    const short = gmtFormat(zone.offset, false), long = gmtFormat(zone.offset, true);
    return { offset: zone.offset, names: [short, long, short, long] };
  }
  const p = zonePeriods(zone.id);
  const i = periodAt(p, t);
  const offset = p.offsets[i];
  if (t >= LATE_NAMES && p.late !== null) {
    const n = zoneNames(p.late.get(offset) ?? p.tuples[i]);
    return { offset, names: [n[0] || gmtFormat(offset, false), n[1] || gmtFormat(offset, true), n[2], n[3]] };
  }
  return { offset, names: zoneNames(p.tuples[i]) };
}

// ---- date fields ------------------------------------------------------------------------------

// Proleptic Gregorian fields of local time `ms` (days from 1970 → civil date).
function fieldsOf(ms) {
  const days = Math.floor(ms / 86400000);
  const msInDay = ms - days * 86400000;
  const z = days + 719468;
  const era = Math.floor(z / 146097);
  const doe = z - era * 146097;
  const yoe = Math.floor((doe - Math.floor(doe / 1460) + Math.floor(doe / 36524) - Math.floor(doe / 146096)) / 365);
  const doy = doe - (365 * yoe + Math.floor(yoe / 4) - Math.floor(yoe / 100));
  const mp = Math.floor((5 * doy + 2) / 153);
  const day = doy - Math.floor((153 * mp + 2) / 5) + 1;
  const month = mp < 10 ? mp + 3 : mp - 9;
  const year = yoe + era * 400 + (month <= 2 ? 1 : 0);
  return {
    year,
    month,
    day,
    weekday: (((days + 4) % 7) + 7) % 7,
    hour: Math.floor(msInDay / 3600000),
    minute: Math.floor(msInDay / 60000) % 60,
    second: Math.floor(msInDay / 1000) % 60,
    ms: msInDay % 1000,
  };
}

// ---- patterns ---------------------------------------------------------------------------------

// A SimpleDateFormat pattern as items: [letter, count] or a literal string.
function compilePattern(pattern) {
  const items = [];
  let lit = '', i = 0, quoted = false;
  while (i < pattern.length) {
    const c = pattern[i];
    if (c === "'") {
      if (pattern[i + 1] === "'") {
        lit += "'";
        i += 2;
      } else {
        quoted = !quoted;
        i++;
      }
    } else if (!quoted && ((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z'))) {
      let j = i + 1;
      while (pattern[j] === c) j++;
      if (lit) items.push(lit);
      lit = '';
      items.push([c, j - i]);
      i = j;
    } else {
      lit += c;
      i++;
    }
  }
  if (lit) items.push(lit);
  return items;
}

const PART_TYPE = {
  G: 'era', y: 'year', M: 'month', L: 'month', d: 'day', E: 'weekday', c: 'weekday', e: 'weekday',
  a: 'dayPeriod', b: 'dayPeriod', B: 'dayPeriod', h: 'hour', H: 'hour', K: 'hour', k: 'hour',
  m: 'minute', s: 'second', S: 'fractionalSecond', z: 'timeZoneName', v: 'timeZoneName', O: 'timeZoneName',
};

function zeroPad(value, min, max) {
  let s = String(value);
  if (s.length > max) s = s.slice(s.length - max);
  while (s.length < min) s = '0' + s;
  return s;
}

const width = (count) => (count === 4 ? 'wide' : count === 5 ? 'narrow' : 'abbreviated');

function dayPeriodFor(data, minutes) {
  for (const r of data.dayPeriodRules) {
    if (r.length === 3 && minutes >= r[1] && minutes < r[2]) return r[0];
  }
  return minutes < 720 ? 'am' : 'pm';
}

// The value of one pattern field.
function fieldText(data, f, zone, letter, count, hasMinute, hasSecond) {
  switch (letter) {
    case 'G': {
      const e = f.year > 0 ? 1 : 0;
      return count === 4 ? data.eras.names[e] : count === 5 ? data.eras.narrow[e] : data.eras.abbr[e];
    }
    case 'y': {
      const y = f.year > 0 ? f.year : 1 - f.year;
      return count === 2 ? zeroPad(y, 2, 2) : zeroPad(y, count, 10);
    }
    case 'M':
    case 'L':
      if (count <= 2) return zeroPad(f.month, count, 10);
      return data.months[letter === 'M' ? 'format' : 'standalone'][width(count)][f.month - 1];
    case 'd':
      return zeroPad(f.day, count, 10);
    case 'E':
    case 'c':
    case 'e': {
      if (letter !== 'E' && count < 3) return zeroPad(f.weekday + 1, letter === 'c' ? 1 : count, 10);
      const ctx = letter === 'c' ? 'standalone' : 'format';
      const w = count === 4 ? 'wide' : count === 5 ? 'narrow' : count === 6 ? 'short' : 'abbreviated';
      return data.days[ctx][w][f.weekday];
    }
    case 'a':
      return data.dayPeriods.format[width(count)][f.hour < 12 ? 'am' : 'pm'];
    case 'b':
      if (f.hour === 12 && (!hasMinute || f.minute === 0) && (!hasSecond || f.second === 0)) {
        const n = data.dayPeriods.format[width(count)].noon;
        if (n !== undefined) return n;
      }
      return fieldText(data, f, zone, 'a', count, hasMinute, hasSecond);
    case 'B': {
      const minute = hasMinute ? f.minute : 0, second = hasSecond ? f.second : 0;
      const hasNoon = data.dayPeriodRules.some((r) => r[0] === 'noon');
      let period;
      if (f.hour === 12 && minute === 0 && second === 0 && hasNoon) period = 'noon';
      else period = dayPeriodFor(data, f.hour * 60);
      if (period === 'am' || period === 'pm') return fieldText(data, f, zone, 'a', count, hasMinute, hasSecond);
      return data.dayPeriods.format[width(count)][period];
    }
    case 'h':
      return zeroPad(f.hour % 12 === 0 ? 12 : f.hour % 12, count, 10);
    case 'H':
      return zeroPad(f.hour, count, 10);
    case 'K':
      return zeroPad(f.hour % 12, count, 10);
    case 'k':
      return zeroPad(f.hour === 0 ? 24 : f.hour, count, 10);
    case 'm':
      return zeroPad(f.minute, count, 10);
    case 's':
      return zeroPad(f.second, count, 10);
    case 'S': {
      let v = f.ms;
      if (count === 1) v = Math.floor(v / 100);
      else if (count === 2) v = Math.floor(v / 10);
      let s = zeroPad(v, Math.min(count, 3), 10);
      if (count > 3) s += '0'.repeat(count - 3);
      return s;
    }
    case 'z':
      return count < 4 ? zone.names[0] : zone.names[1];
    case 'v':
      return count === 1 ? zone.names[2] : zone.names[3];
    case 'O':
      return gmtFormat(zone.offset, count === 4);
  }
  throw new Error(`js2wasi3 Intl: pattern field ${letter.repeat(count)}`);
}

// ---- Intl.DateTimeFormat ----------------------------------------------------------------------

const generators = new Map();
function generatorFor(lang, data, hourChar, noStdPatterns = false) {
  const key = lang + hourChar + (noStdPatterns ? '-' : '');
  let g = generators.get(key);
  if (g === undefined) {
    g = new Generator(data, hourChar, noStdPatterns);
    generators.set(key, g);
  }
  return g;
}

const HC_CHAR = { h11: 'K', h12: 'h', h23: 'H', h24: 'k' };

function hourCycleFromPattern(pattern) {
  let inQuote = false;
  for (const ch of pattern) {
    if (ch === "'") inQuote = !inQuote;
    else if (!inQuote) {
      if (ch === 'K') return 'h11';
      if (ch === 'h') return 'h12';
      if (ch === 'H') return 'h23';
      if (ch === 'k') return 'h24';
    }
  }
  return undefined;
}

// V8's ReplaceHourCycleInPattern (incl. its space after a day field).
function replaceHourCycle(pattern, hc) {
  if (hc === undefined) return pattern;
  const to = HC_CHAR[hc];
  let replace = true, out = '', last = '';
  for (const ch of pattern) {
    if (ch === "'") {
      replace = !replace;
      out += ch;
    } else if (ch === 'H' || ch === 'h' || ch === 'K' || ch === 'k') {
      if (replace && last === 'd') out += ' ';
      out += replace ? to : ch;
    } else {
      out += ch;
    }
    last = ch;
  }
  return out;
}

function replaceSkeleton(skeleton, hc) {
  const to = HC_CHAR[hc];
  let out = '';
  for (const ch of skeleton) {
    if (ch === 'a' || ch === 'b' || ch === 'B') continue;
    out += ch === 'h' || ch === 'H' || ch === 'K' || ch === 'k' ? to : ch;
  }
  return out;
}

// The option rows in V8's order: property, values, skeleton per value (hour: per hour cycle).
const ROWS = [
  ['weekday', ['narrow', 'long', 'short'], { narrow: 'EEEEE', long: 'EEEE', short: 'EEE' }],
  ['era', ['narrow', 'long', 'short'], { narrow: 'GGGGG', long: 'GGGG', short: 'GGG' }],
  ['year', ['2-digit', 'numeric'], { '2-digit': 'yy', numeric: 'y' }],
  ['month', ['narrow', 'long', 'short', '2-digit', 'numeric'], { narrow: 'MMMMM', long: 'MMMM', short: 'MMM', '2-digit': 'MM', numeric: 'M' }],
  ['day', ['2-digit', 'numeric'], { '2-digit': 'dd', numeric: 'd' }],
  ['dayPeriod', ['narrow', 'long', 'short'], { narrow: 'BBBBB', long: 'BBBB', short: 'B' }],
  ['hour', ['2-digit', 'numeric'], null],
  ['minute', ['2-digit', 'numeric'], { '2-digit': 'mm', numeric: 'm' }],
  ['second', ['2-digit', 'numeric'], { '2-digit': 'ss', numeric: 's' }],
  ['timeZoneName', ['long', 'short', 'longOffset', 'shortOffset', 'longGeneric', 'shortGeneric'],
    { long: 'zzzz', short: 'z', longOffset: 'OOOO', shortOffset: 'O', longGeneric: 'vvvv', shortGeneric: 'v' }],
];
// resolvedOptions: the first of each row's pattern texts found in the pattern (V8's PatternItems).
const RESOLVED = [
  ['weekday', [['EEEEE', 'narrow'], ['EEEE', 'long'], ['EEE', 'short'], ['ccccc', 'narrow'], ['cccc', 'long'], ['ccc', 'short']]],
  ['era', [['GGGGG', 'narrow'], ['GGGG', 'long'], ['GGG', 'short']]],
  ['year', [['yy', '2-digit'], ['y', 'numeric']]],
  ['month', [['MMMMM', 'narrow'], ['MMMM', 'long'], ['MMM', 'short'], ['MM', '2-digit'], ['M', 'numeric'],
    ['LLLLL', 'narrow'], ['LLLL', 'long'], ['LLL', 'short'], ['LL', '2-digit'], ['L', 'numeric']]],
  ['day', [['dd', '2-digit'], ['d', 'numeric']]],
  ['dayPeriod', [['BBBBB', 'narrow'], ['bbbbb', 'narrow'], ['BBBB', 'long'], ['bbbb', 'long'], ['B', 'short'], ['b', 'short']]],
  ['hour', [['HH', '2-digit'], ['H', 'numeric'], ['hh', '2-digit'], ['h', 'numeric'], ['kk', '2-digit'], ['k', 'numeric'],
    ['KK', '2-digit'], ['K', 'numeric']]],
  ['minute', [['mm', '2-digit'], ['m', 'numeric']]],
  ['second', [['ss', '2-digit'], ['s', 'numeric']]],
  ['timeZoneName', [['zzzz', 'long'], ['z', 'short'], ['OOOO', 'longOffset'], ['O', 'shortOffset'], ['vvvv', 'longGeneric'],
    ['v', 'shortGeneric']]],
];
const STYLES = ['full', 'long', 'medium', 'short'];
// ICU's SimpleDateFormat builds a style's time pattern from these where the locale has -u-hc-.
const TIME_SKELETONS = ['jmmsszzzz', 'jmmssz', 'jmmss', 'jmm'];

const CALENDAR_TYPE = /^[a-z0-9]{3,8}(-[a-z0-9]{3,8})*$/i;

// CreateDateTimeFormat(locales, options, required, defaults): the internal state.
function createDateTimeFormat(locales, options, required, defaults) {
  const service = 'Intl.DateTimeFormat';
  const requested = canonicalizeLocaleList(locales);
  options = coerceOptions(options, service);
  getOption(options, 'localeMatcher', 'string', ['lookup', 'best fit'], 'best fit', service);
  const calendar = getOption(options, 'calendar', 'string', undefined, undefined, service);
  if (calendar !== undefined && !CALENDAR_TYPE.test(calendar)) throw new RangeError(`Invalid calendar : ${calendar}`);
  const numberingSystem = getOption(options, 'numberingSystem', 'string', undefined, undefined, service);
  if (numberingSystem !== undefined && !CALENDAR_TYPE.test(numberingSystem)) {
    throw new RangeError(`Invalid numberingSystem : ${numberingSystem}`);
  }
  const hour12 = getOption(options, 'hour12', 'boolean', undefined, undefined, service);
  let hourCycle = getOption(options, 'hourCycle', 'string', ['h11', 'h12', 'h23', 'h24'], undefined, service);
  if (hour12 !== undefined) hourCycle = undefined;

  const r = resolveLocale(requested);
  const data = localeData(r.locale);
  const kw = r.keywords;
  // The supported -u- keywords of the request, in key order: ca, hc, nu.
  const ext = new Map();
  const ca = kw.get('ca');
  if (ca !== undefined && meta.values.calendar.split(' ').includes(ca === 'gregorian' ? 'gregory' : ca)) ext.set('ca', ca);
  const hcKw = kw.get('hc');
  if (hcKw !== undefined && HC_CHAR[hcKw] !== undefined) ext.set('hc', hcKw);
  const nu = kw.get('nu');
  if (nu !== undefined && meta.values.numberingSystem.split(' ').includes(nu)) ext.set('nu', nu);
  if (calendar !== undefined && ext.has('ca') && ext.get('ca') !== calendar) ext.delete('ca');
  if (numberingSystem !== undefined && ext.has('nu') && ext.get('nu') !== numberingSystem) ext.delete('nu');

  let resolvedCalendar = ext.get('ca') ?? 'gregory';
  if (calendar !== undefined && meta.values.calendar.split(' ').includes(calendar.toLowerCase())) {
    resolvedCalendar = calendar.toLowerCase();
  }
  if (resolvedCalendar === 'gregorian') resolvedCalendar = 'gregory';
  if (resolvedCalendar !== 'gregory') throw unsupported(`The calendar '${resolvedCalendar}'`);
  let resolvedNu = ext.get('nu') ?? 'latn';
  if (numberingSystem !== undefined && meta.values.numberingSystem.split(' ').includes(numberingSystem.toLowerCase())) {
    resolvedNu = numberingSystem.toLowerCase();
  }
  if (resolvedNu !== 'latn') throw unsupported(`The numbering system '${resolvedNu}'`);

  const gen = generatorFor(r.locale.split('-')[0], data, ext.has('hc') ? HC_CHAR[ext.get('hc')] : '');
  const hcDefault = { K: 'h11', h: 'h12', H: 'h23', k: 'h24' }[gen.defaultHourChar];
  let hc = hourCycle ?? ext.get('hc');
  if (hour12 !== undefined) {
    if (hour12) hc = hcDefault === 'h11' || hcDefault === 'h12' ? hcDefault : r.locale.split('-')[1] === 'JP' ? 'h11' : 'h12';
    else hc = hcDefault === 'h23' || hcDefault === 'h24' ? hcDefault : 'h23';
  } else if (hc === undefined) {
    hc = hcDefault;
  }

  const tzOption = options.timeZone;
  const zone = tzOption === undefined ? resolveTimeZone(DEFAULT_TIME_ZONE) : resolveTimeZone(tzOption);

  let explicit = false, hasHour = false, skeleton = '';
  const seen = {};
  for (const [prop, values, map] of ROWS) {
    if (prop === 'timeZoneName') {
      const fsd = getNumberOption(options, 'fractionalSecondDigits', 1, 3, undefined);
      if (fsd !== undefined) {
        explicit = true;
        seen.fractionalSecondDigits = true;
        skeleton += 'S'.repeat(fsd);
      }
    }
    const v = getOption(options, prop, 'string', values, undefined, service);
    if (v === undefined) continue;
    explicit = true;
    seen[prop] = true;
    if (prop === 'hour') {
      hasHour = true;
      const c = HC_CHAR[hc];
      skeleton += v === '2-digit' ? c + c : c;
    } else {
      skeleton += map[v];
    }
  }
  getOption(options, 'formatMatcher', 'string', ['best fit', 'basic'], 'best fit', service);
  const dateStyle = getOption(options, 'dateStyle', 'string', STYLES, undefined, service);
  const timeStyle = getOption(options, 'timeStyle', 'string', STYLES, undefined, service);
  let dtfHc;
  if (timeStyle !== undefined) dtfHc = hc;

  let pattern;
  if (dateStyle !== undefined || timeStyle !== undefined) {
    if (explicit) throw new TypeError('Invalid option : option');
    if (required === 'date' && timeStyle !== undefined) throw new TypeError('Invalid option : timeStyle');
    if (required === 'time' && dateStyle !== undefined) throw new TypeError('Invalid option : dateStyle');
    if (timeStyle === undefined) {
      pattern = data.dateFormats[STYLES.indexOf(dateStyle)];
    } else {
      const time = ext.has('hc')
        ? generatorFor(r.locale.split('-')[0], data, HC_CHAR[ext.get('hc')], true).bestPattern(TIME_SKELETONS[STYLES.indexOf(timeStyle)], 0)
        : data.timeFormats[STYLES.indexOf(timeStyle)];
      pattern = dateStyle === undefined ? time
        : data.dateTimeFormats[STYLES.indexOf(dateStyle)].replace(/\{([01])\}/g, (_, n) => (n === '0' ? time : data.dateFormats[STYLES.indexOf(dateStyle)]));
      if (hourCycleFromPattern(pattern) !== dtfHc) {
        pattern = replaceHourCycle(gen.bestPattern(replaceSkeleton(patternSkeleton(pattern), dtfHc), MATCH_HOUR_FIELD_LENGTH), dtfHc);
      }
    }
  } else {
    let needDefaults = true;
    if (required === 'date' || required === 'any') {
      if (seen.weekday || seen.year || seen.month || seen.day) needDefaults = false;
    }
    if (required === 'time' || required === 'any') {
      if (seen.dayPeriod || seen.hour || seen.minute || seen.second || seen.fractionalSecondDigits) needDefaults = false;
    }
    if (needDefaults && (defaults === 'date' || defaults === 'all')) skeleton += 'yMd';
    if (needDefaults && (defaults === 'time' || defaults === 'all')) {
      skeleton += { h12: 'hms', h23: 'Hms', h11: 'Kms', h24: 'kms' }[hc];
    }
    dtfHc = hasHour ? hc : undefined;
    pattern = replaceHourCycle(gen.bestPattern(skeleton, MATCH_HOUR_FIELD_LENGTH), dtfHc);
  }

  if ((hour12 !== undefined || hourCycle !== undefined) && ext.has('hc') && ext.get('hc') !== dtfHc) ext.delete('hc');
  let locale = r.locale;
  if (ext.size) locale += '-u-' + [...ext].map(([k, v]) => `${k}-${v}`).join('-');

  const items = compilePattern(pattern);
  return {
    locale, data, zone, pattern, items, dateStyle, timeStyle, hourCycle: dtfHc,
    hasMinute: items.some((i) => typeof i !== 'string' && i[0] === 'm'),
    hasSecond: items.some((i) => typeof i !== 'string' && i[0] === 's'),
    boundFormat: undefined,
  };
}

// ToDateTimeFormattable + TimeClip.
function timeValue(date) {
  let x = date === undefined ? Date.now() : Number(date);
  if (!Number.isFinite(x) || Math.abs(x) > 8.64e15) throw new RangeError('Invalid time value');
  x = Math.trunc(x) + 0;
  return x;
}

function formatParts(s, t) {
  const zone = zoneAt(s.zone, t);
  const f = fieldsOf(t + zone.offset * 1000);
  const parts = [];
  for (const it of s.items) {
    if (typeof it === 'string') {
      parts.push({ type: 'literal', value: it });
    } else {
      parts.push({ type: PART_TYPE[it[0]], value: fieldText(s.data, f, zone, it[0], it[1], s.hasMinute, s.hasSecond) });
    }
  }
  return parts;
}

// V8's FormatDateTime reverts ICU 72's U+202F before AM/PM to a space (crbug.com/1414292); formatToParts
// keeps it.
function formatString(s, t) {
  let r = '';
  for (const p of formatParts(s, t)) r += p.value;
  return r.replaceAll('\u202f', ' ');
}

class DateTimeFormatState {
  #s;
  constructor(locales, options) {
    this.#s = createDateTimeFormat(locales, options, 'any', 'date');
  }
  static get(o) {
    return o !== null && (typeof o === 'object' || typeof o === 'function') && #s in o ? o.#s : undefined;
  }
}

function incompatible(method, receiver) {
  let d;
  if (receiver !== null && typeof receiver === 'object') {
    const c = receiver.constructor;
    d = `#<${(c && typeof c === 'function' && c.name) || 'Object'}>`;
  } else if (typeof receiver === 'string') {
    d = receiver;
  } else {
    d = String(receiver);
  }
  return new TypeError(`Method ${method} called on incompatible receiver ${d}`);
}

function unwrap(o, method) {
  const s = DateTimeFormatState.get(o);
  if (s === undefined) throw incompatible(method, o);
  return s;
}

function DateTimeFormat(locales = undefined, options = undefined) {
  return Reflect.construct(DateTimeFormatState, [locales, options], new.target || DateTimeFormat);
}

const dtfProto = DateTimeFormat.prototype;
Object.defineProperties(dtfProto, {
  resolvedOptions: {
    value: function resolvedOptions() {
      const s = unwrap(this, 'Intl.DateTimeFormat.prototype.resolvedOptions');
      const o = {
        locale: s.locale,
        calendar: 'gregory',
        numberingSystem: 'latn',
        timeZone: s.zone.id,
      };
      if (s.hourCycle !== undefined) {
        o.hourCycle = s.hourCycle;
        o.hour12 = s.hourCycle === 'h11' || s.hourCycle === 'h12';
      }
      if (s.dateStyle === undefined && s.timeStyle === undefined) {
        for (const [prop, pairs] of RESOLVED) {
          if (prop === 'timeZoneName') {
            let fsd = 0;
            for (let i = 0; i < s.pattern.length && fsd < 3; i++) if (s.pattern[i] === 'S') fsd++;
            if (fsd > 0) o.fractionalSecondDigits = fsd;
          }
          for (const [text, value] of pairs) {
            if (s.pattern.includes(text)) {
              o[prop] = value;
              break;
            }
          }
        }
      }
      if (s.dateStyle !== undefined) o.dateStyle = s.dateStyle;
      if (s.timeStyle !== undefined) o.timeStyle = s.timeStyle;
      return o;
    },
    writable: true,
    configurable: true,
  },
  formatToParts: {
    value: function formatToParts(date) {
      const s = unwrap(this, 'Intl.DateTimeFormat.prototype.formatToParts');
      return formatParts(s, timeValue(date));
    },
    writable: true,
    configurable: true,
  },
  format: {
    get: function format() {
      const s = unwrap(this, 'UnwrapDateTimeFormat');
      if (s.boundFormat === undefined) {
        const bound = (date) => formatString(s, timeValue(date));
        Object.defineProperty(bound, 'name', { value: '' });
        s.boundFormat = bound;
      }
      return s.boundFormat;
    },
    configurable: true,
  },
  formatRange: {
    value: function formatRange(startDate, endDate) {
      unwrap(this, 'Intl.DateTimeFormat.prototype.formatRange');
      throw unsupported('Intl.DateTimeFormat.prototype.formatRange');
    },
    writable: true,
    configurable: true,
  },
  formatRangeToParts: {
    value: function formatRangeToParts(startDate, endDate) {
      unwrap(this, 'Intl.DateTimeFormat.prototype.formatRangeToParts');
      throw unsupported('Intl.DateTimeFormat.prototype.formatRangeToParts');
    },
    writable: true,
    configurable: true,
  },
  [Symbol.toStringTag]: { value: 'Intl.DateTimeFormat', configurable: true },
});
Object.defineProperty(DateTimeFormat, 'prototype', { writable: false, enumerable: false, configurable: false });
Object.defineProperty(DateTimeFormat, 'supportedLocalesOf', {
  value: function supportedLocalesOf(locales, options = undefined) {
    return supportedLocales(canonicalizeLocaleList(locales), options, 'Intl.DateTimeFormat.supportedLocalesOf');
  },
  writable: true,
  configurable: true,
});

// ---- Date.prototype.toLocale{,Date,Time}String --------------------------------------------------

// Where a program names `Intl`, these replace the prelude's (which take no locales or options, F182):
// ECMA-402's ToDateTimeOptions(required, defaults) + FormatDateTime, as V8.
function dateLocaleMethod(name, required, defaults) {
  const f = {
    [name](locales = undefined, options = undefined) {
      const t = Date.prototype.getTime.call(this);
      if (Number.isNaN(t)) return 'Invalid Date';
      return formatString(createDateTimeFormat(locales, options, required, defaults), t);
    },
  }[name];
  Object.defineProperty(Date.prototype, name, { value: f, writable: true, enumerable: false, configurable: true });
}
dateLocaleMethod('toLocaleString', 'any', 'all');
dateLocaleMethod('toLocaleDateString', 'date', 'date');
dateLocaleMethod('toLocaleTimeString', 'time', 'time');

// ---- the other constructors (later stages of T77) -----------------------------------------------

function notYet(name) {
  const f = {
    [name]: function () {
      throw new TypeError(`Intl.${name} isn't supported by js2wasi3 yet (T77)`);
    },
  }[name];
  Object.defineProperty(f, 'supportedLocalesOf', {
    value: function supportedLocalesOf(locales, options = undefined) {
      return supportedLocales(canonicalizeLocaleList(locales), options, `Intl.${name}.supportedLocalesOf`);
    },
    writable: true,
    configurable: true,
  });
  return f;
}

// ---- Intl ---------------------------------------------------------------------------------------

const Intl = {};
const SUPPORTED_KEYS = ['calendar', 'collation', 'currency', 'numberingSystem', 'timeZone', 'unit'];
Object.defineProperties(Intl, {
  getCanonicalLocales: {
    value: function getCanonicalLocales(locales) {
      return canonicalizeLocaleList(locales);
    },
    writable: true,
    configurable: true,
  },
  supportedValuesOf: {
    value: function supportedValuesOf(key) {
      key = String(key);
      if (!SUPPORTED_KEYS.includes(key)) throw new RangeError(`Invalid key : ${key}`);
      return (key === 'timeZone' ? tzData.supported : meta.values[key]).split(' ');
    },
    writable: true,
    configurable: true,
  },
  DateTimeFormat: { value: DateTimeFormat, writable: true, configurable: true },
  [Symbol.toStringTag]: { value: 'Intl', configurable: true },
});
for (const name of ['NumberFormat', 'Collator', 'PluralRules', 'RelativeTimeFormat', 'ListFormat', 'Locale',
  'DisplayNames', 'Segmenter', 'DurationFormat']) {
  Object.defineProperty(Intl, name, { value: notYet(name), writable: true, configurable: true });
}

module.exports = { Intl, createDateTimeFormat, formatString, timeValue };
