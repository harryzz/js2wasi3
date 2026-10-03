// The WHATWG URL parser (T64), written for js2wasi3 from the URL Standard
// (https://url.spec.whatwg.org/): what node's C++ binding (ada) gives `internal/url`. A URL is
// parsed into a record, serialized to its `href`, and `urlComponents` gets ada's offsets into
// it (node's `URL` getters slice the `href` with them). Hosts: IPv4, IPv6, opaque hosts and
// domains; a domain's IDNA processing is reduced (ASCII lower-casing, full-width forms and dots,
// `toLowerCase`/NFC for the rest, Punycode): no UTS #46 mapping table or validity checks.
'use strict';

const OMITTED = 4294967295;
const urlComponents = [0, 0, 0, 0, OMITTED, 0, OMITTED, OMITTED, 1];

function defaultPort(scheme) {
  switch (scheme) {
    case 'ftp': return 21;
    case 'http': case 'ws': return 80;
    case 'https': case 'wss': return 443;
  }
  return null;
}

function isSpecialScheme(scheme) {
  switch (scheme) {
    case 'ftp': case 'file': case 'http': case 'https': case 'ws': case 'wss': return true;
  }
  return false;
}

// ada's `scheme::type`: HTTP 0, NOT_SPECIAL 1, HTTPS 2, WS 3, FTP 4, WSS 5, FILE 6.
function schemeType(scheme) {
  switch (scheme) {
    case 'http': return 0;
    case 'https': return 2;
    case 'ws': return 3;
    case 'ftp': return 4;
    case 'wss': return 5;
    case 'file': return 6;
  }
  return 1;
}

// Percent-encode sets: 0 C0 control, 1 fragment, 2 query, 3 special-query, 4 path, 5 userinfo,
// 6 component.
const C0 = 0, FRAGMENT = 1, QUERY = 2, SPECIAL_QUERY = 3, PATH = 4, USERINFO = 5;

function inSet(c, set) {
  if (c < 0x20 || c > 0x7e) return true;
  if (set === C0) return false;
  if (set === FRAGMENT) return c === 0x20 || c === 0x22 || c === 0x3c || c === 0x3e || c === 0x60;
  const query = c === 0x20 || c === 0x22 || c === 0x23 || c === 0x3c || c === 0x3e;
  if (set === QUERY) return query;
  if (set === SPECIAL_QUERY) return query || c === 0x27;
  const path = query || c === 0x3f || c === 0x5e || c === 0x60 || c === 0x7b || c === 0x7d;
  if (set === PATH) return path;
  return path || c === 0x2f || c === 0x3a || c === 0x3b || c === 0x3d || c === 0x40 ||
    (c >= 0x5b && c <= 0x5e) || c === 0x7c;
}

const HEX = '0123456789ABCDEF';

function pctByte(b) {
  return '%' + HEX[b >> 4] + HEX[b & 15];
}

// UTF-8 bytes of code point `c` (a lone surrogate is U+FFFD).
function utf8Bytes(c) {
  if (c >= 0xd800 && c <= 0xdfff) c = 0xfffd;
  if (c < 0x80) return [c];
  if (c < 0x800) return [0xc0 | (c >> 6), 0x80 | (c & 63)];
  if (c < 0x10000) return [0xe0 | (c >> 12), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63)];
  return [0xf0 | (c >> 18), 0x80 | ((c >> 12) & 63), 0x80 | ((c >> 6) & 63), 0x80 | (c & 63)];
}

function encodeCodePoint(c, set) {
  if (!inSet(c, set)) return String.fromCharCode(c);
  const bytes = utf8Bytes(c);
  let out = '';
  for (let i = 0; i < bytes.length; i++) out += pctByte(bytes[i]);
  return out;
}

function codePoints(s) {
  const out = [];
  for (let i = 0; i < s.length; i++) {
    const c = s.codePointAt(i);
    out.push(c);
    if (c > 0xffff) i++;
  }
  return out;
}

function encodeString(s, set) {
  const cps = codePoints(s);
  let out = '';
  for (let i = 0; i < cps.length; i++) out += encodeCodePoint(cps[i], set);
  return out;
}

function isHexDigit(c) {
  return (c >= 0x30 && c <= 0x39) || (c >= 0x41 && c <= 0x46) || (c >= 0x61 && c <= 0x66);
}

function hexValue(c) {
  return c <= 0x39 ? c - 0x30 : (c | 0x20) - 0x57;
}

// The bytes of `s` (UTF-8), percent-decoded.
function percentDecode(s) {
  const bytes = [];
  const cps = codePoints(s);
  for (let i = 0; i < cps.length; i++) {
    const b = utf8Bytes(cps[i]);
    for (let j = 0; j < b.length; j++) bytes.push(b[j]);
  }
  const out = [];
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (b === 0x25 && i + 2 < bytes.length && isHexDigit(bytes[i + 1]) && isHexDigit(bytes[i + 2])) {
      out.push(hexValue(bytes[i + 1]) * 16 + hexValue(bytes[i + 2]));
      i += 2;
    } else {
      out.push(b);
    }
  }
  return out;
}

