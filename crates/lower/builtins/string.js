
// Strings and number parsing/formatting (T21 slice 2). Conventions: crates/lower/src/builtins.rs.
// `$$String_prototype_x` is `String.prototype.x`, `$$Number_prototype_x` is `Number.prototype.x`,
// `$$parseInt` is the global `parseInt`, `$$StringToNumber` is the runtime's ToNumber of a string.
// String intrinsics: `str_len(s)`, `unit(s, i)` (a code unit), `sub(s, a, b)` (a new string),
// `str_new(n)` + `str_set(s, i, u)` (build a string), `to_string(v)` (ToString), `trap()`.

// ---- helpers -----------------------------------------------------------------------------

// RequireObjectCoercible(this) + ToString.
function $$thisString(v, name) {
  if (v === undefined || v === null) {
    throw new TypeError("String.prototype." + name + " called on null or undefined");
  }
  return __intrinsic_to_string(v);
}

// ToIntegerOrInfinity.
function $$toInteger(x) {
  x = +x;
  if (x !== x) {
    return 0;
  }
  return __intrinsic_trunc(x) + 0;
}

// Clamp an integer (or ±Infinity) to [0, len].
function $$clamp(x, len) {
  if (x < 0) {
    return 0;
  }
  if (x > len) {
    return len;
  }
  return x;
}

// A relative index (negative: from the end), clamped to [0, len].
function $$relative(x, len) {
  if (x < 0) {
    return $$clamp(len + x, len);
  }
  return $$clamp(x, len);
}

// A string of one code unit.
function $$fromUnit(u) {
  const r = __intrinsic_str_new(1);
  __intrinsic_str_set(r, 0, u);
  return r;
}

// StrWhiteSpaceChar: WhiteSpace or LineTerminator.
function $$isSpace(c) {
  if (c < 128) {
    return c === 32 || (c >= 9 && c <= 13);
  }
  return (
    c === 0xa0 ||
    c === 0x1680 ||
    (c >= 0x2000 && c <= 0x200a) ||
    c === 0x2028 ||
    c === 0x2029 ||
    c === 0x202f ||
    c === 0x205f ||
    c === 0x3000 ||
    c === 0xfeff
  );
}

// The first index >= from where t occurs in s, or -1 (from in [0, len]).
function $$indexOf(s, t, from) {
  const n = s.length;
  const m = t.length;
  for (let i = from; i + m <= n; i++) {
    let j = 0;
    while (j < m && __intrinsic_unit(s, i + j) === __intrinsic_unit(t, j)) {
      j++;
    }
    if (j === m) {
      return i;
    }
  }
  return -1;
}

// Does t occur in s at i?
function $$occursAt(s, t, i) {
  const m = t.length;
  if (i < 0 || i + m > s.length) {
    return false;
  }
  for (let j = 0; j < m; j++) {
    if (__intrinsic_unit(s, i + j) !== __intrinsic_unit(t, j)) {
      return false;
    }
  }
  return true;
}

// ---- String.prototype ----------------------------------------------------------------------

function $$String_prototype_toString() {
  const s = __intrinsic_unwrap(this);
  if (typeof s !== "string") {
    throw new TypeError("String.prototype.toString requires that 'this' be a String");
  }
  return s;
}

function $$String_prototype_valueOf() {
  const s = __intrinsic_unwrap(this);
  if (typeof s !== "string") {
    throw new TypeError("String.prototype.valueOf requires that 'this' be a String");
  }
  return s;
}

function $$String_prototype_at(index) {
  const s = $$thisString(this, "at");
  const len = s.length;
  let k = $$toInteger(index);
  if (k < 0) {
    k = len + k;
  }
  if (k < 0 || k >= len) {
    return undefined;
  }
  return __intrinsic_sub(s, k, k + 1);
}

function $$String_prototype_charAt(pos) {
  const s = $$thisString(this, "charAt");
  const k = $$toInteger(pos);
  if (k < 0 || k >= s.length) {
    return "";
  }
  return __intrinsic_sub(s, k, k + 1);
}

function $$String_prototype_charCodeAt(pos) {
  const s = $$thisString(this, "charCodeAt");
  const k = $$toInteger(pos);
  if (k < 0 || k >= s.length) {
    return NaN;
  }
  return __intrinsic_unit(s, k);
}

function $$String_prototype_codePointAt(pos) {
  const s = $$thisString(this, "codePointAt");
  const k = $$toInteger(pos);
  const len = s.length;
  if (k < 0 || k >= len) {
    return undefined;
  }
  const a = __intrinsic_unit(s, k);
  if (a >= 0xd800 && a <= 0xdbff && k + 1 < len) {
    const b = __intrinsic_unit(s, k + 1);
    if (b >= 0xdc00 && b <= 0xdfff) {
      return (a - 0xd800) * 1024 + (b - 0xdc00) + 0x10000;
    }
  }
  return a;
}

function $$String_prototype_indexOf(searchString, position) {
  const s = $$thisString(this, "indexOf");
  const t = __intrinsic_to_string(searchString);
  const start = $$clamp($$toInteger(position), s.length);
  return $$indexOf(s, t, start);
}

function $$String_prototype_lastIndexOf(searchString, position) {
  const s = $$thisString(this, "lastIndexOf");
  const t = __intrinsic_to_string(searchString);
  const n = +position;
  let pos = Infinity;
  if (n === n) {
    pos = __intrinsic_trunc(n);
  }
  const len = s.length;
  const m = t.length;
  let i = $$clamp(pos, len);
  if (i > len - m) {
    i = len - m;
  }
  for (; i >= 0; i--) {
    if ($$occursAt(s, t, i)) {
      return i;
    }
  }
  return -1;
}

// A RegExp search string is a TypeError (IsRegExp, T27: `Symbol.match` or a RegExp).
function $$noRegExp(v, name) {
  if ($$isRegExp(v)) {
    throw new TypeError(
      "First argument to String.prototype." + name + " must not be a regular expression",
    );
  }
}

function $$String_prototype_includes(searchString, position) {
  const s = $$thisString(this, "includes");
  $$noRegExp(searchString, "includes");
  const t = __intrinsic_to_string(searchString);
  const start = $$clamp($$toInteger(position), s.length);
  return $$indexOf(s, t, start) >= 0;
}

function $$String_prototype_startsWith(searchString, position) {
  const s = $$thisString(this, "startsWith");
  $$noRegExp(searchString, "startsWith");
  const t = __intrinsic_to_string(searchString);
  const start = $$clamp($$toInteger(position), s.length);
  return $$occursAt(s, t, start);
}

function $$String_prototype_endsWith(searchString, endPosition) {
  const s = $$thisString(this, "endsWith");
  $$noRegExp(searchString, "endsWith");
  const t = __intrinsic_to_string(searchString);
  const len = s.length;
  let end = len;
  if (endPosition !== undefined) {
    end = $$clamp($$toInteger(endPosition), len);
  }
  return $$occursAt(s, t, end - t.length);
}

