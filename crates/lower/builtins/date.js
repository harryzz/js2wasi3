// ---- Date (T26) --------------------------------------------------------------------------------
// A Date is an object of class `CLASS_DATE` whose data `[tv]` (its time value: ms since the epoch,
// or NaN) is under a private key: `__intrinsic_date_new(data)`, `__intrinsic_date_data(v)`
// (`undefined` for anything else). Local time is UTC: WASI 0.3.1 has no stable time zone interface
// (T26's probe: wasmtime 49 doesn't implement `wasi:clocks/timezone`), so the local offset is 0, like
// node with `TZ=UTC`. The spec's day/time arithmetic runs on numbers; year/month/day come from
// Howard Hinnant's days <-> civil conversion.

const $$msPerDay = 86400000;

// The time value of `this`, or node's TypeError.
function $$dtThis(d) {
  const data = __intrinsic_date_data(d);
  if (data === undefined) {
    throw new TypeError("this is not a Date object.");
  }
  return data;
}

// The time value of `this` for method `name` (setters, string forms), or V8's TypeError.
function $$dtThisNamed(d, name) {
  const data = __intrinsic_date_data(d);
  if (data === undefined) {
    throw new TypeError("Method Date.prototype." + name + " called on incompatible receiver " + $$reWhat(d));
  }
  return data;
}

// ToIntegerOrInfinity of a number (-0 → +0).
function $$dtInt(x) {
  if (x !== x) {
    return 0;
  }
  return __intrinsic_trunc(x) + 0;
}

function $$dtFinite(x) {
  return x === x && x !== Infinity && x !== -Infinity;
}

// TimeClip.
function $$dtClip(t) {
  if (!$$dtFinite(t) || __intrinsic_abs(t) > 8.64e15) {
    return NaN;
  }
  return __intrinsic_trunc(t) + 0;
}

// MakeTime.
function $$dtMakeTime(h, m, s, ms) {
  if (!$$dtFinite(h) || !$$dtFinite(m) || !$$dtFinite(s) || !$$dtFinite(ms)) {
    return NaN;
  }
  return $$dtInt(h) * 3600000 + $$dtInt(m) * 60000 + $$dtInt(s) * 1000 + $$dtInt(ms);
}

// Days from 1970-01-01 to the civil date `y`-`m`-`d` (`m` 1..12, `d` from 1; any `y`).
function $$dtDaysFromCivil(y, m, d) {
  if (m <= 2) {
    y = y - 1;
  }
  const era = __intrinsic_floor(y / 400);
  const yoe = y - era * 400;
  const mp = m > 2 ? m - 3 : m + 9;
  const doy = __intrinsic_floor((153 * mp + 2) / 5) + d - 1;
  const doe = yoe * 365 + __intrinsic_floor(yoe / 4) - __intrinsic_floor(yoe / 100) + doy;
  return era * 146097 + doe - 719468;
}

// MakeDay.
function $$dtMakeDay(year, month, date) {
  if (!$$dtFinite(year) || !$$dtFinite(month) || !$$dtFinite(date)) {
    return NaN;
  }
  const m = $$dtInt(month);
  const ym = $$dtInt(year) + __intrinsic_floor(m / 12);
  // Far outside TimeClip's range (and exact arithmetic).
  if (__intrinsic_abs(ym) > 1000000) {
    return NaN;
  }
  const mn = m - __intrinsic_floor(m / 12) * 12;
  return $$dtDaysFromCivil(ym, mn + 1, 1) + $$dtInt(date) - 1;
}

// MakeDate.
function $$dtMakeDate(day, time) {
  if (!$$dtFinite(day) || !$$dtFinite(time)) {
    return NaN;
  }
  const tv = day * $$msPerDay + time;
  return $$dtFinite(tv) ? tv : NaN;
}

function $$dtDay(t) {
  return __intrinsic_floor(t / $$msPerDay);
}

function $$dtTimeWithinDay(t) {
  return t - __intrinsic_floor(t / $$msPerDay) * $$msPerDay;
}

// The civil date of a time value, in `$$dtY`, `$$dtMo` (0..11), `$$dtD` (1..31).
let $$dtY = 0;
let $$dtMo = 0;
let $$dtD = 0;