// UTF-8 decode with replacement (the WHATWG decoder's maximal subparts).
function utf8Decode(bytes) {
  let out = '';
  let i = 0;
  while (i < bytes.length) {
    const b = bytes[i];
    if (b < 0x80) { out += String.fromCharCode(b); i++; continue; }
    let need = 0, cp = 0, lower = 0x80, upper = 0xbf;
    if (b >= 0xc2 && b <= 0xdf) { need = 1; cp = b & 0x1f; }
    else if (b >= 0xe0 && b <= 0xef) {
      need = 2; cp = b & 0xf;
      if (b === 0xe0) lower = 0xa0;
      if (b === 0xed) upper = 0x9f;
    } else if (b >= 0xf0 && b <= 0xf4) {
      need = 3; cp = b & 0x7;
      if (b === 0xf0) lower = 0x90;
      if (b === 0xf4) upper = 0x8f;
    } else { out += '�'; i++; continue; }
    let j = i + 1;
    let ok = true;
    for (let k = 0; k < need; k++, j++) {
      const n = bytes[j];
      if (j >= bytes.length || n < lower || n > upper) { ok = false; break; }
      lower = 0x80; upper = 0xbf;
      cp = (cp << 6) | (n & 0x3f);
    }
    if (!ok) { out += '�'; i = j; continue; }
    out += String.fromCodePoint(cp);
    i = j;
  }
  return out;
}

// --- Punycode (RFC 3492) ---

const BASE = 36, TMIN = 1, TMAX = 26, SKEW = 38, DAMP = 700;

function adapt(delta, numPoints, first) {
  delta = first ? Math.floor(delta / DAMP) : delta >> 1;
  delta += Math.floor(delta / numPoints);
  let k = 0;
  while (delta > ((BASE - TMIN) * TMAX) >> 1) {
    delta = Math.floor(delta / (BASE - TMIN));
    k += BASE;
  }
  return k + Math.floor(((BASE - TMIN + 1) * delta) / (delta + SKEW));
}

function digitChar(d) {
  return String.fromCharCode(d < 26 ? d + 97 : d + 22);
}

function punycodeEncode(input) {
  const cps = codePoints(input);
  let out = '';
  for (let i = 0; i < cps.length; i++) if (cps[i] < 0x80) out += String.fromCharCode(cps[i]);
  const b = out.length;
  let h = b;
  if (b > 0) out += '-';
  let n = 128, delta = 0, bias = 72;
  while (h < cps.length) {
    let m = 0x7fffffff;
    for (let i = 0; i < cps.length; i++) if (cps[i] >= n && cps[i] < m) m = cps[i];
    delta += (m - n) * (h + 1);
    n = m;
    for (let i = 0; i < cps.length; i++) {
      const c = cps[i];
      if (c < n) delta++;
      if (c === n) {
        let q = delta;
        for (let k = BASE; ; k += BASE) {
          const t = k <= bias ? TMIN : k >= bias + TMAX ? TMAX : k - bias;
          if (q < t) break;
          out += digitChar(t + ((q - t) % (BASE - t)));
          q = Math.floor((q - t) / (BASE - t));
        }
        out += digitChar(q);
        bias = adapt(delta, h + 1, h === b);
        delta = 0;
        h++;
      }
    }
    delta++;
    n++;
  }
  return out;
}

// The decoded label, or `null`.
function punycodeDecode(input) {
  const out = [];
  let basic = input.lastIndexOf('-');
  if (basic < 0) basic = 0;
  for (let j = 0; j < basic; j++) {
    const c = input.charCodeAt(j);
    if (c >= 0x80) return null;
    out.push(c);
  }
  let n = 128, i = 0, bias = 72;
  for (let idx = basic > 0 ? basic + 1 : 0; idx < input.length;) {
    const oldi = i;
    let w = 1;
    for (let k = BASE; ; k += BASE) {
      if (idx >= input.length) return null;
      const c = input.charCodeAt(idx++);
      const digit = c >= 0x30 && c <= 0x39 ? c - 22 : c >= 0x41 && c <= 0x5a ? c - 0x41 : c >= 0x61 && c <= 0x7a ? c - 0x61 : BASE;
      if (digit >= BASE) return null;
      i += digit * w;
      const t = k <= bias ? TMIN : k >= bias + TMAX ? TMAX : k - bias;
      if (digit < t) break;
      w *= BASE - t;
      if (w > 0x7fffffff) return null;
    }
    bias = adapt(i - oldi, out.length + 1, oldi === 0);
    n += Math.floor(i / (out.length + 1));
    i %= out.length + 1;
    if (n > 0x10ffff) return null;
    out.splice(i, 0, n);
    i++;
  }
  let s = '';
  for (let j = 0; j < out.length; j++) s += String.fromCodePoint(out[j]);
  return s;
}

