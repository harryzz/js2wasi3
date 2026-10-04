// node:internal/intl_dtpg (T77): ICU 78's DateTimePatternGenerator (icu4c i18n/dtptngen.cpp), the
// skeleton → pattern matcher behind `Intl.DateTimeFormat`, ported from C++ step by step (same tables,
// same iteration order, same tie-breaks) so its patterns are node's. Plain JS: node can run it as is,
// which is how scripts/gen-intl.mjs checks it against node's own `Intl`.
'use strict';

// UDATPG_*_FIELD
const ERA = 0, YEAR = 1, MONTH = 3, WEEKDAY = 6, DAYPERIOD = 10, HOUR = 11, MINUTE = 12, SECOND = 13;
const FRACTIONAL_SECOND = 14, ZONE = 15, FIELD_COUNT = 16;
const NARROW = -0x101, SHORTER = -0x102, SHORT = -0x103, LONG = -0x104, NUMERIC = 0x100, DELTA = 0x10;
const EXTRA_FIELD = 0x10000, MISSING_FIELD = 0x1000;
const FRACTIONAL_MASK = 1 << FRACTIONAL_SECOND;
const SECOND_AND_FRACTIONAL_MASK = (1 << SECOND) | (1 << FRACTIONAL_SECOND);
// Match options and flags.
const MATCH_HOUR_FIELD_LENGTH = 1 << HOUR;
const MATCH_MINUTE_FIELD_LENGTH = 1 << MINUTE;
const MATCH_SECOND_FIELD_LENGTH = 1 << SECOND;
const FIX_FRACTIONAL_SECONDS = 1, SKELETON_USES_CAP_J = 2;

// dtTypes: patternChar, field, type, minLen (weight unused).
const dtTypes = [
  ['G', ERA, SHORT, 1], ['G', ERA, LONG, 4], ['G', ERA, NARROW, 5],
  ['y', YEAR, NUMERIC, 1], ['Y', YEAR, NUMERIC + DELTA, 1], ['u', YEAR, NUMERIC + 2 * DELTA, 1],
  ['r', YEAR, NUMERIC + 3 * DELTA, 1], ['U', YEAR, SHORT, 1], ['U', YEAR, LONG, 4], ['U', YEAR, NARROW, 5],
  ['Q', 2, NUMERIC, 1], ['Q', 2, SHORT, 3], ['Q', 2, LONG, 4], ['Q', 2, NARROW, 5],
  ['q', 2, NUMERIC + DELTA, 1], ['q', 2, SHORT - DELTA, 3], ['q', 2, LONG - DELTA, 4], ['q', 2, NARROW - DELTA, 5],
  ['M', MONTH, NUMERIC, 1], ['M', MONTH, SHORT, 3], ['M', MONTH, LONG, 4], ['M', MONTH, NARROW, 5],
  ['L', MONTH, NUMERIC + DELTA, 1], ['L', MONTH, SHORT - DELTA, 3], ['L', MONTH, LONG - DELTA, 4],
  ['L', MONTH, NARROW - DELTA, 5], ['l', MONTH, NUMERIC + DELTA, 1],
  ['w', 4, NUMERIC, 1],
  ['W', 5, NUMERIC, 1],
  ['E', WEEKDAY, SHORT, 1], ['E', WEEKDAY, LONG, 4], ['E', WEEKDAY, NARROW, 5], ['E', WEEKDAY, SHORTER, 6],
  ['c', WEEKDAY, NUMERIC + 2 * DELTA, 1], ['c', WEEKDAY, SHORT - 2 * DELTA, 3], ['c', WEEKDAY, LONG - 2 * DELTA, 4],
  ['c', WEEKDAY, NARROW - 2 * DELTA, 5], ['c', WEEKDAY, SHORTER - 2 * DELTA, 6],
  ['e', WEEKDAY, NUMERIC + DELTA, 1], ['e', WEEKDAY, SHORT - DELTA, 3], ['e', WEEKDAY, LONG - DELTA, 4],
  ['e', WEEKDAY, NARROW - DELTA, 5], ['e', WEEKDAY, SHORTER - DELTA, 6],
  ['d', 9, NUMERIC, 1], ['g', 9, NUMERIC + DELTA, 1],
  ['D', 7, NUMERIC, 1],
  ['F', 8, NUMERIC, 1],
  ['a', DAYPERIOD, SHORT, 1], ['a', DAYPERIOD, LONG, 4], ['a', DAYPERIOD, NARROW, 5],
  ['b', DAYPERIOD, SHORT - DELTA, 1], ['b', DAYPERIOD, LONG - DELTA, 4], ['b', DAYPERIOD, NARROW - DELTA, 5],
  ['B', DAYPERIOD, SHORT - 3 * DELTA, 1], ['B', DAYPERIOD, LONG - 3 * DELTA, 4], ['B', DAYPERIOD, NARROW - 3 * DELTA, 5],
  ['H', HOUR, NUMERIC + 10 * DELTA, 1], ['k', HOUR, NUMERIC + 11 * DELTA, 1], ['h', HOUR, NUMERIC, 1],
  ['K', HOUR, NUMERIC + DELTA, 1], ['J', HOUR, NUMERIC + 5 * DELTA, 1], ['j', HOUR, NUMERIC + 6 * DELTA, 1],
  ['C', HOUR, NUMERIC + 7 * DELTA, 1],
  ['m', MINUTE, NUMERIC, 1],
  ['s', SECOND, NUMERIC, 1], ['A', SECOND, NUMERIC + DELTA, 1],
  ['S', FRACTIONAL_SECOND, NUMERIC, 1],
  ['v', ZONE, SHORT - 2 * DELTA, 1], ['v', ZONE, LONG - 2 * DELTA, 4], ['z', ZONE, SHORT, 1], ['z', ZONE, LONG, 4],
  ['Z', ZONE, NARROW - DELTA, 1], ['Z', ZONE, LONG - DELTA, 4], ['Z', ZONE, SHORT - DELTA, 5],
  ['O', ZONE, SHORT - DELTA, 1], ['O', ZONE, LONG - DELTA, 4], ['V', ZONE, SHORT - DELTA, 1],
  ['V', ZONE, LONG - DELTA, 2], ['V', ZONE, LONG - 1 - DELTA, 3], ['V', ZONE, LONG - 2 - DELTA, 4],
  ['X', ZONE, NARROW - DELTA, 1], ['X', ZONE, SHORT - DELTA, 2], ['X', ZONE, LONG - DELTA, 4],
  ['x', ZONE, NARROW - DELTA, 1], ['x', ZONE, SHORT - DELTA, 2], ['x', ZONE, LONG - DELTA, 4],
];