function $$dtCivil(t) {
  const z = $$dtDay(t) + 719468;
  const era = __intrinsic_floor(z / 146097);
  const doe = z - era * 146097;
  const yoe = __intrinsic_floor((doe - __intrinsic_floor(doe / 1460) + __intrinsic_floor(doe / 36524) -
    __intrinsic_floor(doe / 146096)) / 365);
  const doy = doe - (365 * yoe + __intrinsic_floor(yoe / 4) - __intrinsic_floor(yoe / 100));
  const mp = __intrinsic_floor((5 * doy + 2) / 153);
  $$dtD = doy - __intrinsic_floor((153 * mp + 2) / 5) + 1;
  const m = mp < 10 ? mp + 3 : mp - 9;
  $$dtMo = m - 1;
  $$dtY = yoe + era * 400 + (m <= 2 ? 1 : 0);
}

function $$dtYear(t) {
  $$dtCivil(t);
  return $$dtY;
}

function $$dtMonth(t) {
  $$dtCivil(t);
  return $$dtMo;
}

function $$dtDate(t) {
  $$dtCivil(t);
  return $$dtD;
}

function $$dtWeekDay(t) {
  const w = ($$dtDay(t) + 4) % 7;
  return w < 0 ? w + 7 : w;
}

function $$dtHour(t) {
  return __intrinsic_floor($$dtTimeWithinDay(t) / 3600000);
}

function $$dtMin(t) {
  return __intrinsic_floor($$dtTimeWithinDay(t) / 60000) % 60;
}

function $$dtSec(t) {
  return __intrinsic_floor($$dtTimeWithinDay(t) / 1000) % 60;
}

function $$dtMs(t) {
  return $$dtTimeWithinDay(t) % 1000;
}

// ---- constructor and statics --------------------------------------------------------------------

// `new Date(…)`, and the construct entry of `Date` as a value.
function $$Date(...args) {
  const n = args.length;
  let tv = 0;
  if (n === 0) {
    tv = __intrinsic_date_now();
  } else if (n === 1) {
    const value = args[0];
    const data = __intrinsic_date_data(value);
    if (data !== undefined) {
      tv = $$dtClip(+data[0]);
    } else {
      const v = __intrinsic_to_primitive(value, 0);
      tv = $$dtClip(typeof v === "string" ? $$dtParse(v) : +v);
    }
  } else {
    tv = $$dtFromFields(args);
  }
  const o = __intrinsic_date_new([tv]);
  // OrdinaryCreateFromConstructor: through `Date` as a value (`super()`, `Reflect.construct`) the
  // construct entry passes the new target as `this`; `new Date(…)` itself passes `undefined`.
  const newTarget = this;
  if (newTarget !== undefined) {
    const proto = newTarget.prototype;
    if ($$isObject(proto) && proto !== Object.getPrototypeOf(o)) {
      __intrinsic_set_proto(o, proto);
    }
  }
  return o;
}

// `Date(…)`: the current time as a string (the arguments are ignored).
function $$DateCall() {
  return $$dtToString(__intrinsic_date_now());
}

// The time value of `Date(y, m, …)` / `Date.UTC(y, m, …)` (local time is UTC).
function $$dtFromFields(args) {
  const n = args.length;
  const y = n > 0 ? +args[0] : NaN;
  const m = n > 1 ? +args[1] : 0;
  const dt = n > 2 ? +args[2] : 1;
  const h = n > 3 ? +args[3] : 0;
  const min = n > 4 ? +args[4] : 0;
  const s = n > 5 ? +args[5] : 0;
  const ms = n > 6 ? +args[6] : 0;
  let yr = y;
  if (y === y) {
    const yi = $$dtInt(y);
    if (yi >= 0 && yi <= 99) {
      yr = 1900 + yi;
    }
  }
  return $$dtClip($$dtMakeDate($$dtMakeDay(yr, m, dt), $$dtMakeTime(h, min, s, ms)));
}

function $$Date_now() {
  return __intrinsic_date_now();
}

function $$Date_UTC(...args) {
  return $$dtFromFields(args);
}

function $$Date_parse(string) {
  return $$dtParse(__intrinsic_to_string(string));
}

// ---- parsing ------------------------------------------------------------------------------------

// Date.parse: the Date Time String Format, then V8's legacy forms (`Sep 28 2026`, `9/28/2026 10:00 PM`,
// `toString`/`toUTCString` output, …). NaN when neither applies.
function $$dtParse(s) {
  const iso = $$dtParseIso(s);
  if (iso === iso) {
    return iso;
  }
  return $$dtParseLegacy(s);
}

// Where the last `$$dtDigits` stopped, and how many digits it read.
let $$dtAt = 0;
let $$dtLen = 0;

// The number in the digits of `s` from `i` (at most `max`); sets `$$dtAt`/`$$dtLen`.
function $$dtDigits(s, i, max) {
  let v = 0;
  let k = 0;
  while (i < s.length && k < max) {
    const c = __intrinsic_unit(s, i);
    if (c < 48 || c > 57) {
      break;
    }
    v = v * 10 + (c - 48);
    i++;
    k++;
  }
  $$dtAt = i;
  $$dtLen = k;
  return v;
}

