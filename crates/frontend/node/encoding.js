// node:internal/encoding (T64): `TextEncoder` and `TextDecoder`, node's globals bound by the linker
// (`node::GLOBALS`), the WHATWG Encoding Standard for UTF-8 (both ways), UTF-16LE/BE and
// windows-1252 (`latin1`, `ascii`, … decode). Differences from node: its other encodings (ICU's
// legacy ones) are "not supported".
'use strict';

const inspectCustom = Symbol.for('nodejs.util.inspect.custom');

function codeError(Ctor, code, message) {
  const e = new Ctor(message);
  e.code = code;
  return e;
}

// The labels of the encodings supported here (WHATWG "Encodings" table).
const LABELS = {
  'utf-8': ['unicode-1-1-utf-8', 'unicode11utf8', 'unicode20utf8', 'utf-8', 'utf8', 'x-unicode20utf8'],
  'utf-16le': ['csunicode', 'iso-10646-ucs-2', 'ucs-2', 'unicode', 'unicodefeff', 'utf-16', 'utf-16le'],
  'utf-16be': ['unicodefffe', 'utf-16be'],
  'windows-1252': [
    'ansi_x3.4-1968', 'ascii', 'cp1252', 'cp819', 'csisolatin1', 'ibm819', 'iso-8859-1', 'iso-ir-100',
    'iso8859-1', 'iso88591', 'iso_8859-1', 'iso_8859-1:1987', 'l1', 'latin1', 'us-ascii', 'windows-1252',
    'x-cp1252',
  ],
};

function encodingOf(label) {
  const l = `${label}`.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '').toLowerCase();
  for (const name of Object.keys(LABELS)) {
    if (LABELS[name].includes(l)) return name;
  }
  return undefined;
}

// windows-1252's 0x80–0x9F.
const W1252 = [
  0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039, 0x152, 0x8d, 0x17d,
  0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x2dc, 0x2122, 0x161, 0x203a, 0x153, 0x9d,
  0x17e, 0x178,
];

// A string from an array of UTF-16 code units.
function fromUnits(units) {
  const n = units.length;
  const s = __intrinsic_str_new(n);
  for (let i = 0; i < n; i++) __intrinsic_str_set(s, i, units[i]);
  return s;
}