const CANONICAL_ITEMS = 'GyQMwWEDFdaHmsSv';
const CLDR_FIELD_APPEND = ['Era', 'Year', 'Quarter', 'Month', 'Week', '*', 'Day-Of-Week', '*', '*', 'Day', '*',
  'Hour', 'Minute', 'Second', '*', 'Timezone'];
const CLDR_FIELD_NAME = ['era', 'year', 'quarter', 'month', 'week', 'weekOfMonth', 'weekday', 'dayOfYear',
  'weekdayOfMonth', 'day', 'dayperiod', 'hour', 'minute', 'second', '*', 'zone'];
const DEFAULT_ITEM_FORMAT = '{0} ├{2}: {1}┤';

const isLetter = (c) => (c >= 'A' && c <= 'Z') || (c >= 'a' && c <= 'z');

// ---- FormatParser ----------------------------------------------------------------------------

// A pattern's items: runs of one ASCII letter, every other character alone (at most 50 items).
function parseItems(pattern) {
  const items = [];
  let i = 0;
  while (i < pattern.length && items.length < 50) {
    const c = pattern[i];
    let j = i + 1;
    if (isLetter(c)) while (j < pattern.length && pattern[j] === c) j++;
    items.push(pattern.slice(i, j));
    i = j;
  }
  return items;
}

const isQuoteLiteral = (s) => s[0] === "'";

// The quoted literal starting at items[i] (quotes kept) and the index of its last item.
function quoteLiteral(items, i) {
  let quote = '';
  if (items[i][0] === "'") {
    quote += items[i];
    ++i;
  }
  while (i < items.length) {
    if (items[i][0] === "'") {
      if (i + 1 < items.length && items[i + 1][0] === "'") {
        quote += items[i++];
        quote += items[i++];
        continue;
      }
      quote += items[i];
      break;
    }
    quote += items[i];
    ++i;
  }
  return [quote, i];
}

// ICU's check, including its use of `items[i]` (the parser's i-th item) for '.'.
function isPatternSeparator(items, field) {
  for (let i = 0; i < field.length; ++i) {
    const c = field[i];
    if (c === "'" || c === '\\' || c === ' ' || c === ':' || c === '"' || c === ',' || c === '-' ||
      (items[i] !== undefined && items[i][0] === '.')) continue;
    return false;
  }
  return true;
}