// --- Hosts ---

function isForbiddenHost(c) {
  return c === 0 || c === 0x09 || c === 0x0a || c === 0x0d || c === 0x20 || c === 0x23 || c === 0x2f ||
    c === 0x3a || c === 0x3c || c === 0x3e || c === 0x3f || c === 0x40 || c === 0x5b || c === 0x5c ||
    c === 0x5d || c === 0x5e || c === 0x7c;
}

function isForbiddenDomain(c) {
  return isForbiddenHost(c) || c <= 0x1f || c === 0x25 || c === 0x7f;
}

// UTS #46 ToASCII, reduced (see the top of the file); `null` on failure.
function toASCII(domain) {
  let mapped = '';
  let ascii = true;
  const cps = codePoints(domain);
  for (let i = 0; i < cps.length; i++) {
    let c = cps[i];
    if (c === 0x3002 || c === 0xff0e || c === 0xff61) c = 0x2e;
    else if (c >= 0xff01 && c <= 0xff5e) c -= 0xfee0;
    if (c === 0xad || c === 0x200b || c === 0x2060 || c === 0xfeff || (c >= 0xfe00 && c <= 0xfe0f)) continue;
    if (c >= 0x41 && c <= 0x5a) c += 0x20;
    if (c >= 0x80) ascii = false;
    mapped += String.fromCodePoint(c);
  }
  if (!ascii) mapped = mapped.toLowerCase().normalize('NFC');
  const labels = mapped.split('.');
  for (let i = 0; i < labels.length; i++) {
    const label = labels[i];
    if (label.startsWith('xn--')) {
      const decoded = punycodeDecode(label.slice(4));
      if (decoded === null || decoded === '' || punycodeEncode(decoded) !== label.slice(4)) return null;
      continue;
    }
    let plain = true;
    for (let j = 0; j < label.length; j++) if (label.charCodeAt(j) >= 0x80) plain = false;
    if (!plain) labels[i] = 'xn--' + punycodeEncode(label);
  }
  const out = labels.join('.');
  return out === '' ? null : out;
}

function toUnicode(domain) {
  const labels = domain.toLowerCase().split('.');
  for (let i = 0; i < labels.length; i++) {
    if (labels[i].startsWith('xn--')) {
      const decoded = punycodeDecode(labels[i].slice(4));
      if (decoded !== null) labels[i] = decoded;
    }
  }
  return labels.join('.');
}

// The IPv4 number in `s`, or -1.
function ipv4Number(s) {
  if (s === '') return -1;
  let radix = 10;
  if (s.length >= 2 && s[0] === '0' && (s[1] === 'x' || s[1] === 'X')) {
    s = s.slice(2);
    radix = 16;
  } else if (s.length >= 2 && s[0] === '0') {
    s = s.slice(1);
    radix = 8;
  }
  if (s === '') return 0;
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    let d;
    if (c >= 0x30 && c <= 0x39) d = c - 0x30;
    else if (radix === 16 && isHexDigit(c)) d = hexValue(c);
    else return -1;
    if (d >= radix) return -1;
    n = n * radix + d;
  }
  return n;
}

function endsInNumber(s) {
  const parts = s.split('.');
  if (parts[parts.length - 1] === '') {
    if (parts.length === 1) return false;
    parts.pop();
  }
  const last = parts[parts.length - 1];
  if (last !== '' && /^[0-9]+$/.test(last)) return true;
  return ipv4Number(last) >= 0;
}

// The IPv4 address in `s` as a number, or -1.
function parseIPv4(s) {
  const parts = s.split('.');
  if (parts[parts.length - 1] === '' && parts.length > 1) parts.pop();
  if (parts.length > 4) return -1;
  const numbers = [];
  for (let i = 0; i < parts.length; i++) {
    const n = ipv4Number(parts[i]);
    if (n < 0) return -1;
    numbers.push(n);
  }
  for (let i = 0; i < numbers.length - 1; i++) if (numbers[i] > 255) return -1;
  const last = numbers[numbers.length - 1];
  if (last >= Math.pow(256, 5 - numbers.length)) return -1;
  let ipv4 = last;
  for (let i = 0; i < numbers.length - 1; i++) ipv4 += numbers[i] * Math.pow(256, 3 - i);
  return ipv4;
}

function serializeIPv4(n) {
  const out = [];
  for (let i = 0; i < 4; i++) {
    out.unshift(String(n % 256));
    n = Math.floor(n / 256);
  }
  return out.join('.');
}