// The Date Time String Format (`YYYY[-MM[-DD]][THH:mm[:ss[.sss]][Z|±HH:mm]]`, `±YYYYYY` years), or NaN.
function $$dtParseIso(s) {
  const n = s.length;
  let i = 0;
  let year = 0;
  const c0 = n > 0 ? __intrinsic_unit(s, 0) : 0;
  if (c0 === 43 || c0 === 45) {
    year = $$dtDigits(s, 1, 6);
    if ($$dtLen !== 6 || (c0 === 45 && year === 0)) {
      return NaN;
    }
    if (c0 === 45) {
      year = -year;
    }
  } else {
    year = $$dtDigits(s, 0, 4);
    if ($$dtLen !== 4) {
      return NaN;
    }
  }
  i = $$dtAt;
  let month = 1;
  let day = 1;
  if (i < n && __intrinsic_unit(s, i) === 45) {
    month = $$dtDigits(s, i + 1, 2);
    if ($$dtLen !== 2 || month < 1 || month > 12) {
      return NaN;
    }
    i = $$dtAt;
    if (i < n && __intrinsic_unit(s, i) === 45) {
      day = $$dtDigits(s, i + 1, 2);
      if ($$dtLen !== 2 || day < 1 || day > $$dtDaysInMonth(year, month)) {
        return NaN;
      }
      i = $$dtAt;
    }
  }
  let time = 0;
  let offset = 0;
  if (i < n) {
    const t = __intrinsic_unit(s, i);
    if (t !== 84 && t !== 116) {
      return NaN;
    }
    const h = $$dtDigits(s, i + 1, 2);
    if ($$dtLen !== 2 || $$dtAt >= n || __intrinsic_unit(s, $$dtAt) !== 58) {
      return NaN;
    }
    const m = $$dtDigits(s, $$dtAt + 1, 2);
    if ($$dtLen !== 2) {
      return NaN;
    }
    i = $$dtAt;
    let sec = 0;
    let ms = 0;
    if (i < n && __intrinsic_unit(s, i) === 58) {
      sec = $$dtDigits(s, i + 1, 2);
      if ($$dtLen !== 2) {
        return NaN;
      }
      i = $$dtAt;
      if (i < n && __intrinsic_unit(s, i) === 46) {
        // Any number of digits; the first three count.
        let k = 0;
        i++;
        const start = i;
        while (i < n && __intrinsic_unit(s, i) >= 48 && __intrinsic_unit(s, i) <= 57) {
          if (k < 3) {
            ms = ms * 10 + (__intrinsic_unit(s, i) - 48);
            k++;
          }
          i++;
        }
        if (i === start) {
          return NaN;
        }
        while (k < 3) {
          ms = ms * 10;
          k++;
        }
      }
    }
    if (h > 24 || m > 59 || sec > 59 || (h === 24 && (m > 0 || sec > 0 || ms > 0))) {
      return NaN;
    }
    time = h * 3600000 + m * 60000 + sec * 1000 + ms;
    if (i < n) {
      const z = __intrinsic_unit(s, i);
      if (z === 90 || z === 122) {
        i++;
      } else if (z === 43 || z === 45) {
        const oh = $$dtDigits(s, i + 1, 2);
        if ($$dtLen !== 2 || $$dtAt >= n || __intrinsic_unit(s, $$dtAt) !== 58) {
          return NaN;
        }
        const om = $$dtDigits(s, $$dtAt + 1, 2);
        if ($$dtLen !== 2 || oh > 23 || om > 59) {
          return NaN;
        }
        offset = (oh * 60 + om) * 60000;
        if (z === 45) {
          offset = -offset;
        }
        i = $$dtAt;
      }
    }
  }
  if (i !== n) {
    return NaN;
  }
  return $$dtClip($$dtMakeDate($$dtDaysFromCivil(year, month, day), time) - offset);
}

function $$dtDaysInMonth(y, m) {
  if (m === 2) {
    return (y % 4 === 0 && y % 100 !== 0) || y % 400 === 0 ? 29 : 28;
  }
  return m === 4 || m === 6 || m === 9 || m === 11 ? 30 : 31;
}

// The month (1..12) a word names (its first three letters, any case), or 0.
function $$dtMonthWord(w) {
  const months = "janfebmaraprmayjunjulaugsepoctnovdec";
  if (w.length < 3) {
    return 0;
  }
  const k = __intrinsic_sub(w, 0, 3);
  for (let i = 0; i < 12; i++) {
    if (__intrinsic_sub(months, 3 * i, 3 * i + 3) === k) {
      return i + 1;
    }
  }
  return 0;
}