function canonicalIndex(s, strict) {
  const len = s.length;
  if (len === 0) return -1;
  const ch = s[0];
  for (let l = 1; l < len; l++) if (s[l] !== ch) return -1;
  let i = 0, bestRow = -1;
  while (i < dtTypes.length) {
    if (dtTypes[i][0] !== ch) {
      ++i;
      continue;
    }
    bestRow = i;
    if (i + 1 >= dtTypes.length || dtTypes[i][0] !== dtTypes[i + 1][0]) return i;
    if (dtTypes[i + 1][3] <= len) {
      ++i;
      continue;
    }
    return i;
  }
  return strict ? -1 : bestRow;
}

// ---- skeletons -------------------------------------------------------------------------------

class Fields {
  constructor() {
    this.chars = new Array(FIELD_COUNT).fill('');
    this.lengths = new Array(FIELD_COUNT).fill(0);
  }
  copy() {
    const f = new Fields();
    f.chars = this.chars.slice();
    f.lengths = this.lengths.slice();
    return f;
  }
  populate(field, ch, length) {
    this.chars[field] = ch;
    this.lengths[field] = length;
  }
  clear(field) {
    this.chars[field] = '';
    this.lengths[field] = 0;
  }
  isEmpty(field) {
    return this.lengths[field] === 0;
  }
  string() {
    let s = '';
    for (let i = 0; i < FIELD_COUNT; i++) s += this.chars[i].repeat(this.lengths[i]);
    return s;
  }
  firstChar() {
    for (let i = 0; i < FIELD_COUNT; i++) if (this.lengths[i] !== 0) return this.chars[i];
    return '';
  }
}

class Skeleton {
  constructor() {
    this.type = new Array(FIELD_COUNT).fill(0);
    this.original = new Fields();
    this.baseOriginal = new Fields();
    this.addedDefaultDayPeriod = false;
  }
  copy() {
    const s = new Skeleton();
    s.type = this.type.slice();
    s.original = this.original.copy();
    s.baseOriginal = this.baseOriginal.copy();
    s.addedDefaultDayPeriod = this.addedDefaultDayPeriod;
    return s;
  }
  fieldMask() {
    let r = 0;
    for (let i = 0; i < FIELD_COUNT; i++) if (this.type[i] !== 0) r |= 1 << i;
    return r;
  }
}

// DateTimeMatcher::set: the skeleton of a pattern or skeleton string.
function skeletonOf(pattern) {
  const s = new Skeleton();
  const items = parseItems(pattern);
  for (let i = 0; i < items.length; i++) {
    const value = items[i];
    if (isQuoteLiteral(value)) {
      i = quoteLiteral(items, i)[1];
      continue;
    }
    const ci = canonicalIndex(value, false);
    if (ci < 0) continue;
    const row = dtTypes[ci];
    const field = row[1];
    s.original.populate(field, value[0], value.length);
    s.baseOriginal.populate(field, row[0], row[3]);
    let sub = row[2];
    if (sub > 0) sub += value.length;
    s.type[field] = sub;
  }
  // Minutes and fractional seconds but no seconds: add seconds.
  if (!s.original.isEmpty(MINUTE) && !s.original.isEmpty(FRACTIONAL_SECOND) && s.original.isEmpty(SECOND)) {
    s.original.populate(SECOND, 's', 1);
    s.baseOriginal.populate(SECOND, 's', 1);
    s.type[SECOND] = NUMERIC + 1;
  }
  if (!s.original.isEmpty(HOUR)) {
    const hc = s.original.chars[HOUR];
    if (hc === 'h' || hc === 'K') {
      if (s.original.isEmpty(DAYPERIOD)) {
        s.original.populate(DAYPERIOD, 'a', 1);
        s.baseOriginal.populate(DAYPERIOD, 'a', 1);
        s.type[DAYPERIOD] = SHORT;
        s.addedDefaultDayPeriod = true;
      }
    } else {
      s.original.clear(DAYPERIOD);
      s.baseOriginal.clear(DAYPERIOD);
      s.type[DAYPERIOD] = 0;
    }
  }
  return s;
}