function $$String_prototype_slice(start, end) {
  const s = $$thisString(this, "slice");
  const len = s.length;
  const from = $$relative($$toInteger(start), len);
  let to = len;
  if (end !== undefined) {
    to = $$relative($$toInteger(end), len);
  }
  if (from >= to) {
    return "";
  }
  return __intrinsic_sub(s, from, to);
}

function $$String_prototype_substring(start, end) {
  const s = $$thisString(this, "substring");
  const len = s.length;
  const a = $$clamp($$toInteger(start), len);
  let b = len;
  if (end !== undefined) {
    b = $$clamp($$toInteger(end), len);
  }
  if (a < b) {
    return __intrinsic_sub(s, a, b);
  }
  return __intrinsic_sub(s, b, a);
}

function $$String_prototype_substr(start, length) {
  const s = $$thisString(this, "substr");
  const len = s.length;
  const from = $$relative($$toInteger(start), len);
  let n = len - from;
  if (length !== undefined) {
    n = $$clamp($$toInteger(length), len - from);
  }
  if (n <= 0) {
    return "";
  }
  return __intrinsic_sub(s, from, from + n);
}

function $$String_prototype_concat(...args) {
  let r = $$thisString(this, "concat");
  for (let i = 0; i < args.length; i++) {
    r = r + __intrinsic_to_string(args[i]);
  }
  return r;
}

// node's (V8's) longest string: 2^29 - 24 code units.
function $$checkLength(n) {
  if (n > 536870888) {
    throw new RangeError("Invalid string length");
  }
}

function $$String_prototype_repeat(count) {
  const s = $$thisString(this, "repeat");
  const n = $$toInteger(count);
  if (n < 0 || n === Infinity) {
    throw new RangeError("Invalid count value: " + n);
  }
  const len = s.length;
  if (len === 0 || n === 0) {
    return "";
  }
  $$checkLength(len * n);
  const r = __intrinsic_str_new(len * n);
  let k = 0;
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < len; j++) {
      __intrinsic_str_set(r, k, __intrinsic_unit(s, j));
      k++;
    }
  }
  return r;
}

// StringPad: `s` filled to `maxLength` at the start (`atStart`) or the end.
function $$pad(s, maxLength, fillString, atStart) {
  const len = s.length;
  let target = $$toInteger(maxLength);
  if (target <= len) {
    return s;
  }
  let fill = " ";
  if (fillString !== undefined) {
    fill = __intrinsic_to_string(fillString);
  }
  const fl = fill.length;
  if (fl === 0) {
    return s;
  }
  $$checkLength(target);
  const r = __intrinsic_str_new(target);
  const n = target - len;
  const off = atStart ? 0 : len;
  for (let i = 0; i < n; i++) {
    __intrinsic_str_set(r, off + i, __intrinsic_unit(fill, i % fl));
  }
  const soff = atStart ? n : 0;
  for (let i = 0; i < len; i++) {
    __intrinsic_str_set(r, soff + i, __intrinsic_unit(s, i));
  }
  return r;
}

function $$String_prototype_padStart(maxLength, fillString) {
  return $$pad($$thisString(this, "padStart"), maxLength, fillString, true);
}

function $$String_prototype_padEnd(maxLength, fillString) {
  return $$pad($$thisString(this, "padEnd"), maxLength, fillString, false);
}

// TrimString: where 0 = both ends, 1 = start, 2 = end.
function $$trim(s, where) {
  let a = 0;
  let b = s.length;
  if (where !== 2) {
    while (a < b && $$isSpace(__intrinsic_unit(s, a))) {
      a++;
    }
  }
  if (where !== 1) {
    while (b > a && $$isSpace(__intrinsic_unit(s, b - 1))) {
      b--;
    }
  }
  if (a === 0 && b === s.length) {
    return s;
  }
  return __intrinsic_sub(s, a, b);
}

function $$String_prototype_trim() {
  return $$trim($$thisString(this, "trim"), 0);
}

function $$String_prototype_trimStart() {
  return $$trim($$thisString(this, "trimStart"), 1);
}

function $$String_prototype_trimLeft() {
  return $$trim($$thisString(this, "trimLeft"), 1);
}

function $$String_prototype_trimEnd() {
  return $$trim($$thisString(this, "trimEnd"), 2);
}

function $$String_prototype_trimRight() {
  return $$trim($$thisString(this, "trimRight"), 2);
}

// Case mapping of one code unit (-1: not implemented, see `$$caseMap`).
function $$upperUnit(c) {
  if (c < 128) {
    return c >= 97 && c <= 122 ? c - 32 : c;
  }
  if (c < 0x100) {
    if ((c >= 0xe0 && c <= 0xfe && c !== 0xf7)) {
      return c - 32;
    }
    if (c === 0xb5) {
      return 0x39c;
    }
    if (c === 0xff) {
      return 0x178;
    }
    return c;
  }
  if (c < 0x180) {
    if (c === 0x131) {
      return 0x49;
    }
    if (c === 0x17f) {
      return 0x53;
    }
    if (c === 0x138 || c === 0x130) {
      return c;
    }
    // Pairs: upper even / lower odd, except 0x139–0x148 and 0x179–0x17e (upper odd).
    if ((c >= 0x139 && c <= 0x148) || (c >= 0x179 && c <= 0x17e)) {
      return c % 2 === 0 ? c - 1 : c;
    }
    return c % 2 === 1 ? c - 1 : c;
  }
  if (c >= 0x391 && c <= 0x3a9) {
    return c;
  }
  if (c >= 0x3b1 && c <= 0x3c9) {
    return c === 0x3c2 ? 0x3a3 : c - 32;
  }
  if (c === 0x386 || (c >= 0x388 && c <= 0x38a) || c === 0x38c || c === 0x38e || c === 0x38f) {
    return c;
  }
  if (c === 0x3ac) {
    return 0x386;
  }
  if (c >= 0x3ad && c <= 0x3af) {
    return c - 37;
  }
  if (c === 0x3cc) {
    return 0x38c;
  }
  if (c === 0x3cd || c === 0x3ce) {
    return c - 63;
  }
  if (c >= 0x400 && c <= 0x42f) {
    return c;
  }
  if (c >= 0x430 && c <= 0x44f) {
    return c - 32;
  }
  if (c >= 0x450 && c <= 0x45f) {
    return c - 80;
  }
  if (c >= 0xff41 && c <= 0xff5a) {
    return c - 32;
  }
  return $$caseless(c) ? c : -1;
}