function $$dtIsWeekDay(w) {
  const days = "sunmontuewedthufrisat";
  if (w.length < 3) {
    return false;
  }
  const k = __intrinsic_sub(w, 0, 3);
  for (let i = 0; i < 7; i++) {
    if (__intrinsic_sub(days, 3 * i, 3 * i + 3) === k) {
      return true;
    }
  }
  return false;
}

// V8's legacy date forms (a subset of its `DateParser`): numbers separated by `/`, `-` or `.` are a
// date (Y-M-D when the first isn't a day, else M/D/Y), `h:mm[:ss[.sss]]` a time, `AM`/`PM`, month
// names, `GMT`/`UTC`/`UT`/`Z` and `±hh[:]mm` offsets; week days and `(…)` comments are ignored; an
// unknown word after a number makes it invalid. Two-digit years: 0–49 → 20xx, 50–99 → 19xx.
function $$dtParseLegacy(s) {
  const n = s.length;
  const nums = [];
  let month = 0;
  let hour = -1;
  let minute = 0;
  let sec = 0;
  let ms = 0;
  let ampm = 0;
  let tz = false;
  let offset = 0;
  let i = 0;
  while (i < n) {
    const c = __intrinsic_unit(s, i);
    if (c === 32 || c === 44 || c === 9 || c === 10 || c === 13) {
      i++;
    } else if (c === 40) {
      // A comment, possibly nested.
      let depth = 0;
      while (i < n) {
        const d = __intrinsic_unit(s, i);
        i++;
        if (d === 40) {
          depth++;
        } else if (d === 41) {
          depth--;
          if (depth === 0) {
            break;
          }
        }
      }
    } else if (c >= 48 && c <= 57) {
      const v = $$dtDigits(s, i, 9);
      i = $$dtAt;
      if (i < n && __intrinsic_unit(s, i) === 58 && hour < 0) {
        // A time: h:mm[:ss[.sss]].
        hour = v;
        minute = $$dtDigits(s, i + 1, 2);
        if ($$dtLen === 0) {
          return NaN;
        }
        i = $$dtAt;
        if (i < n && __intrinsic_unit(s, i) === 58) {
          sec = $$dtDigits(s, i + 1, 2);
          if ($$dtLen === 0) {
            return NaN;
          }
          i = $$dtAt;
          if (i < n && __intrinsic_unit(s, i) === 46) {
            ms = $$dtDigits(s, i + 1, 3);
            const k = $$dtLen;
            if (k === 0) {
              return NaN;
            }
            i = $$dtAt;
            ms = k === 1 ? ms * 100 : k === 2 ? ms * 10 : ms;
            while (i < n && __intrinsic_unit(s, i) >= 48 && __intrinsic_unit(s, i) <= 57) {
              i++;
            }
          }
        }
      } else {
        if (nums.length >= 3) {
          return NaN;
        }
        $$append(nums, v);
        if (i < n) {
          const d = __intrinsic_unit(s, i);
          if (d === 47 || d === 45 || d === 46) {
            i++;
          }
        }
      }
    } else if ((c === 43 || c === 45) && (hour >= 0 || tz) && i + 1 < n &&
      __intrinsic_unit(s, i + 1) >= 48 && __intrinsic_unit(s, i + 1) <= 57) {
      // An offset: ±hh, ±hhmm or ±hh:mm.
      let v = $$dtDigits(s, i + 1, 4);
      const k = $$dtLen;
      i = $$dtAt;
      if (k <= 2) {
        v = v * 100;
        if (i < n && __intrinsic_unit(s, i) === 58) {
          v = v + $$dtDigits(s, i + 1, 2);
          i = $$dtAt;
        }
      }
      offset = (__intrinsic_floor(v / 100) * 60 + (v % 100)) * 60000;
      if (c === 45) {
        offset = -offset;
      }
      tz = true;
    } else if ((c >= 65 && c <= 90) || (c >= 97 && c <= 122)) {
      const start = i;
      while (i < n) {
        const d = __intrinsic_unit(s, i);
        if (!((d >= 65 && d <= 90) || (d >= 97 && d <= 122))) {
          break;
        }
        i++;
      }
      const w = __intrinsic_sub(s, start, i).toLowerCase();
      if (w === "am" || w === "pm") {
        ampm = w === "am" ? 1 : 2;
      } else if (w === "utc" || w === "gmt" || w === "ut" || w === "z") {
        tz = true;
      } else if (w === "t" && nums.length > 0) {
        // The date/time separator of `2026-09-28T10:00` forms.
      } else if ($$dtMonthWord(w) !== 0 && month === 0) {
        month = $$dtMonthWord(w);
      } else if ($$dtIsWeekDay(w)) {
        // Ignored.
      } else if (nums.length > 0 || hour >= 0) {
        return NaN;
      }
    } else {
      // Other punctuation is ignored only before any number; a sign there (an ISO extended year
      // the ISO parser rejected) is invalid.
      if (nums.length > 0 || hour >= 0 || c === 43 || c === 45) {
        return NaN;
      }
      i++;
    }
  }
  // Compose the date.
  const k = nums.length;
  let year = 0;
  let mon = 0;
  let day = 0;
  if (month === 0) {
    if (k < 2) {
      return NaN;
    }
    const a = +nums[0];
    if (k === 3 && !(a >= 1 && a <= 31)) {
      year = a;
      mon = +nums[1];
      day = +nums[2];
    } else {
      mon = a;
      day = +nums[1];
      year = k === 3 ? +nums[2] : 2001;
    }
  } else {
    mon = month;
    if (k === 0) {
      return NaN;
    }
    const a = +nums[0];
    if (k === 1) {
      if (a >= 1 && a <= 31) {
        day = a;
        year = 2001;
      } else {
        day = 1;
        year = a;
      }
    } else if (!(a >= 1 && a <= 31)) {
      year = a;
      day = +nums[1];
    } else {
      day = a;
      year = +nums[1];
    }
  }
  if (year >= 0 && year <= 49) {
    year += 2000;
  } else if (year >= 50 && year <= 99) {
    year += 1900;
  }
  if (mon < 1 || mon > 12 || day < 1 || day > 31) {
    return NaN;
  }
  if (hour < 0) {
    hour = 0;
  }
  if (ampm !== 0) {
    if (hour > 12) {
      return NaN;
    }
    if (hour === 12) {
      hour = 0;
    }
    if (ampm === 2) {
      hour += 12;
    }
  }
  if (hour > 24 || minute > 59 || sec > 59 || (hour === 24 && (minute > 0 || sec > 0 || ms > 0))) {
    return NaN;
  }
  const days = $$dtDaysFromCivil(year, mon, 1) + day - 1;
  return $$dtClip($$dtMakeDate(days, hour * 3600000 + minute * 60000 + sec * 1000 + ms) - offset);
}