// The eight pieces of the IPv6 address in `s`, or `null`.
function parseIPv6(s) {
  const address = [0, 0, 0, 0, 0, 0, 0, 0];
  let pieceIndex = 0;
  let compress = -1;
  let p = 0;
  const c = (i) => (i < s.length ? s.charCodeAt(i) : -1);
  if (c(p) === 0x3a) {
    if (c(p + 1) !== 0x3a) return null;
    p += 2;
    pieceIndex++;
    compress = pieceIndex;
  }
  while (c(p) !== -1) {
    if (pieceIndex === 8) return null;
    if (c(p) === 0x3a) {
      if (compress !== -1) return null;
      p++;
      pieceIndex++;
      compress = pieceIndex;
      continue;
    }
    let value = 0, length = 0;
    while (length < 4 && isHexDigit(c(p))) {
      value = value * 16 + hexValue(c(p));
      p++;
      length++;
    }
    if (c(p) === 0x2e) {
      if (length === 0) return null;
      p -= length;
      if (pieceIndex > 6) return null;
      let numbersSeen = 0;
      while (c(p) !== -1) {
        let ipv4Piece = -1;
        if (numbersSeen > 0) {
          if (c(p) === 0x2e && numbersSeen < 4) p++;
          else return null;
        }
        if (!(c(p) >= 0x30 && c(p) <= 0x39)) return null;
        while (c(p) >= 0x30 && c(p) <= 0x39) {
          const n = c(p) - 0x30;
          if (ipv4Piece === -1) ipv4Piece = n;
          else if (ipv4Piece === 0) return null;
          else ipv4Piece = ipv4Piece * 10 + n;
          if (ipv4Piece > 255) return null;
          p++;
        }
        address[pieceIndex] = address[pieceIndex] * 0x100 + ipv4Piece;
        numbersSeen++;
        if (numbersSeen === 2 || numbersSeen === 4) pieceIndex++;
      }
      if (numbersSeen !== 4) return null;
      break;
    } else if (c(p) === 0x3a) {
      p++;
      if (c(p) === -1) return null;
    } else if (c(p) !== -1) {
      return null;
    }
    address[pieceIndex] = value;
    pieceIndex++;
  }
  if (compress !== -1) {
    let swaps = pieceIndex - compress;
    pieceIndex = 7;
    while (pieceIndex !== 0 && swaps > 0) {
      const t = address[pieceIndex];
      address[pieceIndex] = address[compress + swaps - 1];
      address[compress + swaps - 1] = t;
      pieceIndex--;
      swaps--;
    }
  } else if (pieceIndex !== 8) {
    return null;
  }
  return address;
}

function serializeIPv6(address) {
  // The first longest run of two or more zero pieces is compressed.
  let best = -1, bestLen = 1;
  for (let i = 0; i < 8;) {
    if (address[i] !== 0) { i++; continue; }
    let j = i;
    while (j < 8 && address[j] === 0) j++;
    if (j - i > bestLen) { best = i; bestLen = j - i; }
    i = j;
  }
  let out = '';
  let ignore0 = false;
  for (let i = 0; i < 8; i++) {
    if (ignore0 && address[i] === 0) continue;
    ignore0 = false;
    if (best === i) {
      out += i === 0 ? '::' : ':';
      ignore0 = true;
      continue;
    }
    out += address[i].toString(16);
    if (i !== 7) out += ':';
  }
  return out;
}

// The serialized host in `input`, or `null` (failure).
function parseHost(input, isOpaque) {
  if (input[0] === '[') {
    if (input[input.length - 1] !== ']') return null;
    const address = parseIPv6(input.slice(1, -1));
    return address === null ? null : '[' + serializeIPv6(address) + ']';
  }
  if (isOpaque) {
    const cps = codePoints(input);
    for (let i = 0; i < cps.length; i++) if (isForbiddenHost(cps[i])) return null;
    return encodeString(input, C0);
  }
  const domain = utf8Decode(percentDecode(input));
  const ascii = toASCII(domain);
  if (ascii === null) return null;
  for (let i = 0; i < ascii.length; i++) if (isForbiddenDomain(ascii.charCodeAt(i))) return null;
  if (endsInNumber(ascii)) {
    const ipv4 = parseIPv4(ascii);
    return ipv4 < 0 ? null : serializeIPv4(ipv4);
  }
  return ascii;
}

// --- The basic URL parser ---

function newRecord() {
  return { scheme: '', username: '', password: '', host: null, port: null, path: [], query: null, fragment: null };
}

function cloneRecord(u) {
  return {
    scheme: u.scheme, username: u.username, password: u.password, host: u.host, port: u.port,
    path: typeof u.path === 'string' ? u.path : u.path.slice(), query: u.query, fragment: u.fragment,
  };
}

function isAlpha(c) {
  return (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a);
}

function isDigit(c) {
  return c >= 0x30 && c <= 0x39;
}