function $$lowerUnit(c) {
  if (c < 128) {
    return c >= 65 && c <= 90 ? c + 32 : c;
  }
  if (c < 0x100) {
    if (c >= 0xc0 && c <= 0xde && c !== 0xd7) {
      return c + 32;
    }
    return c;
  }
  if (c < 0x180) {
    if (c === 0x178) {
      return 0xff;
    }
    if (c === 0x131 || c === 0x138 || c === 0x149 || c === 0x17f) {
      return c;
    }
    if ((c >= 0x139 && c <= 0x148) || (c >= 0x179 && c <= 0x17e)) {
      return c % 2 === 1 ? c + 1 : c;
    }
    return c % 2 === 0 ? c + 1 : c;
  }
  // (Σ lowercases depending on context: not implemented.)
  if (c >= 0x391 && c <= 0x3a9 && c !== 0x3a2 && c !== 0x3a3) {
    return c + 32;
  }
  if (c >= 0x3b1 && c <= 0x3c9) {
    return c;
  }
  if (c === 0x386) {
    return 0x3ac;
  }
  if (c >= 0x388 && c <= 0x38a) {
    return c + 37;
  }
  if (c === 0x38c) {
    return 0x3cc;
  }
  if (c === 0x38e || c === 0x38f) {
    return c + 63;
  }
  if (c === 0x3ac || (c >= 0x3ad && c <= 0x3af) || c === 0x3cc || c === 0x3cd || c === 0x3ce) {
    return c;
  }
  if (c >= 0x400 && c <= 0x40f) {
    return c + 80;
  }
  if (c >= 0x410 && c <= 0x42f) {
    return c + 32;
  }
  if (c >= 0x430 && c <= 0x45f) {
    return c;
  }
  if (c >= 0xff21 && c <= 0xff3a) {
    return c + 32;
  }
  return $$caseless(c) ? c : -1;
}

// Code units known to have no case mapping (outside the ranges handled above).
function $$caseless(c) {
  return (
    (c >= 0x2bb && c <= 0x36f) ||
    (c >= 0x590 && c <= 0x109f) ||
    (c >= 0x2000 && c <= 0x215f) ||
    (c >= 0x2190 && c <= 0x24b5) ||
    (c >= 0x2500 && c <= 0x2bff) ||
    (c >= 0x3000 && c <= 0xa63f) ||
    (c >= 0xac00 && c <= 0xd7ff) ||
    (c >= 0xe000 && c <= 0xfaff) ||
    (c >= 0xfe00 && c <= 0xff20) ||
    (c >= 0xff3b && c <= 0xff40) ||
    (c >= 0xff5b && c <= 0xffff)
  );
}

// toUpperCase / toLowerCase. Units outside the handled ranges trap (not implemented yet).
function $$caseMap(s, upper) {
  const len = s.length;
  const r = __intrinsic_str_new(len * 2);
  let k = 0;
  for (let i = 0; i < len; i++) {
    const c = __intrinsic_unit(s, i);
    if (c >= 0xd800 && c <= 0xdfff) {
      // A surrogate: emoji planes (U+1F000–U+1FBFF) are caseless; others aren't implemented.
      const hi = c <= 0xdbff ? c : i > 0 ? __intrinsic_unit(s, i - 1) : 0;
      if (hi < 0xd83c || hi > 0xd83e) {
        __intrinsic_trap();
      }
      __intrinsic_str_set(r, k, c);
      k++;
      continue;
    }
    if (upper && c === 0xdf) {
      // ß → SS
      __intrinsic_str_set(r, k, 83);
      __intrinsic_str_set(r, k + 1, 83);
      k += 2;
      continue;
    }
    if (upper && c === 0x149) {
      // ŉ → ʼN
      __intrinsic_str_set(r, k, 0x2bc);
      __intrinsic_str_set(r, k + 1, 78);
      k += 2;
      continue;
    }
    if (!upper && c === 0x130) {
      // İ → i̇
      __intrinsic_str_set(r, k, 105);
      __intrinsic_str_set(r, k + 1, 0x307);
      k += 2;
      continue;
    }
    const m = upper ? $$upperUnit(c) : $$lowerUnit(c);
    if (m < 0) {
      __intrinsic_trap();
    }
    __intrinsic_str_set(r, k, m);
    k++;
  }
  return __intrinsic_sub(r, 0, k);
}

function $$String_prototype_toUpperCase() {
  return $$caseMap($$thisString(this, "toUpperCase"), true);
}

function $$String_prototype_toLowerCase() {
  return $$caseMap($$thisString(this, "toLowerCase"), false);
}

function $$String_prototype_toLocaleUpperCase() {
  return $$caseMap($$thisString(this, "toLocaleUpperCase"), true);
}

function $$String_prototype_toLocaleLowerCase() {
  return $$caseMap($$thisString(this, "toLocaleLowerCase"), false);
}

// GetMethod(v, @@split / @@replace) (T27): undefined/null → undefined, else a function.
function $$symbolMethod(v, key) {
  const m = v[key];
  if (m === undefined || m === null) {
    return undefined;
  }
  if (typeof m !== "function") {
    throw new TypeError($$typeDescribe(m) + " is not a function");
  }
  return m;
}

function $$String_prototype_split(separator, limit) {
  if (this === undefined || this === null) {
    throw new TypeError("String.prototype.split called on null or undefined");
  }
  // T27: a separator with a `Symbol.split` method (a RegExp) does the splitting.
  // Only an object's method (ES2025: not a primitive's; node 24 still looks them up, T27).
  if ((typeof separator === "object" && separator !== null) || typeof separator === "function") {
    const splitter = $$symbolMethod(separator, Symbol.split);
    if (splitter !== undefined) {
      return __intrinsic_call_list(splitter, separator, [this, limit]);
    }
  }
  const s = __intrinsic_to_string(this);
  let lim = 4294967295;
  if (limit !== undefined) {
    lim = (+limit) >>> 0;
  }
  const sep = __intrinsic_to_string(separator);
  const out = [];
  if (lim === 0) {
    return out;
  }
  if (separator === undefined) {
    $$append(out, s);
    return out;
  }
  const len = s.length;
  const m = sep.length;
  if (len === 0) {
    if (m !== 0) {
      $$append(out, s);
    }
    return out;
  }
  if (m === 0) {
    for (let i = 0; i < len && out.length < lim; i++) {
      $$append(out, __intrinsic_sub(s, i, i + 1));
    }
    return out;
  }
  let p = 0;
  let q = $$indexOf(s, sep, 0);
  while (q >= 0) {
    $$append(out, __intrinsic_sub(s, p, q));
    if (out.length >= lim) {
      return out;
    }
    p = q + m;
    q = $$indexOf(s, sep, p);
  }
  $$append(out, __intrinsic_sub(s, p, len));
  return out;
}

// GetSubstitution without captures: `$$`, `$&`, `` $` ``, `$'`; anything else is literal.
function $$substitute(matched, s, position, replacement) {
  const n = replacement.length;
  if ($$indexOf(replacement, "$", 0) < 0) {
    return replacement;
  }
  let r = "";
  let from = 0;
  for (let i = 0; i + 1 < n; i++) {
    if (__intrinsic_unit(replacement, i) !== 36) {
      continue;
    }
    const c = __intrinsic_unit(replacement, i + 1);
    let add = "";
    if (c === 36) {
      add = "$";
    } else if (c === 38) {
      add = matched;
    } else if (c === 96) {
      add = __intrinsic_sub(s, 0, position);
    } else if (c === 39) {
      const end = position + matched.length;
      add = __intrinsic_sub(s, end, s.length);
    } else {
      continue;
    }
    r = r + __intrinsic_sub(replacement, from, i) + add;
    i++;
    from = i + 1;
  }
  return r + __intrinsic_sub(replacement, from, n);
}