// ---- formatting ---------------------------------------------------------------------------------

// `n` (a non-negative integer) with at least `w` digits.
function $$dtPad(n, w) {
  let s = __intrinsic_to_string(n);
  while (s.length < w) {
    s = "0" + s;
  }
  return s;
}

function $$dtWeekDayName(t) {
  const i = $$dtWeekDay(t);
  return __intrinsic_sub("SunMonTueWedThuFriSat", 3 * i, 3 * i + 3);
}

function $$dtMonthName(m) {
  return __intrinsic_sub("JanFebMarAprMayJunJulAugSepOctNovDec", 3 * m, 3 * m + 3);
}

// The year as `toString` writes it: at least four digits, `-` before a negative one.
function $$dtYearText(y) {
  return (y < 0 ? "-" : "") + $$dtPad(__intrinsic_abs(y), 4);
}

// DateString: `Mon Sep 28 2026`.
function $$dtDateString(t) {
  $$dtCivil(t);
  return $$dtWeekDayName(t) + " " + $$dtMonthName($$dtMo) + " " + $$dtPad($$dtD, 2) + " " +
    $$dtYearText($$dtY);
}

// TimeString: `12:03:04 GMT`.
function $$dtTimeString(t) {
  return $$dtPad($$dtHour(t), 2) + ":" + $$dtPad($$dtMin(t), 2) + ":" + $$dtPad($$dtSec(t), 2) + " GMT";
}

// The local time zone (UTC, T26).
function $$dtZoneString() {
  return "+0000 (Coordinated Universal Time)";
}

function $$dtToString(t) {
  if (t !== t) {
    return "Invalid Date";
  }
  return $$dtDateString(t) + " " + $$dtTimeString(t) + $$dtZoneString();
}

function $$dtIso(t) {
  $$dtCivil(t);
  const y = $$dtY;
  const year = y >= 0 && y <= 9999 ? $$dtPad(y, 4) : (y < 0 ? "-" : "+") + $$dtPad(__intrinsic_abs(y), 6);
  return year + "-" + $$dtPad($$dtMo + 1, 2) + "-" + $$dtPad($$dtD, 2) + "T" + $$dtPad($$dtHour(t), 2) + ":" +
    $$dtPad($$dtMin(t), 2) + ":" + $$dtPad($$dtSec(t), 2) + "." + $$dtPad($$dtMs(t), 3) + "Z";
}