function isWindowsDriveLetter(s, normalized) {
  return s.length === 2 && isAlpha(s.charCodeAt(0)) && (s[1] === ':' || (!normalized && s[1] === '|'));
}

// Whether code points `cps` from `p` start with a Windows drive letter.
function startsWithDriveLetter(cps, p) {
  if (cps.length - p < 2) return false;
  if (!isAlpha(cps[p]) || (cps[p + 1] !== 0x3a && cps[p + 1] !== 0x7c)) return false;
  if (cps.length - p === 2) return true;
  const c = cps[p + 2];
  return c === 0x2f || c === 0x5c || c === 0x3f || c === 0x23;
}

function shortenPath(u) {
  const path = u.path;
  if (u.scheme === 'file' && path.length === 1 && isWindowsDriveLetter(path[0], true)) return;
  path.pop();
}

function isSingleDot(s) {
  return s === '.' || s.toLowerCase() === '%2e';
}

function isDoubleDot(s) {
  switch (s.toLowerCase()) {
    case '..': case '.%2e': case '%2e.': case '%2e%2e': return true;
  }
  return false;
}

function hasCredentials(u) {
  return u.username !== '' || u.password !== '';
}

const FAILURE = null;

// The URL Standard's basic URL parser: the record, or `FAILURE`. With `state` (a setter's state
// override) it changes `url` in place and returns it (or `FAILURE`).
function basicParse(input, base, url, stateOverride) {
  if (url === undefined) {
    url = newRecord();
    let start = 0, end = input.length;
    while (start < end && input.charCodeAt(start) <= 0x20) start++;
    while (end > start && input.charCodeAt(end - 1) <= 0x20) end--;
    input = input.slice(start, end);
  }
  let clean = '';
  for (let i = 0; i < input.length; i++) {
    const ch = input.charCodeAt(i);
    if (ch !== 0x09 && ch !== 0x0a && ch !== 0x0d) clean += input[i];
  }
  const cps = codePoints(clean);
  const override = stateOverride !== undefined;
  let state = override ? stateOverride : 'scheme start';
  let buffer = '';
  let atSignSeen = false, insideBrackets = false, passwordTokenSeen = false;
  const len = cps.length;
  for (let p = 0; ; p++) {
    const c = p < len ? cps[p] : -1;
    const special = isSpecialScheme(url.scheme);
    switch (state) {
      case 'scheme start':
        if (isAlpha(c)) {
          buffer += String.fromCharCode(c | 0x20);
          state = 'scheme';
        } else if (!override) {
          state = 'no scheme';
          p--;
        } else {
          return FAILURE;
        }
        break;
      case 'scheme':
        if (isAlpha(c) || isDigit(c) || c === 0x2b || c === 0x2d || c === 0x2e) {
          buffer += String.fromCharCode(c >= 0x41 && c <= 0x5a ? c | 0x20 : c);
        } else if (c === 0x3a) {
          if (override) {
            if (isSpecialScheme(url.scheme) !== isSpecialScheme(buffer)) return url;
            if ((hasCredentials(url) || url.port !== null) && buffer === 'file') return url;
            if (url.scheme === 'file' && url.host === '') return url;
          }
          url.scheme = buffer;
          if (override) {
            if (url.port === defaultPort(url.scheme)) url.port = null;
            return url;
          }
          buffer = '';
          if (url.scheme === 'file') {
            state = 'file';
          } else if (isSpecialScheme(url.scheme) && base !== null && base.scheme === url.scheme) {
            state = 'special relative or authority';
          } else if (isSpecialScheme(url.scheme)) {
            state = 'special authority slashes';
          } else if (cps[p + 1] === 0x2f) {
            state = 'path or authority';
            p++;
          } else {
            url.path = '';
            state = 'opaque path';
          }
        } else if (!override) {
          buffer = '';
          state = 'no scheme';
          p = -1;
        } else {
          return FAILURE;
        }
        break;
      case 'no scheme':
        if (base === null || (typeof base.path === 'string' && c !== 0x23)) return FAILURE;
        if (typeof base.path === 'string' && c === 0x23) {
          url.scheme = base.scheme;
          url.path = base.path;
          url.query = base.query;
          url.fragment = '';
          state = 'fragment';
        } else if (base.scheme !== 'file') {
          state = 'relative';
          p--;
        } else {
          state = 'file';
          p--;
        }
        break;
      case 'special relative or authority':
        if (c === 0x2f && cps[p + 1] === 0x2f) {
          state = 'special authority ignore slashes';
          p++;
        } else {
          state = 'relative';
          p--;
        }
        break;
      case 'path or authority':
        if (c === 0x2f) {
          state = 'authority';
        } else {
          state = 'path';
          p--;
        }
        break;
      case 'relative':
        url.scheme = base.scheme;
        if (c === 0x2f || (isSpecialScheme(url.scheme) && c === 0x5c)) {
          state = 'relative slash';
        } else {
          url.username = base.username;
          url.password = base.password;
          url.host = base.host;
          url.port = base.port;
          url.path = base.path.slice();
          url.query = base.query;
          if (c === 0x3f) {
            url.query = '';
            state = 'query';
          } else if (c === 0x23) {
            url.fragment = '';
            state = 'fragment';
          } else if (c !== -1) {
            url.query = null;
            shortenPath(url);
            state = 'path';
            p--;
          }
        }
        break;
      case 'relative slash':
        if (special && (c === 0x2f || c === 0x5c)) {
          state = 'special authority ignore slashes';
        } else if (c === 0x2f) {
          state = 'authority';
        } else {
          url.username = base.username;
          url.password = base.password;
          url.host = base.host;
          url.port = base.port;
          state = 'path';
          p--;
        }
        break;
      case 'special authority slashes':
        if (c === 0x2f && cps[p + 1] === 0x2f) {
          state = 'special authority ignore slashes';
          p++;
        } else {
          state = 'special authority ignore slashes';
          p--;
        }
        break;
      case 'special authority ignore slashes':
        if (c !== 0x2f && c !== 0x5c) {
          state = 'authority';
          p--;
        }
        break;
      case 'authority':
        if (c === 0x40) {
          if (atSignSeen) buffer = '%40' + buffer;
          atSignSeen = true;
          const bcps = codePoints(buffer);
          for (let i = 0; i < bcps.length; i++) {
            if (bcps[i] === 0x3a && !passwordTokenSeen) {
              passwordTokenSeen = true;
              continue;
            }
            const encoded = encodeCodePoint(bcps[i], USERINFO);
            if (passwordTokenSeen) url.password += encoded;
            else url.username += encoded;
          }
          buffer = '';
        } else if (c === -1 || c === 0x2f || c === 0x3f || c === 0x23 || (special && c === 0x5c)) {
          if (atSignSeen && buffer === '') return FAILURE;
          p -= codePoints(buffer).length + 1;
          buffer = '';
          state = 'host';
        } else {
          buffer += String.fromCodePoint(c);
        }
        break;
      case 'host':
      case 'hostname':
        if (override && url.scheme === 'file') {
          p--;
          state = 'file host';
        } else if (c === 0x3a && !insideBrackets) {
          if (buffer === '') return FAILURE;
          if (stateOverride === 'hostname') return FAILURE;
          const host = parseHost(buffer, !special);
          if (host === null) return FAILURE;
          url.host = host;
          buffer = '';
          state = 'port';
        } else if (c === -1 || c === 0x2f || c === 0x3f || c === 0x23 || (special && c === 0x5c)) {
          p--;
          if (special && buffer === '') return FAILURE;
          if (override && buffer === '' && (hasCredentials(url) || url.port !== null)) return FAILURE;
          const host = parseHost(buffer, !special);
          if (host === null) return FAILURE;
          url.host = host;
          buffer = '';
          state = 'path start';
          if (override) return url;
        } else {
          if (c === 0x5b) insideBrackets = true;
          if (c === 0x5d) insideBrackets = false;
          buffer += String.fromCodePoint(c);
        }
        break;
      case 'port':
        if (isDigit(c)) {
          buffer += String.fromCharCode(c);
        } else if (c === -1 || c === 0x2f || c === 0x3f || c === 0x23 || (special && c === 0x5c) || override) {
          if (buffer !== '') {
            const port = parseInt(buffer, 10);
            if (port > 65535) return FAILURE;
            url.port = port === defaultPort(url.scheme) ? null : port;
            buffer = '';
            if (override) return url;
          }
          if (override) return FAILURE;
          state = 'path start';
          p--;
        } else {
          return FAILURE;
        }
        break;
      case 'file':
        url.scheme = 'file';
        url.host = '';
        if (c === 0x2f || c === 0x5c) {
          state = 'file slash';
        } else if (base !== null && base.scheme === 'file') {
          url.host = base.host;
          url.path = base.path.slice();
          url.query = base.query;
          if (c === 0x3f) {
            url.query = '';
            state = 'query';
          } else if (c === 0x23) {
            url.fragment = '';
            state = 'fragment';
          } else if (c !== -1) {
            url.query = null;
            if (!startsWithDriveLetter(cps, p)) shortenPath(url);
            else url.path = [];
            state = 'path';
            p--;
          }
        } else {
          state = 'path';
          p--;
        }
        break;
      case 'file slash':
        if (c === 0x2f || c === 0x5c) {
          state = 'file host';
        } else {
          if (base !== null && base.scheme === 'file') {
            url.host = base.host;
            if (!startsWithDriveLetter(cps, p) && base.path.length > 0 && isWindowsDriveLetter(base.path[0], true)) {
              url.path.push(base.path[0]);
            }
          }
          state = 'path';
          p--;
        }
        break;
      case 'file host':
        if (c === -1 || c === 0x2f || c === 0x5c || c === 0x3f || c === 0x23) {
          p--;
          if (!override && isWindowsDriveLetter(buffer, false)) {
            state = 'path';
          } else if (buffer === '') {
            url.host = '';
            if (override) return url;
            state = 'path start';
          } else {
            let host = parseHost(buffer, !special);
            if (host === null) return FAILURE;
            if (host === 'localhost') host = '';
            url.host = host;
            if (override) return url;
            buffer = '';
            state = 'path start';
          }
        } else {
          buffer += String.fromCodePoint(c);
        }
        break;
      case 'path start':
        if (special) {
          state = 'path';
          if (c !== 0x2f && c !== 0x5c) p--;
        } else if (!override && c === 0x3f) {
          url.query = '';
          state = 'query';
        } else if (!override && c === 0x23) {
          url.fragment = '';
          state = 'fragment';
        } else if (c !== -1) {
          state = 'path';
          if (c !== 0x2f) p--;
        } else if (override && url.host === null) {
          url.path.push('');
        }
        break;
      case 'path':
        if (c === -1 || c === 0x2f || (special && c === 0x5c) || (!override && (c === 0x3f || c === 0x23))) {
          const slash = c === 0x2f || (special && c === 0x5c);
          if (isDoubleDot(buffer)) {
            shortenPath(url);
            if (!slash) url.path.push('');
          } else if (isSingleDot(buffer) && !slash) {
            url.path.push('');
          } else if (!isSingleDot(buffer)) {
            if (url.scheme === 'file' && url.path.length === 0 && isWindowsDriveLetter(buffer, false)) {
              buffer = buffer[0] + ':';
            }
            url.path.push(buffer);
          }
          buffer = '';
          if (c === 0x3f) {
            url.query = '';
            state = 'query';
          }
          if (c === 0x23) {
            url.fragment = '';
            state = 'fragment';
          }
        } else {
          buffer += encodeCodePoint(c, PATH);
        }
        break;
      case 'opaque path':
        if (c === 0x3f) {
          url.query = '';
          state = 'query';
        } else if (c === 0x23) {
          url.fragment = '';
          state = 'fragment';
        } else if (c === 0x20) {
          const next = cps[p + 1];
          url.path += next === 0x3f || next === 0x23 ? '%20' : ' ';
        } else if (c !== -1) {
          url.path += encodeCodePoint(c, C0);
        }
        break;
      case 'query':
        if ((!override && c === 0x23) || c === -1) {
          url.query += encodeString(buffer, special ? SPECIAL_QUERY : QUERY);
          buffer = '';
          if (c === 0x23) {
            url.fragment = '';
            state = 'fragment';
          }
        } else {
          buffer += String.fromCodePoint(c);
        }
        break;
      case 'fragment':
        if (c !== -1) url.fragment += encodeCodePoint(c, FRAGMENT);
        break;
    }
    if (p >= len) break;
  }
  return url;
}