function $$String_prototype_replace(searchValue, replaceValue) {
  if (this === undefined || this === null) {
    throw new TypeError("String.prototype.replace called on null or undefined");
  }
  // T27: a search value with a `Symbol.replace` method (a RegExp) does the replacing.
  // Only an object's method (ES2025: not a primitive's; node 24 still looks them up, T27).
  if ((typeof searchValue === "object" && searchValue !== null) || typeof searchValue === "function") {
    const replacer = $$symbolMethod(searchValue, Symbol.replace);
    if (replacer !== undefined) {
      return __intrinsic_call_list(replacer, searchValue, [this, replaceValue]);
    }
  }
  const s = __intrinsic_to_string(this);
  const t = __intrinsic_to_string(searchValue);
  const functional = typeof replaceValue === "function";
  let replacement = "";
  if (!functional) {
    replacement = __intrinsic_to_string(replaceValue);
  }
  const pos = $$indexOf(s, t, 0);
  if (pos < 0) {
    return s;
  }
  let r = "";
  if (functional) {
    r = __intrinsic_to_string(replaceValue(t, pos, s));
  } else {
    r = $$substitute(t, s, pos, replacement);
  }
  return __intrinsic_sub(s, 0, pos) + r + __intrinsic_sub(s, pos + t.length, s.length);
}

function $$String_prototype_replaceAll(searchValue, replaceValue) {
  if (this === undefined || this === null) {
    throw new TypeError("String.prototype.replaceAll called on null or undefined");
  }
  // T27: a RegExp must be global; `Symbol.replace` does the replacing.
  // Only an object's method (ES2025: not a primitive's; node 24 still looks them up, T27).
  if ((typeof searchValue === "object" && searchValue !== null) || typeof searchValue === "function") {
    $$reRequireGlobal(searchValue, "replaceAll");
    const replacer = $$symbolMethod(searchValue, Symbol.replace);
    if (replacer !== undefined) {
      return __intrinsic_call_list(replacer, searchValue, [this, replaceValue]);
    }
  }
  const s = __intrinsic_to_string(this);
  const t = __intrinsic_to_string(searchValue);
  const functional = typeof replaceValue === "function";
  let replacement = "";
  if (!functional) {
    replacement = __intrinsic_to_string(replaceValue);
  }
  const m = t.length;
  const advance = m > 0 ? m : 1;
  const positions = [];
  let p = $$indexOf(s, t, 0);
  while (p >= 0) {
    $$append(positions, p);
    p = p + advance <= s.length ? $$indexOf(s, t, p + advance) : -1;
  }
  let end = 0;
  let r = "";
  for (let i = 0; i < positions.length; i++) {
    const q = positions[i];
    let rep = "";
    if (functional) {
      rep = __intrinsic_to_string(replaceValue(t, q, s));
    } else {
      rep = $$substitute(t, s, q, replacement);
    }
    r = r + __intrinsic_sub(s, end, q) + rep;
    end = q + m;
  }
  if (end < s.length) {
    r = r + __intrinsic_sub(s, end, s.length);
  }
  return r;
}

// `localeCompare` as node's ICU root collation orders ASCII (T25): control characters are
// ignored; then, by primary weight, whitespace < punctuation and symbols (`$$collation`'s order)
// < digits < letters (case-insensitive); then the first case difference (lowercase first).
// Other characters, locales and options aren't implemented (they trap).
function $$collationWeight(c) {
  if (c >= 128) {
    __intrinsic_trap();
  }
  if (c >= 97 && c <= 122) {
    return c;
  }
  if (c >= 65 && c <= 90) {
    return c + 32;
  }
  const order = "\t\n\v\f\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789";
  for (let i = 0; i < 48; i++) {
    if (__intrinsic_unit(order, i) === c) {
      return i + 1;
    }
  }
  // Ignorable.
  return 0;
}

// The next non-ignorable index at or after `i`, or `len`.
function $$collationNext(s, i, len) {
  while (i < len && $$collationWeight(__intrinsic_unit(s, i)) === 0) {
    i++;
  }
  return i;
}

function $$String_prototype_localeCompare(that, locales, options) {
  const s = $$thisString(this, "localeCompare");
  const t = $$String(that);
  if (locales !== undefined || options !== undefined) {
    __intrinsic_trap();
  }
  const n = __intrinsic_str_len(s);
  const m = __intrinsic_str_len(t);
  let i = $$collationNext(s, 0, n);
  let j = $$collationNext(t, 0, m);
  while (i < n && j < m) {
    const a = $$collationWeight(__intrinsic_unit(s, i));
    const b = $$collationWeight(__intrinsic_unit(t, j));
    if (a !== b) {
      return a < b ? -1 : 1;
    }
    i = $$collationNext(s, i + 1, n);
    j = $$collationNext(t, j + 1, m);
  }
  if (i < n) {
    return 1;
  }
  if (j < m) {
    return -1;
  }
  // The same letters: the first case difference.
  i = $$collationNext(s, 0, n);
  j = $$collationNext(t, 0, m);
  while (i < n) {
    const a = __intrinsic_unit(s, i);
    const b = __intrinsic_unit(t, j);
    if (a !== b) {
      return a > b ? -1 : 1;
    }
    i = $$collationNext(s, i + 1, n);
    j = $$collationNext(t, j + 1, m);
  }
  return 0;
}

// `normalize(form)` (T25): strings no form changes (ASCII; below U+0300 for NFC) are returned
// as they are; the others need Unicode's tables (they trap).
function $$String_prototype_normalize(form) {
  const s = $$thisString(this, "normalize");
  let f = "NFC";
  if (form !== undefined) {
    f = $$String(form);
  }
  if (f !== "NFC" && f !== "NFD" && f !== "NFKC" && f !== "NFKD") {
    throw new RangeError("The normalization form should be one of NFC, NFD, NFKC, NFKD.");
  }
  const limit = f === "NFC" ? 768 : 128;
  const n = __intrinsic_str_len(s);
  for (let i = 0; i < n; i++) {
    if (__intrinsic_unit(s, i) >= limit) {
      __intrinsic_trap();
    }
  }
  return s;
}

function $$String_prototype_isWellFormed() {
  const s = $$thisString(this, "isWellFormed");
  const len = s.length;
  for (let i = 0; i < len; i++) {
    const c = __intrinsic_unit(s, i);
    if (c >= 0xd800 && c <= 0xdbff) {
      if (i + 1 < len) {
        const d = __intrinsic_unit(s, i + 1);
        if (d >= 0xdc00 && d <= 0xdfff) {
          i++;
          continue;
        }
      }
      return false;
    }
    if (c >= 0xdc00 && c <= 0xdfff) {
      return false;
    }
  }
  return true;
}