// `console.log`'s text of a Date (called by the runtime): a subclass's instance starts with its
// constructor's name (`X 1970-01-01T00:00:00.000Z`).
function $$dateInspect(d) {
  const t = +$$dtThis(d)[0];
  const text = t !== t ? "Invalid Date" : $$dtIso(t);
  let p = Object.getPrototypeOf(d);
  if (p === null || p === Object.getPrototypeOf(__intrinsic_date_new([0]))) {
    return text;
  }
  while (p !== null && !Object.hasOwn(p, "constructor")) {
    p = Object.getPrototypeOf(p);
  }
  const c = p === null ? undefined : p.constructor;
  const name = typeof c === "function" ? c.name : "";
  return name !== "" && name !== "Date" ? name + " " + text : text;
}

// node's `en-US` forms (`9/28/2026`, `12:03:04 PM`) for `toLocale*String()` without arguments.
function $$dtLocaleDate(t) {
  $$dtCivil(t);
  return __intrinsic_to_string($$dtMo + 1) + "/" + __intrinsic_to_string($$dtD) + "/" +
    __intrinsic_to_string($$dtY);
}

function $$dtLocaleTime(t) {
  const h = $$dtHour(t);
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return __intrinsic_to_string(h12) + ":" + $$dtPad($$dtMin(t), 2) + ":" + $$dtPad($$dtSec(t), 2) +
    (h < 12 ? " AM" : " PM");
}

// Locales and options need `Intl` (not implemented): only the default `en-US` form.
function $$dtLocaleCheck(locales, options, name) {
  if ((locales !== undefined && locales !== "en-US") || options !== undefined) {
    throw new RangeError("Date.prototype." + name + " with locales or options is not supported (js2wasi3)");
  }
}

// ---- Date.prototype -----------------------------------------------------------------------------

function $$Date_prototype_getTime() {
  return +$$dtThis(this)[0];
}

function $$Date_prototype_valueOf() {
  return +$$dtThis(this)[0];
}

function $$Date_prototype_getTimezoneOffset() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : 0;
}

function $$Date_prototype_getFullYear() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtYear(t);
}

function $$Date_prototype_getUTCFullYear() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtYear(t);
}

// Annex B.
function $$Date_prototype_getYear() {
  const t = +$$dtThisNamed(this, "getYear")[0];
  return t !== t ? NaN : $$dtYear(t) - 1900;
}

function $$Date_prototype_getMonth() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtMonth(t);
}

function $$Date_prototype_getUTCMonth() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtMonth(t);
}

function $$Date_prototype_getDate() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtDate(t);
}

function $$Date_prototype_getUTCDate() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtDate(t);
}

function $$Date_prototype_getDay() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtWeekDay(t);
}

function $$Date_prototype_getUTCDay() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtWeekDay(t);
}

function $$Date_prototype_getHours() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtHour(t);
}

function $$Date_prototype_getUTCHours() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtHour(t);
}

function $$Date_prototype_getMinutes() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtMin(t);
}

function $$Date_prototype_getUTCMinutes() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtMin(t);
}

function $$Date_prototype_getSeconds() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtSec(t);
}

function $$Date_prototype_getUTCSeconds() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtSec(t);
}

function $$Date_prototype_getMilliseconds() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtMs(t);
}

function $$Date_prototype_getUTCMilliseconds() {
  const t = +$$dtThis(this)[0];
  return t !== t ? NaN : $$dtMs(t);
}

function $$Date_prototype_setTime(time) {
  const data = $$dtThisNamed(this, "setTime");
  const v = $$dtClip(+time);
  data[0] = v;
  return v;
}

// Set the time value to TimeClip(`date`) and return it.
function $$dtSet(data, date) {
  const v = $$dtClip(date);
  data[0] = v;
  return v;
}

// The setters of the time fields (local and UTC are the same): each converts its arguments in order
// (only those present), then returns NaN if the time value is NaN.
function $$dtSetTime(data, t, h, m, s, ms) {
  return $$dtSet(data, $$dtMakeDate($$dtDay(t), $$dtMakeTime(h, m, s, ms)));
}

function $$Date_prototype_setMilliseconds(ms) {
  const data = $$dtThisNamed(this, "setMilliseconds");
  const t = +data[0];
  const milli = +ms;
  if (t !== t) {
    return NaN;
  }
  return $$dtSetTime(data, t, $$dtHour(t), $$dtMin(t), $$dtSec(t), milli);
}