function distance(mine, other, includeMask, info) {
  let result = 0;
  info.missing = 0;
  info.extra = 0;
  for (let i = 0; i < FIELD_COUNT; i++) {
    const myType = (includeMask & (1 << i)) === 0 ? 0 : mine.type[i];
    const otherType = other.type[i];
    if (myType === otherType) continue;
    if (myType === 0) {
      result += EXTRA_FIELD;
      info.extra |= 1 << i;
    } else if (otherType === 0) {
      result += MISSING_FIELD;
      info.missing |= 1 << i;
    } else {
      result += Math.abs(myType - otherType);
    }
  }
  return result;
}

const typesEqual = (a, b) => a.every((t, i) => t === b[i]);

// ---- the generator ---------------------------------------------------------------------------

class Generator {
  // data: a locale's calendar data (scripts/gen-intl.mjs); hourChar: the -u-hc- keyword's hour
  // character, or '' for the region's preferred one; noStdPatterns: createInstanceNoStdPat.
  constructor(data, hourChar, noStdPatterns = false) {
    this.boot = new Map(); // base char → [elements] in insertion order; boot order is A-Z then a-z
    this.availableKeys = new Set();
    this.appendItemFormats = new Array(FIELD_COUNT).fill('');
    this.fieldNames = new Array(FIELD_COUNT).fill('');
    for (const c of CANONICAL_ITEMS) this.addPattern(c, null, false);
    if (!noStdPatterns) {
      for (const p of data.timeFormats) this.addPattern(p, null, false);
      for (const p of data.dateFormats) this.addPattern(p, null, false);
    }
    for (let i = 0; i < FIELD_COUNT; i++) {
      const f = data.appendItems[CLDR_FIELD_APPEND[i]];
      this.appendItemFormats[i] = f || DEFAULT_ITEM_FORMAT;
      const n = data.fieldNames[CLDR_FIELD_NAME[i]];
      this.fieldNames[i] = n || (i < 10 ? 'F' + i : 'F1' + (i - 10));
    }
    for (const [key, pattern] of data.availableFormats) {
      if (!this.availableKeys.has(key)) {
        this.availableKeys.add(key);
        this.addPattern(pattern, key, true);
      }
    }
    this.dateTimeFormats = data.dateTimeFormats;
    this.decimal = data.decimal;
    const allowed = data.hours.allowed;
    const preferred = data.hours.preferred;
    this.allowedHourFormats = allowed.slice();
    this.defaultHourChar = hourChar || ({ h: 'h', H: 'H', K: 'K', k: 'k' }[preferred[0]] ?? 'H');
  }

  headerOf(baseChar) {
    return this.boot.get(baseChar) || null;
  }

  addPattern(pattern, skeletonToUse, override) {
    const skeleton = skeletonOf(skeletonToUse === null ? pattern : skeletonToUse);
    const basePattern = skeleton.baseOriginal.string();
    const list = this.headerOf(basePattern[0]);
    if (list) {
      const dup = list.find((e) => e.basePattern === basePattern);
      if (dup && (!dup.skeletonWasSpecified || (skeletonToUse !== null && !override))) {
        if (!override) return;
      }
    }
    const firstChar = skeleton.baseOriginal.firstChar();
    const list2 = this.headerOf(firstChar);
    if (list2) {
      const orig = skeleton.original.string();
      const dup = list2.find((e) => e.skeleton.original.string() === orig);
      if (dup && (!override || (skeletonToUse !== null && dup.skeletonWasSpecified))) return;
    }
    this.mapAdd(basePattern, skeleton, pattern, skeletonToUse !== null);
  }

  mapAdd(basePattern, skeleton, value, specified) {
    const baseChar = basePattern[0];
    if (!baseChar || !isLetter(baseChar)) return;
    let list = this.boot.get(baseChar);
    if (!list) {
      this.boot.set(baseChar, [{ basePattern, pattern: value, skeleton: skeleton.copy(), skeletonWasSpecified: specified }]);
      return;
    }
    const dup = list.find((e) => e.basePattern === basePattern && typesEqual(e.skeleton.type, skeleton.type));
    if (!dup) {
      list.push({ basePattern, pattern: value, skeleton: skeleton.copy(), skeletonWasSpecified: specified });
    } else {
      dup.pattern = value;
      dup.skeletonWasSpecified = specified;
    }
  }

  *entries() {
    for (let c = 65; c <= 122; c++) {
      if (c > 90 && c < 97) continue;
      const list = this.boot.get(String.fromCharCode(c));
      if (list) yield* list;
    }
  }

  // The entry whose skeleton.original equals the trial's (getPatternFromSkeleton).
  fromSkeleton(sk) {
    const list = this.headerOf(sk.baseOriginal.firstChar());
    if (!list) return null;
    const orig = sk.original.string();
    return list.find((e) => e.skeleton.original.string() === orig) || null;
  }