function $$String_prototype_toWellFormed() {
  const s = $$thisString(this, "toWellFormed");
  const len = s.length;
  const r = __intrinsic_str_new(len);
  for (let i = 0; i < len; i++) {
    const c = __intrinsic_unit(s, i);
    let u = c;
    if (c >= 0xd800 && c <= 0xdbff) {
      if (i + 1 < len) {
        const d = __intrinsic_unit(s, i + 1);
        if (d >= 0xdc00 && d <= 0xdfff) {
          __intrinsic_str_set(r, i, c);
          __intrinsic_str_set(r, i + 1, d);
          i++;
          continue;
        }
      }
      u = 0xfffd;
    } else if (c >= 0xdc00 && c <= 0xdfff) {
      u = 0xfffd;
    }
    __intrinsic_str_set(r, i, u);
  }
  return r;
}

// ---- String --------------------------------------------------------------------------------

function $$String(...args) {
  if (args.length === 0) {
    return "";
  }
  const v = args[0];
  if (typeof v === "symbol") {
    return "Symbol(" + __intrinsic_sub(v, 0, __intrinsic_str_len(v)) + ")";
  }
  return __intrinsic_to_string(v);
}

function $$String_fromCharCode(...codes) {
  const n = codes.length;
  const r = __intrinsic_str_new(n);
  for (let i = 0; i < n; i++) {
    __intrinsic_str_set(r, i, (+codes[i]) & 0xffff);
  }
  return r;
}

function $$String_fromCodePoint(...codePoints) {
  // Convert and check every argument first (once each, in order), counting the code units, then fill one string.
  const n = codePoints.length;
  let units = 0;
  for (let i = 0; i < n; i++) {
    const next = codePoints[i];
    const cp = +next;
    if (cp !== __intrinsic_trunc(cp) || cp < 0 || cp > 0x10ffff) {
      throw new RangeError("Invalid code point " + __intrinsic_to_string(next));
    }
    codePoints[i] = cp;
    units = units + (cp < 0x10000 ? 1 : 2);
  }
  const r = __intrinsic_str_new(units);
  let j = 0;
  for (let i = 0; i < n; i++) {
    const cp = +codePoints[i];
    if (cp < 0x10000) {
      __intrinsic_str_set(r, j, cp);
      j = j + 1;
    } else {
      const v = cp - 0x10000;
      __intrinsic_str_set(r, j, 0xd800 + (v >> 10));
      __intrinsic_str_set(r, j + 1, 0xdc00 + (v & 1023));
      j = j + 2;
    }
  }
  return r;
}

// ---- string → number -----------------------------------------------------------------------

// Where the last `$$parseDecimal` stopped.
let $$parse_end = 0;

function $$isDigit(c) {
  return c >= 48 && c <= 57;
}

// The value of a digit in radix 36 (99: not a digit).
function $$digitValue(c) {
  if (c >= 48 && c <= 57) {
    return c - 48;
  }
  if (c >= 97 && c <= 122) {
    return c - 87;
  }
  if (c >= 65 && c <= 90) {
    return c - 55;
  }
  return 99;
}

// 10^n for n in [0, 22] (exact).
function $$pow10(n) {
  let p = 1;
  for (let i = 0; i < n; i++) {
    p = p * 10;
  }
  return p;
}

// A StrDecimalLiteral (optional sign, `Infinity`, digits, `.`, exponent) at `i` in s[..end]:
// its value; `$$parse_end` is where it ends (`i` and NaN if there is none).
function $$parseDecimal(s, i, end) {
  const begin = i;
  let neg = false;
  if (i < end) {
    const c = __intrinsic_unit(s, i);
    if (c === 43 || c === 45) {
      neg = c === 45;
      i++;
    }
  }
  if ($$occursAt(s, "Infinity", i) && i + 8 <= end) {
    $$parse_end = i + 8;
    return neg ? -Infinity : Infinity;
  }
  const a0 = i;
  while (i < end && $$isDigit(__intrinsic_unit(s, i))) {
    i++;
  }
  const a1 = i;
  let b0 = i;
  let b1 = i;
  if (i < end && __intrinsic_unit(s, i) === 46) {
    i++;
    b0 = i;
    while (i < end && $$isDigit(__intrinsic_unit(s, i))) {
      i++;
    }
    b1 = i;
  }
  if (a0 === a1 && b0 === b1) {
    $$parse_end = begin;
    return NaN;
  }
  let exp = 0;
  if (i < end && (__intrinsic_unit(s, i) === 101 || __intrinsic_unit(s, i) === 69)) {
    let j = i + 1;
    let eneg = false;
    if (j < end) {
      const c = __intrinsic_unit(s, j);
      if (c === 43 || c === 45) {
        eneg = c === 45;
        j++;
      }
    }
    if (j < end && $$isDigit(__intrinsic_unit(s, j))) {
      let e = 0;
      while (j < end && $$isDigit(__intrinsic_unit(s, j))) {
        if (e < 100000) {
          e = e * 10 + (__intrinsic_unit(s, j) - 48);
        }
        j++;
      }
      exp = eneg ? -e : e;
      i = j;
    }
  }
  $$parse_end = i;
  const v = $$decimalToDouble(s, a0, a1, b0, b1, exp);
  return neg ? -v : v;
}

// The double nearest to the digits s[a0..a1) s[b0..b1) × 10^(exp - (b1 - b0)) (ties to even).
function $$decimalToDouble(s, a0, a1, b0, b1, exp) {
  const ds = [];
  let nd = 0;
  for (let i = a0; i < b1; i++) {
    if (i === a1) {
      i = b0;
      if (i >= b1) {
        break;
      }
    }
    const d = __intrinsic_unit(s, i) - 48;
    if (nd > 0 || d !== 0) {
      $$append(ds, d);
      nd++;
    }
  }
  let e = exp - (b1 - b0);
  while (nd > 0 && ds[nd - 1] === 0) {
    nd--;
    e++;
  }
  if (nd === 0) {
    return 0;
  }
  if (nd + e > 310) {
    return Infinity;
  }
  if (nd + e < -324) {
    return 0;
  }
  if (nd <= 15) {
    let d = 0;
    for (let i = 0; i < nd; i++) {
      d = d * 10 + +ds[i];
    }
    if (e >= 0 && e <= 22) {
      return d * $$pow10(e);
    }
    if (e < 0 && e >= -22) {
      return d / $$pow10(-e);
    }
    if (e > 22 && e <= 37 - nd) {
      return d * $$pow10(e - 22) * 1e22;
    }
  }
  // A guess from the first 17 digits, then exact big-integer comparisons (Clinger's AlgorithmR).
  const used = nd < 17 ? nd : 17;
  let g = 0;
  for (let i = 0; i < used; i++) {
    g = g * 10 + +ds[i];
  }
  const ge = e + (nd - used);
  if (ge >= 0) {
    g = g * 10 ** ge;
  } else if (ge >= -300) {
    g = g / 10 ** -ge;
  } else {
    g = g / 1e300 / 10 ** (-ge - 300);
  }
  return $$refine(ds, nd, e, g);
}

// ---- big integers: little-endian arrays of base-2^24 limbs ---------------------------------

function $$bigFrom(x) {
  const a = [];
  while (x > 0) {
    const q = __intrinsic_floor(x / 16777216);
    $$append(a, x - q * 16777216);
    x = q;
  }
  return a;
}