function pushCodePoint(units, cp) {
  if (cp < 0x10000) {
    units.push(cp);
  } else {
    cp -= 0x10000;
    units.push(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
  }
}

// The bytes of `input` (an ArrayBuffer or a view), else node's TypeError.
function bytesOf(input) {
  if (input === undefined) return new Uint8Array(0);
  if (__intrinsic_is_array_buffer(input)) return new Uint8Array(input);
  if (ArrayBuffer.isView(input)) return new Uint8Array(input.buffer, input.byteOffset, input.byteLength);
  throw codeError(TypeError, 'ERR_INVALID_ARG_TYPE',
    'The "list" argument must be an instance of SharedArrayBuffer, ArrayBuffer or ArrayBufferView.');
}

function invalid(encoding) {
  return codeError(TypeError, 'ERR_ENCODING_INVALID_ENCODED_DATA', `The encoded data was not valid for encoding ${encoding}`);
}

class TextEncoder {
  get encoding() {
    return 'utf-8';
  }

  encode(input = '') {
    const s = `${input}`;
    const out = [];
    const n = s.length;
    for (let i = 0; i < n; i++) {
      let c = s.charCodeAt(i);
      if (c >= 0xd800 && c <= 0xdfff) {
        const d = i + 1 < n ? s.charCodeAt(i + 1) : 0;
        if (c <= 0xdbff && d >= 0xdc00 && d <= 0xdfff) {
          c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
          i++;
        } else {
          c = 0xfffd;
        }
      }
      if (c < 0x80) {
        out.push(c);
      } else if (c < 0x800) {
        out.push(0xc0 | (c >> 6), 0x80 | (c & 0x3f));
      } else if (c < 0x10000) {
        out.push(0xe0 | (c >> 12), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
      } else {
        out.push(0xf0 | (c >> 18), 0x80 | ((c >> 12) & 0x3f), 0x80 | ((c >> 6) & 0x3f), 0x80 | (c & 0x3f));
      }
    }
    return new Uint8Array(out);
  }

  encodeInto(src, dest) {
    const s = `${src}`;
    if (__intrinsic_typed_kind(dest) !== 1) {
      throw $$errInvalidArgType("dest", "an instance of Uint8Array", dest);
    }
    const n = s.length;
    const cap = dest.length;
    let read = 0;
    let written = 0;
    while (read < n) {
      let c = s.charCodeAt(read);
      let units = 1;
      if (c >= 0xd800 && c <= 0xdfff) {
        const d = read + 1 < n ? s.charCodeAt(read + 1) : 0;
        if (c <= 0xdbff && d >= 0xdc00 && d <= 0xdfff) {
          c = 0x10000 + ((c - 0xd800) << 10) + (d - 0xdc00);
          units = 2;
        } else {
          c = 0xfffd;
        }
      }
      const size = c < 0x80 ? 1 : c < 0x800 ? 2 : c < 0x10000 ? 3 : 4;
      if (written + size > cap) break;
      if (size === 1) {
        dest[written] = c;
      } else if (size === 2) {
        dest[written] = 0xc0 | (c >> 6);
        dest[written + 1] = 0x80 | (c & 0x3f);
      } else if (size === 3) {
        dest[written] = 0xe0 | (c >> 12);
        dest[written + 1] = 0x80 | ((c >> 6) & 0x3f);
        dest[written + 2] = 0x80 | (c & 0x3f);
      } else {
        dest[written] = 0xf0 | (c >> 18);
        dest[written + 1] = 0x80 | ((c >> 12) & 0x3f);
        dest[written + 2] = 0x80 | ((c >> 6) & 0x3f);
        dest[written + 3] = 0x80 | (c & 0x3f);
      }
      written += size;
      read += units;
    }
    return { read, written };
  }

  // node prints `{ encoding: 'utf-8' }`.
  [inspectCustom](depth, options, inspect) {
    if (depth < 0) return this;
    return { encoding: this.encoding };
  }
}

// A UTF-8 decoder's state between `stream` calls (the WHATWG decoder: bytes seen and needed, the
// code point so far, the next byte's bounds).
class TextDecoder {
  #encoding;
  #fatal;
  #ignoreBOM;
  // Decoding state.
  #bomSeen = false;
  #pending = [];

  constructor(encoding = 'utf-8', options = undefined) {
    const name = encodingOf(encoding);
    if (name === undefined) {
      throw codeError(RangeError, 'ERR_ENCODING_NOT_SUPPORTED', `The "${encoding}" encoding is not supported`);
    }
    const o = options === undefined || options === null ? {} : options;
    this.#encoding = name;
    this.#fatal = !!o.fatal;
    this.#ignoreBOM = !!o.ignoreBOM;
  }

  decode(input = undefined, options = undefined) {
    if (!(#encoding in this)) throw codeError(TypeError, 'ERR_INVALID_THIS', 'Value of "this" must be of type TextDecoder');
    const bytes = bytesOf(input);
    const stream = options !== undefined && options !== null && !!options.stream;
    // The bytes left over from the last `stream` call first.
    let all = bytes;
    if (this.#pending.length > 0) {
      all = new Uint8Array(this.#pending.length + bytes.length);
      for (let i = 0; i < this.#pending.length; i++) all[i] = this.#pending[i];
      for (let i = 0; i < bytes.length; i++) all[this.#pending.length + i] = bytes[i];
      this.#pending = [];
    }
    const units = [];
    let end;
    if (this.#encoding === 'utf-8') end = this.#utf8(all, units, stream);
    else if (this.#encoding === 'windows-1252') end = latin(all, units);
    else end = this.#utf16(all, units, stream, this.#encoding === 'utf-16le');
    for (let i = end; i < all.length; i++) this.#pending.push(all[i]);
    // The BOM is skipped once at the start of a stream.
    let start = 0;
    if (!this.#ignoreBOM && !this.#bomSeen && units.length > 0) {
      if (units[0] === 0xfeff) start = 1;
      this.#bomSeen = true;
    }
    if (!stream) this.#bomSeen = false;
    return fromUnits(start === 0 ? units : units.slice(start));
  }

  // UTF-8 (WHATWG): U+FFFD for each maximal invalid subsequence. Returns where the bytes kept for the
  // next `stream` call start.
  #utf8(b, units, stream) {
    const n = b.length;
    let i = 0;
    while (i < n) {
      const c = b[i];
      if (c < 0x80) {
        units.push(c);
        i++;
        continue;
      }
      let need;
      let cp;
      let lower = 0x80;
      let upper = 0xbf;
      if (c >= 0xc2 && c <= 0xdf) {
        need = 1;
        cp = c & 0x1f;
      } else if (c >= 0xe0 && c <= 0xef) {
        if (c === 0xe0) lower = 0xa0;
        if (c === 0xed) upper = 0x9f;
        need = 2;
        cp = c & 0xf;
      } else if (c >= 0xf0 && c <= 0xf4) {
        if (c === 0xf0) lower = 0x90;
        if (c === 0xf4) upper = 0x8f;
        need = 3;
        cp = c & 0x7;
      } else {
        if (this.#fatal) throw invalid('utf-8');
        units.push(0xfffd);
        i++;
        continue;
      }
      let j = i + 1;
      let ok = true;
      while (need > 0) {
        if (j >= n) {
          // Truncated: kept for the next call when streaming.
          if (stream) return i;
          ok = false;
          break;
        }
        const d = b[j];
        if (d < lower || d > upper) {
          ok = false;
          break;
        }
        lower = 0x80;
        upper = 0xbf;
        cp = (cp << 6) | (d & 0x3f);
        need--;
        j++;
      }
      if (!ok) {
        if (this.#fatal) throw invalid('utf-8');
        units.push(0xfffd);
        i = j;
        continue;
      }
      pushCodePoint(units, cp);
      i = j;
    }
    return n;
  }

  #utf16(b, units, stream, little) {
    const n = b.length;
    let i = 0;
    while (i + 1 < n) {
      const c = little ? b[i] | (b[i + 1] << 8) : (b[i] << 8) | b[i + 1];
      if (c >= 0xd800 && c <= 0xdbff) {
        if (i + 3 >= n) {
          if (stream) return i;
          if (this.#fatal) throw invalid(this.#encoding);
          units.push(0xfffd);
          i += 2;
          continue;
        }
        const d = little ? b[i + 2] | (b[i + 3] << 8) : (b[i + 2] << 8) | b[i + 3];
        if (d >= 0xdc00 && d <= 0xdfff) {
          units.push(c, d);
          i += 4;
          continue;
        }
        if (this.#fatal) throw invalid(this.#encoding);
        units.push(0xfffd);
        i += 2;
        continue;
      }
      if (c >= 0xdc00 && c <= 0xdfff) {
        if (this.#fatal) throw invalid(this.#encoding);
        units.push(0xfffd);
      } else {
        units.push(c);
      }
      i += 2;
    }
    if (i < n) {
      if (stream) return i;
      if (this.#fatal) throw invalid(this.#encoding);
      units.push(0xfffd);
    }
    return n;
  }

  get encoding() {
    if (!(#encoding in this)) throw codeError(TypeError, 'ERR_INVALID_THIS', 'Value of "this" must be of type TextDecoder');
    return this.#encoding;
  }

  get fatal() {
    if (!(#encoding in this)) throw codeError(TypeError, 'ERR_INVALID_THIS', 'Value of "this" must be of type TextDecoder');
    return this.#fatal;
  }

  get ignoreBOM() {
    if (!(#encoding in this)) throw codeError(TypeError, 'ERR_INVALID_THIS', 'Value of "this" must be of type TextDecoder');
    return this.#ignoreBOM;
  }

  // node prints `TextDecoder { encoding: 'utf-8', fatal: false, ignoreBOM: false }`.
  [inspectCustom](depth, options, inspect) {
    if (!(#encoding in this) || depth < 0) return this;
    const o = Object.create(TextDecoder.prototype);
    // Own data properties shadow the prototype's getters.
    Object.defineProperty(o, 'encoding', { value: this.#encoding, enumerable: true });
    Object.defineProperty(o, 'fatal', { value: this.#fatal, enumerable: true });
    Object.defineProperty(o, 'ignoreBOM', { value: this.#ignoreBOM, enumerable: true });
    return o;
  }
}

// windows-1252: every byte is one character.
function latin(b, units) {
  for (let i = 0; i < b.length; i++) {
    const c = b[i];
    units.push(c >= 0x80 && c <= 0x9f ? W1252[c - 0x80] : c);
  }
  return b.length;
}

for (const [C, tag] of [[TextEncoder, 'TextEncoder'], [TextDecoder, 'TextDecoder']]) {
  Object.defineProperty(C.prototype, Symbol.toStringTag, {
    value: tag, writable: false, enumerable: false, configurable: true,
  });
}
for (const name of ['encoding', 'encode', 'encodeInto']) {
  Object.defineProperty(TextEncoder.prototype, name, { enumerable: true });
}
for (const name of ['decode', 'encoding', 'fatal', 'ignoreBOM']) {
  Object.defineProperty(TextDecoder.prototype, name, { enumerable: true });
}

module.exports = { TextEncoder, TextDecoder };