function $$Date_prototype_setUTCMilliseconds(ms) {
  const data = $$dtThisNamed(this, "setUTCMilliseconds");
  const t = +data[0];
  const milli = +ms;
  if (t !== t) {
    return NaN;
  }
  return $$dtSetTime(data, t, $$dtHour(t), $$dtMin(t), $$dtSec(t), milli);
}

function $$dtSetSeconds(data, n, sec, ms) {
  const t = +data[0];
  const s = +sec;
  const milli = n > 1 ? +ms : 0;
  if (t !== t) {
    return NaN;
  }
  return $$dtSetTime(data, t, $$dtHour(t), $$dtMin(t), s, n > 1 ? milli : $$dtMs(t));
}

function $$Date_prototype_setSeconds(sec, ms) {
  return $$dtSetSeconds($$dtThisNamed(this, "setSeconds"), arguments.length, sec, ms);
}

function $$Date_prototype_setUTCSeconds(sec, ms) {
  return $$dtSetSeconds($$dtThisNamed(this, "setUTCSeconds"), arguments.length, sec, ms);
}

function $$dtSetMinutes(data, n, min, sec, ms) {
  const t = +data[0];
  const m = +min;
  const s = n > 1 ? +sec : 0;
  const milli = n > 2 ? +ms : 0;
  if (t !== t) {
    return NaN;
  }
  return $$dtSetTime(data, t, $$dtHour(t), m, n > 1 ? s : $$dtSec(t), n > 2 ? milli : $$dtMs(t));
}

function $$Date_prototype_setMinutes(min, sec, ms) {
  return $$dtSetMinutes($$dtThisNamed(this, "setMinutes"), arguments.length, min, sec, ms);
}

function $$Date_prototype_setUTCMinutes(min, sec, ms) {
  return $$dtSetMinutes($$dtThisNamed(this, "setUTCMinutes"), arguments.length, min, sec, ms);
}

function $$dtSetHours(data, n, hour, min, sec, ms) {
  const t = +data[0];
  const h = +hour;
  const m = n > 1 ? +min : 0;
  const s = n > 2 ? +sec : 0;
  const milli = n > 3 ? +ms : 0;
  if (t !== t) {
    return NaN;
  }
  return $$dtSetTime(data, t, h, n > 1 ? m : $$dtMin(t), n > 2 ? s : $$dtSec(t), n > 3 ? milli : $$dtMs(t));
}

function $$Date_prototype_setHours(hour, min, sec, ms) {
  return $$dtSetHours($$dtThisNamed(this, "setHours"), arguments.length, hour, min, sec, ms);
}

function $$Date_prototype_setUTCHours(hour, min, sec, ms) {
  return $$dtSetHours($$dtThisNamed(this, "setUTCHours"), arguments.length, hour, min, sec, ms);
}

function $$dtSetDate(data, date) {
  const t = +data[0];
  const dt = +date;
  if (t !== t) {
    return NaN;
  }
  $$dtCivil(t);
  return $$dtSet(data, $$dtMakeDate($$dtMakeDay($$dtY, $$dtMo, dt), $$dtTimeWithinDay(t)));
}

function $$Date_prototype_setDate(date) {
  return $$dtSetDate($$dtThisNamed(this, "setDate"), date);
}

function $$Date_prototype_setUTCDate(date) {
  return $$dtSetDate($$dtThisNamed(this, "setUTCDate"), date);
}

function $$dtSetMonth(data, n, month, date) {
  const t = +data[0];
  const m = +month;
  const dt = n > 1 ? +date : 0;
  if (t !== t) {
    return NaN;
  }
  $$dtCivil(t);
  return $$dtSet(data, $$dtMakeDate($$dtMakeDay($$dtY, m, n > 1 ? dt : $$dtD), $$dtTimeWithinDay(t)));
}

function $$Date_prototype_setMonth(month, date) {
  return $$dtSetMonth($$dtThisNamed(this, "setMonth"), arguments.length, month, date);
}

function $$Date_prototype_setUTCMonth(month, date) {
  return $$dtSetMonth($$dtThisNamed(this, "setUTCMonth"), arguments.length, month, date);
}

function $$dtSetFullYear(data, n, year, month, date) {
  let t = +data[0];
  const y = +year;
  if (t !== t) {
    t = 0;
  }
  $$dtCivil(t);
  const mo = $$dtMo;
  const day = $$dtD;
  const m = n > 1 ? +month : mo;
  const dt = n > 2 ? +date : day;
  return $$dtSet(data, $$dtMakeDate($$dtMakeDay(y, m, dt), $$dtTimeWithinDay(t)));
}