// Serializes `u` and sets `urlComponents` (ada's offsets) for it.
function serialize(u) {
  let out = u.scheme + ':';
  const protocolEnd = out.length;
  let usernameEnd = protocolEnd, hostStart = protocolEnd, hostEnd = protocolEnd;
  if (u.host !== null) {
    out += '//';
    usernameEnd = out.length;
    hostStart = out.length;
    if (hasCredentials(u)) {
      out += u.username;
      usernameEnd = out.length;
      if (u.password !== '') out += ':' + u.password;
      hostStart = out.length;
      out += '@';
    }
    out += u.host;
    hostEnd = out.length;
    if (u.port !== null) out += ':' + u.port;
  } else if (typeof u.path !== 'string' && u.path.length > 1 && u.path[0] === '') {
    out += '/.';
    usernameEnd = hostStart = hostEnd = out.length;
  }
  const pathnameStart = out.length;
  if (typeof u.path === 'string') out += u.path;
  else for (let i = 0; i < u.path.length; i++) out += '/' + u.path[i];
  let searchStart = OMITTED, hashStart = OMITTED;
  if (u.query !== null) {
    searchStart = out.length;
    out += '?' + u.query;
  }
  if (u.fragment !== null) {
    hashStart = out.length;
    out += '#' + u.fragment;
  }
  urlComponents[0] = protocolEnd;
  urlComponents[1] = usernameEnd;
  urlComponents[2] = hostStart;
  urlComponents[3] = hostEnd;
  urlComponents[4] = u.port === null ? OMITTED : u.port;
  urlComponents[5] = pathnameStart;
  urlComponents[6] = searchStart;
  urlComponents[7] = hashStart;
  urlComponents[8] = schemeType(u.scheme);
  return out;
}