  bestRaw(source, includeMask, info) {
    let bestDistance = 0x7fffffff, bestMissing = -1, best = null;
    const temp = { missing: 0, extra: 0 };
    for (const e of this.entries()) {
      const d = distance(source, e.skeleton, includeMask, temp);
      if (d < bestDistance || (d === bestDistance && bestMissing < temp.missing)) {
        bestDistance = d;
        bestMissing = temp.missing;
        best = this.fromSkeleton(e.skeleton);
        info.missing = temp.missing;
        info.extra = temp.extra;
        if (d === 0) break;
      }
    }
    return best;
  }

  // mapSkeletonMetacharacters: j, C, J.
  mapMeta(form, state) {
    let out = '';
    let inQuoted = false;
    for (let pos = 0; pos < form.length; pos++) {
      const ch = form[pos];
      if (ch === "'") {
        inQuoted = !inQuoted;
      } else if (!inQuoted) {
        if (ch === 'j' || ch === 'C') {
          let extraLen = 0;
          while (pos + 1 < form.length && form[pos + 1] === ch) {
            extraLen++;
            pos++;
          }
          let hourLen = 1 + (extraLen & 1);
          let dayPeriodLen = extraLen < 2 ? 1 : 3 + (extraLen >> 1);
          let hourChar = 'h', dayPeriodChar = 'a';
          if (ch === 'j') {
            hourChar = this.defaultHourChar;
          } else {
            const best = this.allowedHourFormats[0];
            if (best === 'H' || best === 'HB' || best === 'Hb') hourChar = 'H';
            else if (best === 'K' || best === 'KB' || best === 'Kb') hourChar = 'K';
            else if (best === 'k') hourChar = 'k';
            if (best === 'HB' || best === 'hB' || best === 'KB') dayPeriodChar = 'B';
            else if (best === 'Hb' || best === 'hb' || best === 'Kb') dayPeriodChar = 'b';
          }
          if (hourChar === 'H' || hourChar === 'k') dayPeriodLen = 0;
          out += dayPeriodChar.repeat(dayPeriodLen) + hourChar.repeat(hourLen);
        } else if (ch === 'J') {
          out += 'H';
          state.flags |= SKELETON_USES_CAP_J;
        } else {
          out += ch;
        }
      }
    }
    return out;
  }

  adjustFieldTypes(pattern, specified, flags, options, req) {
    let out = '';
    const items = parseItems(pattern);
    for (let i = 0; i < items.length; i++) {
      let field = items[i];
      if (isQuoteLiteral(field)) {
        const [q, j] = quoteLiteral(items, i);
        out += q;
        i = j;
        continue;
      }
      if (isPatternSeparator(items, field)) {
        out += field;
        continue;
      }
      const ci = canonicalIndex(field, false);
      if (ci < 0) {
        out += field;
        continue;
      }
      const row = dtTypes[ci];
      const typeValue = row[1];
      if ((flags & FIX_FRACTIONAL_SECONDS) !== 0 && typeValue === SECOND) {
        field += this.decimal;
        field += req.original.chars[FRACTIONAL_SECOND].repeat(req.original.lengths[FRACTIONAL_SECOND]);
      } else if (req.type[typeValue] !== 0) {
        const reqFieldChar = req.original.chars[typeValue];
        let reqFieldLen = req.original.lengths[typeValue];
        if (reqFieldChar === 'E' && reqFieldLen < 3) reqFieldLen = 3;
        let adjFieldLen = reqFieldLen;
        if ((typeValue === HOUR && (options & MATCH_HOUR_FIELD_LENGTH) === 0) ||
          (typeValue === MINUTE && (options & MATCH_MINUTE_FIELD_LENGTH) === 0) ||
          (typeValue === SECOND && (options & MATCH_SECOND_FIELD_LENGTH) === 0)) {
          adjFieldLen = field.length;
        } else if (specified && reqFieldChar !== 'c' && reqFieldChar !== 'e') {
          const skelFieldLen = specified.original.lengths[typeValue];
          const patNumeric = row[2] > 0;
          const reqNumeric = req.type[typeValue] > 0;
          if (skelFieldLen === reqFieldLen || (patNumeric && !reqNumeric) || (reqNumeric && !patNumeric)) {
            adjFieldLen = field.length;
          }
        }
        let c = typeValue !== HOUR && typeValue !== MONTH && typeValue !== WEEKDAY &&
          (typeValue !== YEAR || reqFieldChar === 'Y') ? reqFieldChar : field[0];
        if (c === 'E' && adjFieldLen < 3) c = 'e';
        if (typeValue === HOUR && this.defaultHourChar) {
          const d = this.defaultHourChar;
          if ((flags & SKELETON_USES_CAP_J) !== 0 || reqFieldChar === d) c = d;
          else if (reqFieldChar === 'h' && d === 'K') c = 'K';
          else if (reqFieldChar === 'H' && d === 'k') c = 'k';
          else if (reqFieldChar === 'k' && d === 'H') c = 'H';
          else if (reqFieldChar === 'K' && d === 'h') c = 'h';
        }
        field = c.repeat(adjFieldLen);
      }
      out += field;
    }
    return out;
  }