// a = a * m + c (m, c < 2^24).
function $$bigMulAdd(a, m, c) {
  const n = a.length;
  for (let i = 0; i < n; i++) {
    const t = a[i] * m + c;
    c = __intrinsic_floor(t / 16777216);
    __intrinsic_define(a, i, t - c * 16777216);
  }
  while (c > 0) {
    const q = __intrinsic_floor(c / 16777216);
    $$append(a, c - q * 16777216);
    c = q;
  }
}

function $$bigMulPow2(a, n) {
  while (n >= 24) {
    $$bigMulAdd(a, 16777216, 0);
    n -= 24;
  }
  if (n > 0) {
    $$bigMulAdd(a, 2 ** n, 0);
  }
}

function $$bigMulPow10(a, n) {
  while (n >= 7) {
    $$bigMulAdd(a, 10000000, 0);
    n -= 7;
  }
  if (n > 0) {
    $$bigMulAdd(a, $$pow10(n), 0);
  }
}

// a = floor(a / d) (d <= 2^24); returns the remainder.
function $$bigDivSmall(a, d) {
  let r = 0;
  for (let i = a.length - 1; i >= 0; i--) {
    const t = r * 16777216 + +a[i];
    const q = __intrinsic_floor(t / d);
    __intrinsic_define(a, i, q);
    r = t - q * d;
  }
  let n = a.length;
  while (n > 0 && a[n - 1] === 0) {
    a.pop();
    n--;
  }
  return r;
}

function $$bigCopy(a) {
  const b = [];
  for (let i = 0; i < a.length; i++) {
    $$append(b, a[i]);
  }
  return b;
}

function $$bigLen(a) {
  let n = a.length;
  while (n > 0 && a[n - 1] === 0) {
    n--;
  }
  return n;
}

// Compare a and b: -1, 0 or 1.
function $$bigCmp(a, b) {
  const n = $$bigLen(a);
  const m = $$bigLen(b);
  if (n !== m) {
    return n < m ? -1 : 1;
  }
  for (let i = n - 1; i >= 0; i--) {
    if (a[i] !== b[i]) {
      return a[i] < b[i] ? -1 : 1;
    }
  }
  return 0;
}

// The next double above / below a positive x.
function $$nextUp(x) {
  const hi = __intrinsic_hi(x);
  const lo = __intrinsic_lo(x);
  if (lo === -1) {
    return __intrinsic_from_words(hi + 1, 0);
  }
  return __intrinsic_from_words(hi, lo + 1);
}

function $$nextDown(x) {
  const hi = __intrinsic_hi(x);
  const lo = __intrinsic_lo(x);
  if (lo === 0) {
    return __intrinsic_from_words(hi - 1, -1);
  }
  return __intrinsic_from_words(hi, lo - 1);
}

// The double nearest to ds[0..nd) × 10^e, starting from the guess x.
function $$refine(ds, nd, e, x) {
  const D = [];
  for (let i = 0; i < nd; i++) {
    $$bigMulAdd(D, 10, +ds[i]);
  }
  if (x === Infinity) {
    x = 1.7976931348623157e308;
  }
  if (x === 0) {
    x = 5e-324;
  }
  const e10 = e > 0 ? e : 0;
  const d10 = e < 0 ? -e : 0;
  for (;;) {
    const hi = __intrinsic_hi(x);
    const be = (hi >>> 20) & 0x7ff;
    const top = hi & 0xfffff;
    const low = __intrinsic_lo(x) >>> 0;
    let m = top * 4294967296 + low;
    let k = -1074;
    if (be !== 0) {
      m = m + 4503599627370496;
      k = be - 1075;
    }
    // x's neighbours' midpoints, all scaled to integers: X = D·10^e10·2^s, H = (2m+1)·10^d10·2^t.
    const s = k < 2 ? 2 - k : 0;
    const t = k - 1 + s;
    const X = $$bigCopy(D);
    $$bigMulPow10(X, e10);
    $$bigMulPow2(X, s);
    const H = $$bigFrom(m);
    $$bigMulAdd(H, 2, 1);
    $$bigMulPow10(H, d10);
    $$bigMulPow2(H, t);
    const odd = m % 2 === 1;
    const c = $$bigCmp(X, H);
    if (c > 0 || (c === 0 && odd)) {
      if (x === 1.7976931348623157e308) {
        return Infinity;
      }
      x = $$nextUp(x);
      continue;
    }
    let L;
    if (m === 4503599627370496 && be > 1) {
      // A power of two: the gap below is half as wide.
      L = $$bigFrom(m - 1);
      $$bigMulAdd(L, 4, 3);
      $$bigMulPow10(L, d10);
      $$bigMulPow2(L, t - 1);
    } else {
      L = $$bigFrom(m - 1);
      $$bigMulAdd(L, 2, 1);
      $$bigMulPow10(L, d10);
      $$bigMulPow2(L, t);
    }
    const d = $$bigCmp(X, L);
    if (d < 0 || (d === 0 && odd)) {
      x = $$nextDown(x);
      if (x === 0) {
        return 0;
      }
      continue;
    }
    return x;
  }
  return x;
}

// ToNumber of a string (StringToNumber).
function $$StringToNumber(s) {
  let start = 0;
  let end = s.length;
  while (start < end && $$isSpace(__intrinsic_unit(s, start))) {
    start++;
  }
  while (end > start && $$isSpace(__intrinsic_unit(s, end - 1))) {
    end--;
  }
  if (start === end) {
    return 0;
  }
  if (end - start > 2 && __intrinsic_unit(s, start) === 48) {
    const c = __intrinsic_unit(s, start + 1);
    let radix = 0;
    if (c === 120 || c === 88) {
      radix = 16;
    } else if (c === 111 || c === 79) {
      radix = 8;
    } else if (c === 98 || c === 66) {
      radix = 2;
    }
    if (radix !== 0) {
      let v = 0;
      for (let i = start + 2; i < end; i++) {
        const d = $$digitValue(__intrinsic_unit(s, i));
        if (d >= radix) {
          return NaN;
        }
        v = v * radix + d;
      }
      return v;
    }
  }
  const v = $$parseDecimal(s, start, end);
  if ($$parse_end !== end) {
    return NaN;
  }
  return v;
}

function $$parseFloat(string) {
  const s = __intrinsic_to_string(string);
  const end = s.length;
  let i = 0;
  while (i < end && $$isSpace(__intrinsic_unit(s, i))) {
    i++;
  }
  const v = $$parseDecimal(s, i, end);
  if ($$parse_end === i) {
    return NaN;
  }
  return v;
}

