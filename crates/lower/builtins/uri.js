// URI handling functions (T53): `encodeURIComponent`, `encodeURI`, `decodeURIComponent`,
// `decodeURI` (ECMA-262 19.2.6: Encode/Decode over UTF-8, `URIError: URI malformed` as in V8), and
// Annex B's `escape`/`unescape`. Uses string.js's `$$fromUnit`.

// A hex digit's value, or -1.
function $$hexValue(u) {
  if (u >= 48 && u <= 57) {
    return u - 48;
  }
  if (u >= 65 && u <= 70) {
    return u - 55;
  }
  if (u >= 97 && u <= 102) {
    return u - 87;
  }
  return -1;
}

// An uppercase hex digit (0–15) as a code unit.
function $$hexUnit(d) {
  return d < 10 ? 48 + d : 55 + d;
}

// `%XX` for a byte.
function $$percentByte(b) {
  const r = __intrinsic_str_new(3);
  __intrinsic_str_set(r, 0, 37);
  __intrinsic_str_set(r, 1, $$hexUnit(b >> 4));
  __intrinsic_str_set(r, 2, $$hexUnit(b & 15));
  return r;
}

// Is the code unit `u` left as it is by Encode: unreserved (ALPHA, DIGIT, `-_.!~*'()`), plus
// with `reserved` also `;/?:@&=+$,#`?
function $$uriUnescaped(u, reserved) {
  if ((u >= 97 && u <= 122) || (u >= 65 && u <= 90) || (u >= 48 && u <= 57)) {
    return true;
  }
  // - _ . ! ~ * ' ( )
  if (u === 45 || u === 95 || u === 46 || u === 33 || u === 126 || u === 42 || u === 39 || u === 40 || u === 41) {
    return true;
  }
  // ; / ? : @ & = + $ , #
  return (
    reserved &&
    (u === 59 || u === 47 || u === 63 || u === 58 || u === 64 || u === 38 || u === 61 || u === 43 || u === 36 ||
      u === 44 || u === 35)
  );
}

function $$uriError() {
  return new URIError("URI malformed");
}

// Encode(string, extraUnescaped).
function $$uriEncode(v, reserved) {
  const s = __intrinsic_to_string(v);
  const len = __intrinsic_str_len(s);
  let r = "";
  let run = 0;
  let k = 0;
  while (k < len) {
    const u = __intrinsic_unit(s, k);
    if ($$uriUnescaped(u, reserved)) {
      k++;
      continue;
    }
    if (run < k) {
      r += __intrinsic_sub(s, run, k);
    }
    let cp = u;
    if (u >= 0xdc00 && u <= 0xdfff) {
      throw $$uriError();
    }
    if (u >= 0xd800 && u <= 0xdbff) {
      const next = k + 1 < len ? __intrinsic_unit(s, k + 1) : 0;
      if (next < 0xdc00 || next > 0xdfff) {
        throw $$uriError();
      }
      cp = 0x10000 + ((u - 0xd800) << 10) + (next - 0xdc00);
      k++;
    }
    if (cp < 0x80) {
      r += $$percentByte(cp);
    } else if (cp < 0x800) {
      r += $$percentByte(0xc0 | (cp >> 6));
      r += $$percentByte(0x80 | (cp & 63));
    } else if (cp < 0x10000) {
      r += $$percentByte(0xe0 | (cp >> 12));
      r += $$percentByte(0x80 | ((cp >> 6) & 63));
      r += $$percentByte(0x80 | (cp & 63));
    } else {
      r += $$percentByte(0xf0 | (cp >> 18));
      r += $$percentByte(0x80 | ((cp >> 12) & 63));
      r += $$percentByte(0x80 | ((cp >> 6) & 63));
      r += $$percentByte(0x80 | (cp & 63));
    }
    k++;
    run = k;
  }
  if (run === 0) {
    return s;
  }
  if (run < len) {
    r += __intrinsic_sub(s, run, len);
  }
  return r;
}

// The byte `%XX` at `k` of `s`, or -1.
function $$uriByte(s, k, len) {
  if (k + 2 >= len || __intrinsic_unit(s, k) !== 37) {
    return -1;
  }
  const hi = $$hexValue(__intrinsic_unit(s, k + 1));
  const lo = $$hexValue(__intrinsic_unit(s, k + 2));
  if (hi < 0 || lo < 0) {
    return -1;
  }
  return (hi << 4) | lo;
}