function invalidURL(input, base) {
  const e = new TypeError('Invalid URL');
  e.code = 'ERR_INVALID_URL';
  e.input = input;
  if (base !== undefined) e.base = base;
  return e;
}

function parseRecord(input, base) {
  let baseRecord = null;
  if (base !== undefined) {
    baseRecord = basicParse(base, null);
    if (baseRecord === FAILURE) return FAILURE;
  }
  return basicParse(input, baseRecord);
}

// The `href` of `input` against `base`, `urlComponents` set; on failure throws node's
// `ERR_INVALID_URL` (or returns `undefined` when `raiseException` is false).
function parse(input, base, raiseException) {
  const u = parseRecord(input, base);
  if (u === FAILURE) {
    if (raiseException) throw invalidURL(input, base);
    return undefined;
  }
  return serialize(u);
}

function canParse(input, base) {
  return parseRecord(input, base) !== FAILURE;
}

function cannotHaveCredentialsOrPort(u) {
  return u.host === null || u.host === '' || u.scheme === 'file';
}

// The URL Standard's setters (node's `updateActions` numbering): the new `href`, or `false`.
function update(href, action, value) {
  let u = basicParse(href, null);
  switch (action) {
    case 0: // protocol
      if (basicParse(value + ':', null, u, 'scheme start') === FAILURE) return false;
      break;
    case 1: // host
    case 2: // hostname
      if (typeof u.path === 'string') return false;
      if (basicParse(value, null, u, action === 1 ? 'host' : 'hostname') === FAILURE) return false;
      break;
    case 3: // port
      if (cannotHaveCredentialsOrPort(u)) return false;
      if (value === '') u.port = null;
      else if (basicParse(value, null, u, 'port') === FAILURE) return false;
      break;
    case 4: // username
      if (cannotHaveCredentialsOrPort(u)) return false;
      u.username = encodeString(value, USERINFO);
      break;
    case 5: // password
      if (cannotHaveCredentialsOrPort(u)) return false;
      u.password = encodeString(value, USERINFO);
      break;
    case 6: // pathname
      if (typeof u.path === 'string') return false;
      u.path = [];
      if (basicParse(value, null, u, 'path start') === FAILURE) return false;
      break;
    case 7: // search
      if (value === '') {
        u.query = null;
        stripTrailingSpaces(u);
        break;
      }
      if (value[0] === '?') value = value.slice(1);
      u.query = '';
      basicParse(value, null, u, 'query');
      break;
    case 8: // hash
      if (value === '') {
        u.fragment = null;
        stripTrailingSpaces(u);
        break;
      }
      if (value[0] === '#') value = value.slice(1);
      u.fragment = '';
      basicParse(value, null, u, 'fragment');
      break;
    case 9: { // href
      const parsed = basicParse(value, null);
      if (parsed === FAILURE) return false;
      u = parsed;
      break;
    }
  }
  return serialize(u);
}