  bestAppending(missingFields, flags, options, req, info) {
    if (missingFields === 0) return '';
    let best = this.bestRaw(req, missingFields, info);
    let result = this.adjustFieldTypes(best.pattern, best.skeletonWasSpecified ? best.skeleton : null, flags, options, req);
    if (info.missing === 0) return result;
    let lastMissing = 0;
    while (info.missing !== 0) {
      if (lastMissing === info.missing) break;
      if ((info.missing & SECOND_AND_FRACTIONAL_MASK) === FRACTIONAL_MASK &&
        (missingFields & SECOND_AND_FRACTIONAL_MASK) === SECOND_AND_FRACTIONAL_MASK) {
        result = this.adjustFieldTypes(result, best.skeletonWasSpecified ? best.skeleton : null,
          flags | FIX_FRACTIONAL_SECONDS, options, req);
        info.missing &= ~FRACTIONAL_MASK;
        continue;
      }
      const starting = info.missing;
      best = this.bestRaw(req, info.missing, info);
      const temp = this.adjustFieldTypes(best.pattern, best.skeletonWasSpecified ? best.skeleton : null, flags, options, req);
      const found = starting & ~info.missing;
      const top = topBit(found);
      if (this.appendItemFormats[top].length !== 0) {
        const name = "'" + this.fieldNames[top] + "'";
        result = this.appendItemFormats[top].replace(/\{([012])\}/g, (_, n) => (n === '0' ? result : n === '1' ? temp : name));
      }
      lastMissing = info.missing;
    }
    return result;
  }

  // getBestPattern(skeleton, options).
  bestPattern(form, options) {
    const state = { flags: 0 };
    const dateMask = (1 << DAYPERIOD) - 1;
    const timeMask = (1 << FIELD_COUNT) - 1 - dateMask;
    const mapped = this.mapMeta(form, state);
    const req = skeletonOf(mapped);
    const info = { missing: 0, extra: 0 };
    const best = this.bestRaw(req, -1, info);
    if (info.missing === 0 && info.extra === 0) {
      return this.adjustFieldTypes(best.pattern, best.skeletonWasSpecified ? best.skeleton : null, state.flags, options, req);
    }
    const needed = req.fieldMask();
    const datePattern = this.bestAppending(needed & dateMask, state.flags, options, req, info);
    const timePattern = this.bestAppending(needed & timeMask, state.flags, options, req, info);
    if (datePattern.length === 0) return timePattern;
    if (timePattern.length === 0) return datePattern;
    let style = 3; // short
    const monthLen = req.baseOriginal.lengths[MONTH];
    if (monthLen === 4) style = req.baseOriginal.lengths[WEEKDAY] > 0 ? 0 : 1;
    else if (monthLen === 3) style = 2;
    return this.dateTimeFormats[style].replace(/\{([01])\}/g, (_, n) => (n === '0' ? timePattern : datePattern));
  }
}

function topBit(mask) {
  if (mask === 0) return 0;
  let i = 0;
  while (mask !== 0) {
    mask >>>= 1;
    ++i;
  }
  return i - 1 > ZONE ? ZONE : i - 1;
}

// staticGetSkeleton: a pattern's skeleton string (the original fields, without an added 'a').
function patternSkeleton(pattern) {
  const s = skeletonOf(pattern);
  let r = s.original.string();
  if (s.addedDefaultDayPeriod) {
    const i = r.indexOf('a');
    if (i >= 0) r = r.slice(0, i) + r.slice(i + 1);
  }
  return r;
}

module.exports = { Generator, patternSkeleton, MATCH_HOUR_FIELD_LENGTH };