function $$Date_prototype_setFullYear(year, month, date) {
  return $$dtSetFullYear($$dtThisNamed(this, "setFullYear"), arguments.length, year, month, date);
}

function $$Date_prototype_setUTCFullYear(year, month, date) {
  return $$dtSetFullYear($$dtThisNamed(this, "setUTCFullYear"), arguments.length, year, month, date);
}

// Annex B.
function $$Date_prototype_setYear(year) {
  const data = $$dtThisNamed(this, "setYear");
  let t = +data[0];
  const y = +year;
  if (y !== y) {
    data[0] = NaN;
    return NaN;
  }
  const yi = $$dtInt(y);
  const yyyy = yi >= 0 && yi <= 99 ? 1900 + yi : y;
  if (t !== t) {
    t = 0;
  }
  $$dtCivil(t);
  return $$dtSet(data, $$dtMakeDate($$dtMakeDay(yyyy, $$dtMo, $$dtD), $$dtTimeWithinDay(t)));
}

function $$Date_prototype_toString() {
  return $$dtToString(+$$dtThisNamed(this, "toString")[0]);
}

function $$Date_prototype_toDateString() {
  const t = +$$dtThisNamed(this, "toDateString")[0];
  return t !== t ? "Invalid Date" : $$dtDateString(t);
}

function $$Date_prototype_toTimeString() {
  const t = +$$dtThisNamed(this, "toTimeString")[0];
  return t !== t ? "Invalid Date" : $$dtTimeString(t) + $$dtZoneString();
}

function $$Date_prototype_toUTCString() {
  const t = +$$dtThisNamed(this, "toUTCString")[0];
  if (t !== t) {
    return "Invalid Date";
  }
  $$dtCivil(t);
  return $$dtWeekDayName(t) + ", " + $$dtPad($$dtD, 2) + " " + $$dtMonthName($$dtMo) + " " +
    $$dtYearText($$dtY) + " " + $$dtTimeString(t);
}

function $$Date_prototype_toISOString() {
  const t = +$$dtThisNamed(this, "toISOString")[0];
  if (t !== t) {
    throw new RangeError("Invalid time value");
  }
  return $$dtIso(t);
}

function $$Date_prototype_toJSON(key) {
  const o = __intrinsic_to_object(this);
  const tv = __intrinsic_to_primitive(o, 1);
  if (typeof tv === "number" && !$$dtFinite(tv)) {
    return null;
  }
  const f = o.toISOString;
  if (typeof f !== "function") {
    throw new TypeError("toISOString is not a function");
  }
  return f.call(o);
}

function $$Date_prototype_toLocaleString(locales, options) {
  const t = +$$dtThisNamed(this, "toLocaleString")[0];
  $$dtLocaleCheck(locales, options, "toLocaleString");
  return t !== t ? "Invalid Date" : $$dtLocaleDate(t) + ", " + $$dtLocaleTime(t);
}

function $$Date_prototype_toLocaleDateString(locales, options) {
  const t = +$$dtThisNamed(this, "toLocaleDateString")[0];
  $$dtLocaleCheck(locales, options, "toLocaleDateString");
  return t !== t ? "Invalid Date" : $$dtLocaleDate(t);
}

function $$Date_prototype_toLocaleTimeString(locales, options) {
  const t = +$$dtThisNamed(this, "toLocaleTimeString")[0];
  $$dtLocaleCheck(locales, options, "toLocaleTimeString");
  return t !== t ? "Invalid Date" : $$dtLocaleTime(t);
}

// `Date.prototype[Symbol.toPrimitive](hint)`.
function $$Date_prototype_$toPrimitive(hint) {
  if (!$$isObject(this)) {
    throw new TypeError("Method Date.prototype [ @@toPrimitive ] called on incompatible receiver " + $$reWhat(this));
  }
  if (hint === "string" || hint === "default") {
    return $$dtOrdinaryToPrimitive(this, "toString", "valueOf");
  }
  if (hint === "number") {
    return $$dtOrdinaryToPrimitive(this, "valueOf", "toString");
  }
  throw new TypeError("Invalid hint: " + __intrinsic_to_string(hint));
}

function $$dtOrdinaryToPrimitive(o, first, second) {
  const f = o[first];
  if (typeof f === "function") {
    const r = f.call(o);
    if (!$$isObject(r)) {
      return r;
    }
  }
  const g = o[second];
  if (typeof g === "function") {
    const r = g.call(o);
    if (!$$isObject(r)) {
      return r;
    }
  }
  throw new TypeError("Cannot convert object to primitive value");
}