// An opaque path's trailing spaces can't end the `href` (ada encodes the last one).
function stripTrailingSpaces(u) {
  if (typeof u.path !== 'string' || u.fragment !== null || u.query !== null) return;
  if (u.path.endsWith(' ')) u.path = u.path.slice(0, -1) + '%20';
}

function domainToASCII(domain) {
  const host = parseHost(domain, false);
  return host === null ? '' : host;
}

function domainToUnicode(domain) {
  const host = parseHost(domain, false);
  return host === null ? '' : toUnicode(host);
}

// ada's `href_from_file`: a `file:` URL for an absolute path (`%`, `\`, newlines and tabs escaped
// first), with `hostname` for a UNC path.
function pathToFileURL(filepath, windows, hostname) {
  let escaped = '';
  for (let i = 0; i < filepath.length; i++) {
    const ch = filepath[i];
    if (ch === '%') escaped += '%25';
    else if (ch === '\\') escaped += windows ? '/' : '%5C';
    else if (ch === '\n') escaped += '%0A';
    else if (ch === '\r') escaped += '%0D';
    else if (ch === '\t') escaped += '%09';
    else escaped += ch;
  }
  const u = newRecord();
  u.scheme = 'file';
  u.host = '';
  basicParse(escaped, null, u, 'path start');
  if (hostname !== undefined) basicParse(hostname, null, u, 'hostname');
  return serialize(u);
}

// node's `url.format(URL, options)`.
function format(href, fragment, unicode, search, auth) {
  const u = basicParse(href, null);
  if (!fragment) u.fragment = null;
  if (unicode && u.host !== null) u.host = toUnicode(u.host);
  if (!search) u.query = null;
  if (!auth) {
    u.username = '';
    u.password = '';
  }
  return serialize(u);
}

module.exports = {
  urlComponents, parse, canParse, update, domainToASCII, domainToUnicode, pathToFileURL, format, toASCII,
  encodeString,
};