function $$parseInt(string, radix) {
  const s = __intrinsic_to_string(string);
  const end = s.length;
  let i = 0;
  while (i < end && $$isSpace(__intrinsic_unit(s, i))) {
    i++;
  }
  let sign = 1;
  if (i < end) {
    const c = __intrinsic_unit(s, i);
    if (c === 45) {
      sign = -1;
      i++;
    } else if (c === 43) {
      i++;
    }
  }
  let r = (+radix) | 0;
  let strip = true;
  if (r !== 0) {
    if (r < 2 || r > 36) {
      return NaN;
    }
    if (r !== 16) {
      strip = false;
    }
  } else {
    r = 10;
  }
  if (strip && i + 1 < end && __intrinsic_unit(s, i) === 48) {
    const c = __intrinsic_unit(s, i + 1);
    if (c === 120 || c === 88) {
      i += 2;
      r = 16;
    }
  }
  const start = i;
  while (i < end && $$digitValue(__intrinsic_unit(s, i)) < r) {
    i++;
  }
  if (i === start) {
    return NaN;
  }
  if (r === 10) {
    return sign * $$decimalToDouble(s, start, i, i, i, 0);
  }
  let v = 0;
  for (let j = start; j < i; j++) {
    v = v * r + $$digitValue(__intrinsic_unit(s, j));
  }
  return sign * v;
}

function $$isNaN(number) {
  const x = +number;
  return x !== x;
}

function $$isFinite(number) {
  const x = +number;
  return x - x === 0;
}

// ---- Number --------------------------------------------------------------------------------

function $$Number(...args) {
  if (args.length === 0) {
    return 0;
  }
  return +args[0];
}

function $$Number_isNaN(number) {
  return typeof number === "number" && number !== number;
}

function $$Number_isFinite(number) {
  return typeof number === "number" && number - number === 0;
}

function $$Number_isInteger(number) {
  return typeof number === "number" && number - number === 0 && __intrinsic_trunc(number) === number;
}

function $$Number_isSafeInteger(number) {
  return (
    typeof number === "number" &&
    number - number === 0 &&
    __intrinsic_trunc(number) === number &&
    __intrinsic_abs(number) <= 9007199254740991
  );
}

// thisNumberValue.
function $$thisNumber(v, name) {
  v = __intrinsic_unwrap(v);
  if (typeof v !== "number") {
    throw new TypeError("Number.prototype." + name + " requires that 'this' be a Number");
  }
  return +v;
}

function $$Number_prototype_valueOf() {
  return $$thisNumber(this, "valueOf");
}

function $$Number_prototype_toString(radix) {
  const x = $$thisNumber(this, "toString");
  let r = 10;
  if (radix !== undefined) {
    r = $$toInteger(radix);
  }
  if (r < 2 || r > 36) {
    throw new RangeError("toString() radix argument must be between 2 and 36");
  }
  if (r === 10) {
    return __intrinsic_number_to_string(x);
  }
  return $$doubleToRadix(x, r);
}

function $$digitChar(d) {
  return d < 10 ? 48 + d : 87 + d;
}

// V8's DoubleToRadixCString: the shortest digits that read back to the same double.
function $$doubleToRadix(value, radix) {
  if (value !== value) {
    return "NaN";
  }
  if (value === Infinity) {
    return "Infinity";
  }
  if (value === -Infinity) {
    return "-Infinity";
  }
  const buf = __intrinsic_str_new(2200);
  let ic = 1100;
  let fc = 1100;
  const negative = value < 0;
  if (negative) {
    value = -value;
  }
  let integer = __intrinsic_floor(value);
  let fraction = value - integer;
  let delta = 0.5 * ($$nextUp(value) - value);
  if (delta < 5e-324) {
    delta = 5e-324;
  }
  if (fraction >= delta) {
    __intrinsic_str_set(buf, fc, 46);
    fc++;
    do {
      fraction = fraction * radix;
      delta = delta * radix;
      const digit = __intrinsic_floor(fraction);
      __intrinsic_str_set(buf, fc, $$digitChar(digit));
      fc++;
      fraction = fraction - digit;
      if (fraction > 0.5 || (fraction === 0.5 && digit % 2 === 1)) {
        if (fraction + delta > 1) {
          // Round up, carrying into earlier digits (and the integer part).
          for (;;) {
            fc--;
            if (fc === 1100) {
              integer = integer + 1;
              break;
            }
            const c = __intrinsic_unit(buf, fc);
            const d = c > 57 ? c - 87 : c - 48;
            if (d + 1 < radix) {
              __intrinsic_str_set(buf, fc, $$digitChar(d + 1));
              fc++;
              break;
            }
          }
          break;
        }
      }
    } while (fraction >= delta);
  }
  // Integer digits; those below the double's precision are 0.
  while (integer / radix >= 9007199254740992) {
    integer = integer / radix;
    ic--;
    __intrinsic_str_set(buf, ic, 48);
  }
  do {
    const rem = integer % radix;
    ic--;
    __intrinsic_str_set(buf, ic, $$digitChar(rem));
    integer = (integer - rem) / radix;
  } while (integer > 0);
  if (negative) {
    ic--;
    __intrinsic_str_set(buf, ic, 45);
  }
  return __intrinsic_sub(buf, ic, fc);
}

// The decimal digits of a big integer.
function $$bigToDecimal(a) {
  const b = $$bigCopy(a);
  let r = "";
  for (;;) {
    const chunk = $$bigDivSmall(b, 10000000);
    const digits = __intrinsic_number_to_string(chunk);
    if ($$bigLen(b) === 0) {
      return digits + r;
    }
    r = __intrinsic_sub("0000000", 0, 7 - digits.length) + digits + r;
  }
  return r;
}

// The exact decimal expansion of a finite x > 0 (T25): [digits without leading zeros, e] with
// x = 0.digits × 10^e.
function $$exactDigits(x) {
  const hi = __intrinsic_hi(x);
  const be = (hi >>> 20) & 0x7ff;
  let m = (hi & 0xfffff) * 4294967296 + (__intrinsic_lo(x) >>> 0);
  let k = -1074;
  if (be !== 0) {
    m = m + 4503599627370496;
    k = be - 1075;
  }
  const n = $$bigFrom(m);
  if (k >= 0) {
    $$bigMulPow2(n, k);
  } else {
    // x = m·5^-k / 10^-k.
    let q = -k;
    while (q >= 10) {
      $$bigMulAdd(n, 9765625, 0);
      q -= 10;
    }
    while (q > 0) {
      $$bigMulAdd(n, 5, 0);
      q--;
    }
  }
  const digits = $$bigToDecimal(n);
  return [digits, digits.length + (k < 0 ? k : 0)];
}

// The shortest digits that round-trip (Number::toString's), as [digits, e] with
// x = 0.digits × 10^e, for a finite x > 0.
function $$shortestDigits(x) {
  const s = __intrinsic_number_to_string(x);
  const len = __intrinsic_str_len(s);
  let e = 0;
  let end = len;
  for (let i = 0; i < len; i++) {
    if (__intrinsic_unit(s, i) === 101) {
      end = i;
      e = +__intrinsic_sub(s, i + 1, len);
    }
  }
  let digits = "";
  let point = end;
  for (let i = 0; i < end; i++) {
    const c = __intrinsic_unit(s, i);
    if (c === 46) {
      point = i;
    } else {
      digits += __intrinsic_sub(s, i, i + 1);
    }
  }
  let lead = 0;
  while (__intrinsic_unit(digits, lead) === 48) {
    lead++;
  }
  let tail = __intrinsic_str_len(digits);
  while (tail > lead + 1 && __intrinsic_unit(digits, tail - 1) === 48) {
    tail--;
  }
  return [__intrinsic_sub(digits, lead, tail), point - lead + e];
}