// Decode(string, preserveEscapeSet): with `reserved`, escapes of `;/?:@&=+$,#` stay as they are.
function $$uriDecode(v, reserved) {
  const s = __intrinsic_to_string(v);
  const len = __intrinsic_str_len(s);
  let r = "";
  let run = 0;
  let k = 0;
  while (k < len) {
    if (__intrinsic_unit(s, k) !== 37) {
      k++;
      continue;
    }
    const start = k;
    const b = $$uriByte(s, k, len);
    if (b < 0) {
      throw $$uriError();
    }
    k += 3;
    let piece;
    if (b < 0x80) {
      if ($$uriUnescaped(b, reserved) && !$$uriUnescaped(b, false)) {
        // A reserved character stays escaped.
        continue;
      }
      piece = $$fromUnit(b);
    } else {
      // The number of leading 1 bits: the sequence's length.
      let n = 0;
      if ((b & 0xe0) === 0xc0) {
        n = 2;
      } else if ((b & 0xf0) === 0xe0) {
        n = 3;
      } else if ((b & 0xf8) === 0xf0) {
        n = 4;
      } else {
        throw $$uriError();
      }
      let cp = b & (0xff >> (n + 1));
      for (let j = 1; j < n; j++) {
        const c = $$uriByte(s, k, len);
        if (c < 0) {
          throw $$uriError();
        }
        if ((c & 0xc0) !== 0x80) {
          throw $$uriError();
        }
        cp = (cp << 6) | (c & 63);
        k += 3;
      }
      // Shortest form, no surrogates, at most U+10FFFF.
      const min = n === 2 ? 0x80 : n === 3 ? 0x800 : 0x10000;
      if (cp < min || cp > 0x10ffff || (cp >= 0xd800 && cp <= 0xdfff)) {
        throw $$uriError();
      }
      if (cp < 0x10000) {
        piece = $$fromUnit(cp);
      } else {
        const p = __intrinsic_str_new(2);
        __intrinsic_str_set(p, 0, 0xd800 + ((cp - 0x10000) >> 10));
        __intrinsic_str_set(p, 1, 0xdc00 + ((cp - 0x10000) & 0x3ff));
        piece = p;
      }
    }
    if (run < start) {
      r += __intrinsic_sub(s, run, start);
    }
    r += piece;
    run = k;
  }
  if (run === 0) {
    return s;
  }
  if (run < len) {
    r += __intrinsic_sub(s, run, len);
  }
  return r;
}

function $$encodeURIComponent(uriComponent) {
  return $$uriEncode(uriComponent, false);
}

function $$encodeURI(uri) {
  return $$uriEncode(uri, true);
}

function $$decodeURIComponent(encodedURIComponent) {
  return $$uriDecode(encodedURIComponent, false);
}

function $$decodeURI(encodedURI) {
  return $$uriDecode(encodedURI, true);
}

// ---- Annex B -----------------------------------------------------------------------------------

// B.2.1.1 escape(string): ALPHA, DIGIT and `@*_+-./` stay; other units below 256 are `%XX`, the
// rest `%uXXXX`.
function $$escape(string) {
  const s = __intrinsic_to_string(string);
  const len = __intrinsic_str_len(s);
  let r = "";
  let run = 0;
  for (let k = 0; k < len; k++) {
    const u = __intrinsic_unit(s, k);
    if (
      (u >= 97 && u <= 122) ||
      (u >= 65 && u <= 90) ||
      (u >= 48 && u <= 57) ||
      u === 64 ||
      u === 42 ||
      u === 95 ||
      u === 43 ||
      u === 45 ||
      u === 46 ||
      u === 47
    ) {
      continue;
    }
    if (run < k) {
      r += __intrinsic_sub(s, run, k);
    }
    if (u < 256) {
      r += $$percentByte(u);
    } else {
      const e = __intrinsic_str_new(6);
      __intrinsic_str_set(e, 0, 37);
      __intrinsic_str_set(e, 1, 117);
      __intrinsic_str_set(e, 2, $$hexUnit(u >> 12));
      __intrinsic_str_set(e, 3, $$hexUnit((u >> 8) & 15));
      __intrinsic_str_set(e, 4, $$hexUnit((u >> 4) & 15));
      __intrinsic_str_set(e, 5, $$hexUnit(u & 15));
      r += e;
    }
    run = k + 1;
  }
  if (run === 0) {
    return s;
  }
  if (run < len) {
    r += __intrinsic_sub(s, run, len);
  }
  return r;
}

// B.2.1.2 unescape(string): `%uXXXX` and `%XX` become the code unit; anything else stays.
function $$unescape(string) {
  const s = __intrinsic_to_string(string);
  const len = __intrinsic_str_len(s);
  let r = "";
  let run = 0;
  let k = 0;
  while (k < len) {
    if (__intrinsic_unit(s, k) !== 37) {
      k++;
      continue;
    }
    let u = -1;
    let n = 0;
    if (k + 6 <= len && __intrinsic_unit(s, k + 1) === 117) {
      const a = $$hexValue(__intrinsic_unit(s, k + 2));
      const b = $$hexValue(__intrinsic_unit(s, k + 3));
      const c = $$hexValue(__intrinsic_unit(s, k + 4));
      const d = $$hexValue(__intrinsic_unit(s, k + 5));
      if (a >= 0 && b >= 0 && c >= 0 && d >= 0) {
        u = (a << 12) | (b << 8) | (c << 4) | d;
        n = 6;
      }
    }
    if (u < 0 && k + 3 <= len) {
      const a = $$hexValue(__intrinsic_unit(s, k + 1));
      const b = $$hexValue(__intrinsic_unit(s, k + 2));
      if (a >= 0 && b >= 0) {
        u = (a << 4) | b;
        n = 3;
      }
    }
    if (u < 0) {
      k++;
      continue;
    }
    if (run < k) {
      r += __intrinsic_sub(s, run, k);
    }
    r += $$fromUnit(u);
    k += n;
    run = k;
  }
  if (run === 0) {
    return s;
  }
  if (run < len) {
    r += __intrinsic_sub(s, run, len);
  }
  return r;
}