// `digits` rounded to p digits (ties: up, which is the larger n for an exact expansion), padded
// with zeros: [p digits, e + 1 if it carried into a new digit].
function $$roundDigits(digits, e, p) {
  const len = __intrinsic_str_len(digits);
  const out = __intrinsic_str_new(p);
  for (let i = 0; i < p; i++) {
    __intrinsic_str_set(out, i, i < len ? __intrinsic_unit(digits, i) : 48);
  }
  if (p < len && __intrinsic_unit(digits, p) >= 53) {
    let i = p - 1;
    while (i >= 0 && __intrinsic_unit(out, i) === 57) {
      __intrinsic_str_set(out, i, 48);
      i--;
    }
    if (i < 0) {
      __intrinsic_str_set(out, 0, 49);
      return [out, e + 1];
    }
    __intrinsic_str_set(out, i, __intrinsic_unit(out, i) + 1);
  }
  return [out, e];
}

function $$zeros(n) {
  const z = __intrinsic_str_new(n);
  for (let i = 0; i < n; i++) {
    __intrinsic_str_set(z, i, 48);
  }
  return z;
}

// d.ddd e±n from p digits and the exponent of the first one.
function $$expForm(digits, e) {
  const p = __intrinsic_str_len(digits);
  let r = __intrinsic_sub(digits, 0, 1);
  if (p > 1) {
    r += "." + __intrinsic_sub(digits, 1, p);
  }
  return r + (e < 0 ? "e-" + $$String(-e) : "e+" + $$String(e));
}

function $$Number_prototype_toExponential(fractionDigits) {
  let x = $$thisNumber(this, "toExponential");
  const f = $$toInteger(fractionDigits);
  if (x !== x || x === Infinity || x === -Infinity) {
    return $$String(x);
  }
  if (f < 0 || f > 100) {
    throw new RangeError("toExponential() argument must be between 0 and 100");
  }
  let sign = "";
  if (x < 0) {
    sign = "-";
    x = -x;
  }
  if (x === 0) {
    return sign + $$expForm($$zeros(fractionDigits === undefined ? 1 : f + 1), 0);
  }
  if (fractionDigits === undefined) {
    const d = $$shortestDigits(x);
    return sign + $$expForm(d[0], d[1] - 1);
  }
  const d = $$exactDigits(x);
  const r = $$roundDigits(d[0], d[1], f + 1);
  return sign + $$expForm(r[0], r[1] - 1);
}

function $$Number_prototype_toPrecision(precision) {
  let x = $$thisNumber(this, "toPrecision");
  if (precision === undefined) {
    return $$String(x);
  }
  const p = $$toInteger(precision);
  if (x !== x || x === Infinity || x === -Infinity) {
    return $$String(x);
  }
  if (p < 1 || p > 100) {
    throw new RangeError("toPrecision() argument must be between 1 and 100");
  }
  let sign = "";
  if (x < 0) {
    sign = "-";
    x = -x;
  }
  let m;
  let e;
  if (x === 0) {
    m = $$zeros(p);
    e = 0;
  } else {
    const d = $$exactDigits(x);
    const r = $$roundDigits(d[0], d[1], p);
    m = r[0];
    e = r[1] - 1;
  }
  if (e < -6 || e >= p) {
    return sign + $$expForm(m, e);
  }
  if (e === p - 1) {
    return sign + m;
  }
  if (e >= 0) {
    return sign + __intrinsic_sub(m, 0, e + 1) + "." + __intrinsic_sub(m, e + 1, p);
  }
  return sign + "0." + $$zeros(-(e + 1)) + m;
}

// `toLocaleString()` as node's default locale (en-US) prints: at most 3 fraction digits, the
// integer digits grouped by 3 (T25). Locales and options aren't implemented (they trap).
function $$Number_prototype_toLocaleString(locales, options) {
  const x = $$thisNumber(this, "toLocaleString");
  if (locales !== undefined || options !== undefined) {
    __intrinsic_trap();
  }
  if (x !== x) {
    return "NaN";
  }
  if (x === Infinity) {
    return "∞";
  }
  if (x === -Infinity) {
    return "-∞";
  }
  if (x >= 1e21 || x <= -1e21) {
    __intrinsic_trap();
  }
  let s = x.toFixed(3);
  if (x === 0 && 1 / x < 0) {
    s = "-" + s;
  }
  let end = s.length;
  while (s[end - 1] === "0") {
    end--;
  }
  if (s[end - 1] === ".") {
    end--;
  }
  s = s.slice(0, end);
  let start = s[0] === "-" ? 1 : 0;
  let dot = s.indexOf(".");
  if (dot < 0) {
    dot = s.length;
  }
  let out = s.slice(dot);
  let i = dot;
  while (i - start > 3) {
    out = "," + s.slice(i - 3, i) + out;
    i -= 3;
  }
  return s.slice(0, i) + out;
}

function $$Number_prototype_toFixed(fractionDigits) {
  let x = $$thisNumber(this, "toFixed");
  const f = $$toInteger(fractionDigits);
  if (f < 0 || f > 100 || f === Infinity) {
    throw new RangeError("toFixed() digits argument must be between 0 and 100");
  }
  if (x !== x) {
    return "NaN";
  }
  if (x >= 1e21 || x <= -1e21) {
    return __intrinsic_number_to_string(x);
  }
  let sign = "";
  if (x < 0) {
    sign = "-";
    x = -x;
  }
  // x = m·2^k exactly; n = the integer nearest to x·10^f (ties: the larger).
  const hi = __intrinsic_hi(x);
  const be = (hi >>> 20) & 0x7ff;
  let m = (hi & 0xfffff) * 4294967296 + (__intrinsic_lo(x) >>> 0);
  let k = -1074;
  if (be !== 0) {
    m = m + 4503599627370496;
    k = be - 1075;
  }
  const n = $$bigFrom(m);
  if (k >= 0) {
    $$bigMulPow2(n, k);
    $$bigMulPow10(n, f);
  } else {
    $$bigMulPow10(n, f);
    let q = -k - 1;
    while (q >= 24) {
      $$bigDivSmall(n, 16777216);
      q -= 24;
    }
    if (q > 0) {
      $$bigDivSmall(n, 2 ** q);
    }
    $$bigMulAdd(n, 1, 1);
    $$bigDivSmall(n, 2);
  }
  let digits = $$bigToDecimal(n);
  if (f === 0) {
    return sign + digits;
  }
  if (digits.length <= f) {
    const z = __intrinsic_str_new(f + 1 - digits.length);
    for (let i = 0; i < f + 1 - digits.length; i++) {
      __intrinsic_str_set(z, i, 48);
    }
    digits = z + digits;
  }
  const point = digits.length - f;
  return sign + __intrinsic_sub(digits, 0, point) + "." + __intrinsic_sub(digits, point, digits.length);
}
