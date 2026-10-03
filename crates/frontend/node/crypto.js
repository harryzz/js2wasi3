// node's `crypto` module (T65), a subset: random values from `wasi:random` (`randomBytes`,
// `randomFill(Sync)`, `randomInt`, `randomUUID`, `getRandomValues`, `webcrypto`), hashes (`md5`,
// `sha1`, `sha224`, `sha256`, `sha384`, `sha512`: `createHash`, `hash`, `createHmac`), AES with 128,
// 192 and 256-bit keys in ECB, CBC, CTR and CCM modes (`createCipheriv`/`createDecipheriv`) and
// `timingSafeEqual`, with node's errors. Everything else is missing (F280).
'use strict';

const { Buffer } = require('buffer');
const wasi = require('node:internal/wasi_random');

function codeError(Base, code, message) {
  const e = new Base(message);
  if (code !== undefined) e.code = code;
  return e;
}

// A DOMException-like error (webcrypto's `getRandomValues`).
function domError(name, code, message) {
  const e = new Error(message);
  e.name = name;
  e.code = code;
  return e;
}

// The bytes of a `string | ArrayBufferView | ArrayBuffer` argument.
function bytesOf(data, encoding, name, expected) {
  if (typeof data === 'string') return Buffer.from(data, encoding === undefined || encoding === 'buffer' ? 'utf8' : encoding);
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  if (expected.includes('ArrayBuffer,') && data instanceof ArrayBuffer) return new Uint8Array(data);
  throw $$errInvalidArgType(name, expected, data);
}

const DATA_TYPES = 'of type string or an instance of Buffer, TypedArray, or DataView';

function output(bytes, encoding) {
  const b = Buffer.from(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return encoding === undefined || encoding === 'buffer' ? b : b.toString(encoding);
}

function concat(chunks) {
  let n = 0;
  for (const c of chunks) n += c.length;
  const out = new Uint8Array(n);
  let at = 0;
  for (const c of chunks) {
    out.set(c, at);
    at += c.length;
  }
  return out;
}

// --- Random values ---

const MAX_SIZE = 2147483647;

function fillRandom(view, offset, size) {
  let at = offset;
  while (size > 0) {
    const n = size > 65536 ? 65536 : size;
    view.set(wasi.getRandomBytes(n), at);
    at += n;
    size -= n;
  }
}

function checkSize(size, name, max) {
  if (typeof size !== 'number') throw $$errInvalidArgType(name, 'of type number', size);
  if (!(size >= 0 && size <= max) || size !== Math.floor(size)) {
    throw $$errOutOfRange(name, `>= 0 && <= ${max}`, size);
  }
}

function randomBytes(size, callback) {
  checkSize(size, 'size', MAX_SIZE);
  if (callback !== undefined && typeof callback !== 'function') {
    throw $$errInvalidArgType('callback', 'of type function', callback);
  }
  const buf = Buffer.alloc(size);
  fillRandom(buf, 0, size);
  if (callback === undefined) return buf;
  process.nextTick(() => callback(null, buf));
}

function viewBytes(buf, name) {
  if (ArrayBuffer.isView(buf)) return new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
  if (buf instanceof ArrayBuffer) return new Uint8Array(buf);
  throw $$errInvalidArgType(name, 'an instance of ArrayBuffer or ArrayBufferView', buf);
}

function randomFillSync(buf, offset = 0, size) {
  const bytes = viewBytes(buf, 'buf');
  const elem = ArrayBuffer.isView(buf) && buf.BYTES_PER_ELEMENT !== undefined ? buf.BYTES_PER_ELEMENT : 1;
  const len = bytes.length;
  checkSize(offset, 'offset', MAX_SIZE);
  const start = offset * elem;
  if (start > len) throw $$errOutOfRange('offset', `>= 0 && <= ${len}`, offset);
  let n = len - start;
  if (size !== undefined) {
    checkSize(size, 'size', MAX_SIZE);
    n = size * elem;
    if (start + n > len) throw $$errOutOfRange('size + offset', `<= ${len}`, size + offset);
  }
  fillRandom(bytes, start, n);
  return buf;
}

function randomFill(buf, offset, size, callback) {
  if (typeof offset === 'function') {
    callback = offset;
    offset = 0;
    size = undefined;
  } else if (typeof size === 'function') {
    callback = size;
    size = undefined;
  }
  if (typeof callback !== 'function') throw $$errInvalidArgType('callback', 'of type function', callback);
  randomFillSync(buf, offset, size);
  process.nextTick(() => callback(null, buf));
}

// `randomInt([min, ]max[, callback])`: uniform in [min, max), by rejection sampling of 48-bit values.
function randomInt(min, max, callback) {
  if (max === undefined || typeof max === 'function') {
    callback = max;
    max = min;
    min = 0;
  }
  if (callback !== undefined && typeof callback !== 'function') {
    throw $$errInvalidArgType('callback', 'of type function', callback);
  }
  if (!Number.isSafeInteger(min)) throw $$errInvalidArgType('min', 'a safe integer', min);
  if (!Number.isSafeInteger(max)) throw $$errInvalidArgType('max', 'a safe integer', max);
  if (max <= min) {
    throw $$errOutOfRange('max', `greater than the value of "min" (${min})`, max);
  }
  const range = max - min;
  if (range > 281474976710655) {
    throw $$errOutOfRange('max - min', '<= 281474976710655', range);
  }
  const limit = 281474976710656 - (281474976710656 % range);
  let x;
  do {
    const b = wasi.getRandomBytes(6);
    x = ((b[0] * 256 + b[1]) * 256 + b[2]) * 16777216 + ((b[3] * 256 + b[4]) * 256 + b[5]);
  } while (x >= limit);
  const n = min + (x % range);
  if (callback === undefined) return n;
  process.nextTick(() => callback(null, n));
}

function randomUUID(options) {
  const b = wasi.getRandomBytes(16);
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  const hex = Buffer.from(b).toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

function getRandomValues(data) {
  const kind = __intrinsic_typed_kind(data);
  // Integer typed arrays only (not Float32/Float64Array).
  if (kind < 0 || data instanceof Float32Array || data instanceof Float64Array) {
    throw domError('TypeMismatchError', 17, 'The data argument must be an integer-type TypedArray');
  }
  if (data.byteLength > 65536) {
    throw domError('QuotaExceededError', 22, 'The requested length exceeds 65,536 bytes');
  }
  fillRandom(new Uint8Array(data.buffer, data.byteOffset, data.byteLength), 0, data.byteLength);
  return data;
}

// `crypto.subtle`: present, every operation rejects (planned: F280).
class SubtleCrypto {}
for (const name of ['encrypt', 'decrypt', 'sign', 'verify', 'digest', 'generateKey', 'deriveKey', 'deriveBits',
  'importKey', 'exportKey', 'wrapKey', 'unwrapKey']) {
  Object.defineProperty(SubtleCrypto.prototype, name, {
    value() {
      return Promise.reject(domError('NotSupportedError', 9, `crypto.subtle.${name} isn't available in js2wasi3`));
    },
    writable: true, configurable: true,
  });
}
Object.defineProperty(SubtleCrypto.prototype, Symbol.toStringTag, { value: 'SubtleCrypto', configurable: true });
const subtle = new SubtleCrypto();

class Crypto {
  get subtle() {
    return subtle;
  }

  getRandomValues(data) {
    return getRandomValues(data);
  }

  randomUUID() {
    return randomUUID();
  }
}
Object.defineProperty(Crypto.prototype, Symbol.toStringTag, { value: 'Crypto', configurable: true });
const webcrypto = new Crypto();

// --- Hashes ---

function rotl(x, n) {
  return (x << n) | (x >>> (32 - n));
}

function rotr(x, n) {
  return (x >>> n) | (x << (32 - n));
}

// The message padded to a multiple of 64 bytes (`block` 64) or 128, with its bit length at the
// end, big- or little-endian.
function pad(bytes, block, littleEndian) {
  const lenBytes = block === 128 ? 16 : 8;
  const n = Math.ceil((bytes.length + 1 + lenBytes) / block) * block;
  const out = new Uint8Array(n);
  out.set(bytes);
  out[bytes.length] = 0x80;
  const bits = bytes.length * 8;
  const hi = Math.floor(bits / 4294967296);
  const lo = bits >>> 0;
  if (littleEndian) {
    for (let i = 0; i < 4; i++) {
      out[n - 8 + i] = (lo >>> (8 * i)) & 0xff;
      out[n - 4 + i] = (hi >>> (8 * i)) & 0xff;
    }
  } else {
    for (let i = 0; i < 4; i++) {
      out[n - 1 - i] = (lo >>> (8 * i)) & 0xff;
      out[n - 5 - i] = (hi >>> (8 * i)) & 0xff;
    }
  }
  return out;
}

function be32(b, i) {
  return ((b[i] << 24) | (b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3]) | 0;
}

function wordsOut(words, count, littleEndian) {
  const out = new Uint8Array(count * 4);
  for (let i = 0; i < count; i++) {
    const w = words[i];
    for (let j = 0; j < 4; j++) {
      out[i * 4 + j] = littleEndian ? (w >>> (8 * j)) & 0xff : (w >>> (24 - 8 * j)) & 0xff;
    }
  }
  return out;
}

const MD5_S = [7, 12, 17, 22, 5, 9, 14, 20, 4, 11, 16, 23, 6, 10, 15, 21];
const MD5_K = [];
for (let i = 0; i < 64; i++) MD5_K.push(Math.floor(Math.abs(Math.sin(i + 1)) * 4294967296) | 0);

function md5(bytes) {
  const m = pad(bytes, 64, true);
  let a0 = 0x67452301, b0 = 0xefcdab89 | 0, c0 = 0x98badcfe | 0, d0 = 0x10325476;
  const x = new Int32Array(16);
  for (let off = 0; off < m.length; off += 64) {
    for (let i = 0; i < 16; i++) {
      const j = off + i * 4;
      x[i] = m[j] | (m[j + 1] << 8) | (m[j + 2] << 16) | (m[j + 3] << 24);
    }
    let a = a0, b = b0, c = c0, d = d0;
    for (let i = 0; i < 64; i++) {
      let f, g;
      if (i < 16) { f = (b & c) | (~b & d); g = i; }
      else if (i < 32) { f = (d & b) | (~d & c); g = (5 * i + 1) & 15; }
      else if (i < 48) { f = b ^ c ^ d; g = (3 * i + 5) & 15; }
      else { f = c ^ (b | ~d); g = (7 * i) & 15; }
      const t = d;
      d = c;
      c = b;
      b = (b + rotl((a + f + MD5_K[i] + x[g]) | 0, MD5_S[(i >> 4) * 4 + (i & 3)])) | 0;
      a = t;
    }
    a0 = (a0 + a) | 0; b0 = (b0 + b) | 0; c0 = (c0 + c) | 0; d0 = (d0 + d) | 0;
  }
  return wordsOut([a0, b0, c0, d0], 4, true);
}

function sha1(bytes) {
  const m = pad(bytes, 64, false);
  const h = [0x67452301, 0xefcdab89 | 0, 0x98badcfe | 0, 0x10325476, 0xc3d2e1f0 | 0];
  const w = new Int32Array(80);
  for (let off = 0; off < m.length; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = be32(m, off + i * 4);
    for (let i = 16; i < 80; i++) w[i] = rotl(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
    let [a, b, c, d, e] = h;
    for (let i = 0; i < 80; i++) {
      let f, k;
      if (i < 20) { f = (b & c) | (~b & d); k = 0x5a827999; }
      else if (i < 40) { f = b ^ c ^ d; k = 0x6ed9eba1; }
      else if (i < 60) { f = (b & c) | (b & d) | (c & d); k = 0x8f1bbcdc | 0; }
      else { f = b ^ c ^ d; k = 0xca62c1d6 | 0; }
      const t = (rotl(a, 5) + f + e + k + w[i]) | 0;
      e = d; d = c; c = rotl(b, 30); b = a; a = t;
    }
    h[0] = (h[0] + a) | 0; h[1] = (h[1] + b) | 0; h[2] = (h[2] + c) | 0; h[3] = (h[3] + d) | 0; h[4] = (h[4] + e) | 0;
  }
  return wordsOut(h, 5, false);
}

const K256 = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

function sha256(bytes, is224) {
  const m = pad(bytes, 64, false);
  const h = is224
    ? [0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7, 0xbefa4fa4]
    : [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
  for (let i = 0; i < 8; i++) h[i] |= 0;
  const w = new Int32Array(64);
  for (let off = 0; off < m.length; off += 64) {
    for (let i = 0; i < 16; i++) w[i] = be32(m, off + i * 4);
    for (let i = 16; i < 64; i++) {
      const s0 = rotr(w[i - 15], 7) ^ rotr(w[i - 15], 18) ^ (w[i - 15] >>> 3);
      const s1 = rotr(w[i - 2], 17) ^ rotr(w[i - 2], 19) ^ (w[i - 2] >>> 10);
      w[i] = (w[i - 16] + s0 + w[i - 7] + s1) | 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let i = 0; i < 64; i++) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K256[i] + w[i]) | 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) | 0;
      hh = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
    }
    h[0] = (h[0] + a) | 0; h[1] = (h[1] + b) | 0; h[2] = (h[2] + c) | 0; h[3] = (h[3] + d) | 0;
    h[4] = (h[4] + e) | 0; h[5] = (h[5] + f) | 0; h[6] = (h[6] + g) | 0; h[7] = (h[7] + hh) | 0;
  }
  return wordsOut(h, is224 ? 7 : 8, false);
}

// SHA-512's constants as (high, low) 32-bit pairs.
const K512 = [
  0x428a2f98, 0xd728ae22, 0x71374491, 0x23ef65cd, 0xb5c0fbcf, 0xec4d3b2f, 0xe9b5dba5, 0x8189dbbc,
  0x3956c25b, 0xf348b538, 0x59f111f1, 0xb605d019, 0x923f82a4, 0xaf194f9b, 0xab1c5ed5, 0xda6d8118,
  0xd807aa98, 0xa3030242, 0x12835b01, 0x45706fbe, 0x243185be, 0x4ee4b28c, 0x550c7dc3, 0xd5ffb4e2,
  0x72be5d74, 0xf27b896f, 0x80deb1fe, 0x3b1696b1, 0x9bdc06a7, 0x25c71235, 0xc19bf174, 0xcf692694,
  0xe49b69c1, 0x9ef14ad2, 0xefbe4786, 0x384f25e3, 0x0fc19dc6, 0x8b8cd5b5, 0x240ca1cc, 0x77ac9c65,
  0x2de92c6f, 0x592b0275, 0x4a7484aa, 0x6ea6e483, 0x5cb0a9dc, 0xbd41fbd4, 0x76f988da, 0x831153b5,
  0x983e5152, 0xee66dfab, 0xa831c66d, 0x2db43210, 0xb00327c8, 0x98fb213f, 0xbf597fc7, 0xbeef0ee4,
  0xc6e00bf3, 0x3da88fc2, 0xd5a79147, 0x930aa725, 0x06ca6351, 0xe003826f, 0x14292967, 0x0a0e6e70,
  0x27b70a85, 0x46d22ffc, 0x2e1b2138, 0x5c26c926, 0x4d2c6dfc, 0x5ac42aed, 0x53380d13, 0x9d95b3df,
  0x650a7354, 0x8baf63de, 0x766a0abb, 0x3c77b2a8, 0x81c2c92e, 0x47edaee6, 0x92722c85, 0x1482353b,
  0xa2bfe8a1, 0x4cf10364, 0xa81a664b, 0xbc423001, 0xc24b8b70, 0xd0f89791, 0xc76c51a3, 0x0654be30,
  0xd192e819, 0xd6ef5218, 0xd6990624, 0x5565a910, 0xf40e3585, 0x5771202a, 0x106aa070, 0x32bbd1b8,
  0x19a4c116, 0xb8d2d0c8, 0x1e376c08, 0x5141ab53, 0x2748774c, 0xdf8eeb99, 0x34b0bcb5, 0xe19b48a8,
  0x391c0cb3, 0xc5c95a63, 0x4ed8aa4a, 0xe3418acb, 0x5b9cca4f, 0x7763e373, 0x682e6ff3, 0xd6b2b8a3,
  0x748f82ee, 0x5defb2fc, 0x78a5636f, 0x43172f60, 0x84c87814, 0xa1f0ab72, 0x8cc70208, 0x1a6439ec,
  0x90befffa, 0x23631e28, 0xa4506ceb, 0xde82bde9, 0xbef9a3f7, 0xb2c67915, 0xc67178f2, 0xe372532b,
  0xca273ece, 0xea26619c, 0xd186b8c7, 0x21c0c207, 0xeada7dd6, 0xcde0eb1e, 0xf57d4f7f, 0xee6ed178,
  0x06f067aa, 0x72176fba, 0x0a637dc5, 0xa2c898a6, 0x113f9804, 0xbef90dae, 0x1b710b35, 0x131c471b,
  0x28db77f5, 0x23047d84, 0x32caab7b, 0x40c72493, 0x3c9ebe0a, 0x15c9bebc, 0x431d67c4, 0x9c100d4c,
  0x4cc5d4be, 0xcb3e42b6, 0x597f299c, 0xfc657e2a, 0x5fcb6fab, 0x3ad6faec, 0x6c44198c, 0x4a475817,
];

const IV512 = [
  0x6a09e667, 0xf3bcc908, 0xbb67ae85, 0x84caa73b, 0x3c6ef372, 0xfe94f82b, 0xa54ff53a, 0x5f1d36f1,
  0x510e527f, 0xade682d1, 0x9b05688c, 0x2b3e6c1f, 0x1f83d9ab, 0xfb41bd6b, 0x5be0cd19, 0x137e2179,
];
const IV384 = [
  0xcbbb9d5d, 0xc1059ed8, 0x629a292a, 0x367cd507, 0x9159015a, 0x3070dd17, 0x152fecd8, 0xf70e5939,
  0x67332667, 0xffc00b31, 0x8eb44a87, 0x68581511, 0xdb0c2e0d, 0x64f98fa7, 0x47b5481d, 0xbefa4fa4,
];

function sha512(bytes, is384) {
  const m = pad(bytes, 128, false);
  const h = new Int32Array(is384 ? IV384 : IV512);
  const w = new Int32Array(160);
  // (hi, lo) of x rotated right by n, for 0 < n < 32 and 32 <= n < 64.
  let rh = 0, rl = 0;
  const rotr64 = (hi, lo, n) => {
    if (n < 32) {
      rh = (hi >>> n) | (lo << (32 - n));
      rl = (lo >>> n) | (hi << (32 - n));
    } else {
      n -= 32;
      rh = n === 0 ? lo : (lo >>> n) | (hi << (32 - n));
      rl = n === 0 ? hi : (hi >>> n) | (lo << (32 - n));
    }
  };
  // Sums of (hi, lo) pairs.
  let sh = 0, sl = 0;
  const add = (hi, lo) => {
    const l = (sl >>> 0) + (lo >>> 0);
    sh = (sh + hi + Math.floor(l / 4294967296)) | 0;
    sl = l | 0;
  };
  for (let off = 0; off < m.length; off += 128) {
    for (let i = 0; i < 32; i++) w[i] = be32(m, off + i * 4);
    for (let i = 16; i < 80; i++) {
      const xh = w[(i - 15) * 2], xl = w[(i - 15) * 2 + 1];
      rotr64(xh, xl, 1); let s0h = rh, s0l = rl;
      rotr64(xh, xl, 8); s0h ^= rh; s0l ^= rl;
      s0h ^= xh >>> 7; s0l ^= (xl >>> 7) | (xh << 25);
      const yh = w[(i - 2) * 2], yl = w[(i - 2) * 2 + 1];
      rotr64(yh, yl, 19); let s1h = rh, s1l = rl;
      rotr64(yh, yl, 61); s1h ^= rh; s1l ^= rl;
      s1h ^= yh >>> 6; s1l ^= (yl >>> 6) | (yh << 26);
      sh = w[(i - 16) * 2]; sl = w[(i - 16) * 2 + 1];
      add(s0h, s0l);
      add(w[(i - 7) * 2], w[(i - 7) * 2 + 1]);
      add(s1h, s1l);
      w[i * 2] = sh; w[i * 2 + 1] = sl;
    }
    let ah = h[0], al = h[1], bh = h[2], bl = h[3], ch = h[4], cl = h[5], dh = h[6], dl = h[7];
    let eh = h[8], el = h[9], fh = h[10], fl = h[11], gh = h[12], gl = h[13], hh = h[14], hl = h[15];
    for (let i = 0; i < 80; i++) {
      rotr64(eh, el, 14); let S1h = rh, S1l = rl;
      rotr64(eh, el, 18); S1h ^= rh; S1l ^= rl;
      rotr64(eh, el, 41); S1h ^= rh; S1l ^= rl;
      const chh = (eh & fh) ^ (~eh & gh), chl = (el & fl) ^ (~el & gl);
      sh = hh; sl = hl;
      add(S1h, S1l);
      add(chh, chl);
      add(K512[i * 2] | 0, K512[i * 2 + 1] | 0);
      add(w[i * 2], w[i * 2 + 1]);
      const t1h = sh, t1l = sl;
      rotr64(ah, al, 28); let S0h = rh, S0l = rl;
      rotr64(ah, al, 34); S0h ^= rh; S0l ^= rl;
      rotr64(ah, al, 39); S0h ^= rh; S0l ^= rl;
      const majh = (ah & bh) ^ (ah & ch) ^ (bh & ch), majl = (al & bl) ^ (al & cl) ^ (bl & cl);
      hh = gh; hl = gl; gh = fh; gl = fl; fh = eh; fl = el;
      sh = dh; sl = dl; add(t1h, t1l); eh = sh; el = sl;
      dh = ch; dl = cl; ch = bh; cl = bl; bh = ah; bl = al;
      sh = t1h; sl = t1l; add(S0h, S0l); add(majh, majl); ah = sh; al = sl;
    }
    const vals = [ah, al, bh, bl, ch, cl, dh, dl, eh, el, fh, fl, gh, gl, hh, hl];
    for (let i = 0; i < 16; i += 2) {
      sh = h[i]; sl = h[i + 1];
      add(vals[i], vals[i + 1]);
      h[i] = sh; h[i + 1] = sl;
    }
  }
  return wordsOut(h, is384 ? 12 : 16, false);
}

// name → [digest(bytes), block size]
const HASHES = {
  md5: [md5, 64],
  sha1: [sha1, 64],
  sha224: [(b) => sha256(b, true), 64],
  sha256: [(b) => sha256(b, false), 64],
  sha384: [(b) => sha512(b, true), 128],
  sha512: [(b) => sha512(b, false), 128],
};
const ALIASES = { 'sha-1': 'sha1', 'sha-224': 'sha224', 'sha-256': 'sha256', 'sha-384': 'sha384', 'sha-512': 'sha512' };

function hashFunction(algorithm) {
  if (typeof algorithm !== 'string') throw $$errInvalidArgType('algorithm', 'of type string', algorithm);
  let name = algorithm.toLowerCase();
  if (ALIASES[name] !== undefined) name = ALIASES[name];
  const entry = Object.prototype.hasOwnProperty.call(HASHES, name) ? HASHES[name] : undefined;
  if (entry === undefined) throw new Error('Digest method not supported');
  return entry;
}

function finalized() {
  return codeError(Error, 'ERR_CRYPTO_HASH_FINALIZED', 'Digest already called');
}

class Hash {
  #fn;
  #chunks = [];
  #done = false;

  constructor(algorithm, options) {
    this.#fn = hashFunction(algorithm)[0];
  }

  update(data, encoding) {
    if (this.#done) throw finalized();
    this.#chunks.push(Uint8Array.from(bytesOf(data, encoding, 'data', DATA_TYPES)));
    return this;
  }

  digest(encoding) {
    if (this.#done) throw finalized();
    this.#done = true;
    return output(this.#fn(concat(this.#chunks)), encoding);
  }

  copy() {
    if (this.#done) throw finalized();
    const h = new Hash('md5');
    h.#fn = this.#fn;
    h.#chunks = this.#chunks.slice();
    return h;
  }
}

class Hmac {
  #fn;
  #inner;
  #outer;
  #chunks = [];
  #done = false;

  constructor(algorithm, key, options) {
    const [fn, block] = hashFunction(algorithm);
    if (key === undefined || key === null || (typeof key !== 'string' && !ArrayBuffer.isView(key) && !(key instanceof ArrayBuffer))) {
      throw $$errInvalidArgType('key', 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, DataView, KeyObject, or CryptoKey', key);
    }
    let k = bytesOf(key, undefined, 'key', 'ArrayBuffer,');
    if (k.length > block) k = fn(k);
    const ipad = new Uint8Array(block), opad = new Uint8Array(block);
    for (let i = 0; i < block; i++) {
      const b = i < k.length ? k[i] : 0;
      ipad[i] = b ^ 0x36;
      opad[i] = b ^ 0x5c;
    }
    this.#fn = fn;
    this.#inner = ipad;
    this.#outer = opad;
  }

  update(data, encoding) {
    if (this.#done) throw codeError(Error, 'ERR_CRYPTO_HASH_FINALIZED', 'Digest already called');
    this.#chunks.push(Uint8Array.from(bytesOf(data, encoding, 'data', DATA_TYPES)));
    return this;
  }

  digest(encoding) {
    if (this.#done) return output(new Uint8Array(0), encoding);
    this.#done = true;
    const inner = this.#fn(concat([this.#inner, ...this.#chunks]));
    return output(this.#fn(concat([this.#outer, inner])), encoding);
  }
}

function createHash(algorithm, options) {
  return new Hash(algorithm, options);
}

function createHmac(algorithm, key, options) {
  return new Hmac(algorithm, key, options);
}

function hash(algorithm, data, outputEncoding = 'hex') {
  if (typeof data !== 'string' && !ArrayBuffer.isView(data)) {
    throw $$errInvalidArgType('data', 'of type string or an instance of Buffer, TypedArray, or DataView', data);
  }
  return new Hash(algorithm).update(data).digest(outputEncoding);
}

function getHashes() {
  return ['md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512'];
}

// --- AES ---

const SBOX = new Uint8Array(256);
const INV_SBOX = new Uint8Array(256);
const T0 = new Int32Array(256), T1 = new Int32Array(256), T2 = new Int32Array(256), T3 = new Int32Array(256);
const D0 = new Int32Array(256), D1 = new Int32Array(256), D2 = new Int32Array(256), D3 = new Int32Array(256);
{
  // GF(2^8) exp/log tables with generator 3.
  const exp = new Uint8Array(256), log = new Uint8Array(256);
  for (let i = 0, x = 1; i < 255; i++) {
    exp[i] = x;
    log[x] = i;
    x ^= (x << 1) ^ (x & 0x80 ? 0x11b : 0);
    x &= 0xff;
  }
  const mul = (a, b) => (a === 0 || b === 0 ? 0 : exp[(log[a] + log[b]) % 255]);
  for (let i = 0; i < 256; i++) {
    let s = i === 0 ? 0 : exp[(255 - log[i]) % 255];
    s = s ^ ((s << 1) | (s >>> 7)) ^ ((s << 2) | (s >>> 6)) ^ ((s << 3) | (s >>> 5)) ^ ((s << 4) | (s >>> 4)) ^ 0x63;
    s &= 0xff;
    SBOX[i] = s;
    INV_SBOX[s] = i;
  }
  for (let i = 0; i < 256; i++) {
    const s = SBOX[i];
    const t = (mul(s, 2) << 24) | (s << 16) | (s << 8) | mul(s, 3);
    T0[i] = t; T1[i] = rotr(t, 8); T2[i] = rotr(t, 16); T3[i] = rotr(t, 24);
    const v = INV_SBOX[i];
    const d = (mul(v, 14) << 24) | (mul(v, 9) << 16) | (mul(v, 13) << 8) | mul(v, 11);
    D0[i] = d; D1[i] = rotr(d, 8); D2[i] = rotr(d, 16); D3[i] = rotr(d, 24);
  }
}

// The encryption and decryption round keys of `key` (16, 24 or 32 bytes).
function expandKey(key) {
  const nk = key.length / 4;
  const rounds = nk + 6;
  const total = 4 * (rounds + 1);
  const ek = new Int32Array(total);
  for (let i = 0; i < nk; i++) ek[i] = be32(key, i * 4);
  let rcon = 1;
  for (let i = nk; i < total; i++) {
    let t = ek[i - 1];
    if (i % nk === 0) {
      t = (SBOX[(t >>> 16) & 0xff] << 24) | (SBOX[(t >>> 8) & 0xff] << 16) | (SBOX[t & 0xff] << 8) | SBOX[t >>> 24];
      t ^= rcon << 24;
      rcon = (rcon << 1) ^ (rcon & 0x80 ? 0x11b : 0);
    } else if (nk > 6 && i % nk === 4) {
      t = (SBOX[t >>> 24] << 24) | (SBOX[(t >>> 16) & 0xff] << 16) | (SBOX[(t >>> 8) & 0xff] << 8) | SBOX[t & 0xff];
    }
    ek[i] = ek[i - nk] ^ t;
  }
  // Decryption keys: reversed rounds, InvMixColumns on the middle ones.
  const dk = new Int32Array(total);
  for (let r = 0; r <= rounds; r++) {
    for (let c = 0; c < 4; c++) {
      const w = ek[(rounds - r) * 4 + c];
      dk[r * 4 + c] = r === 0 || r === rounds ? w
        : D0[SBOX[w >>> 24]] ^ D1[SBOX[(w >>> 16) & 0xff]] ^ D2[SBOX[(w >>> 8) & 0xff]] ^ D3[SBOX[w & 0xff]];
    }
  }
  return { rounds, ek, dk };
}

function cryptBlock(k, input, inOff, out, outOff, decrypt) {
  const keys = decrypt ? k.dk : k.ek;
  const A = decrypt ? D0 : T0, B = decrypt ? D1 : T1, C = decrypt ? D2 : T2, D = decrypt ? D3 : T3;
  const S = decrypt ? INV_SBOX : SBOX;
  let s0 = be32(input, inOff) ^ keys[0];
  let s1 = be32(input, inOff + 4) ^ keys[1];
  let s2 = be32(input, inOff + 8) ^ keys[2];
  let s3 = be32(input, inOff + 12) ^ keys[3];
  let ki = 4;
  for (let r = 1; r < k.rounds; r++) {
    let t0, t1, t2, t3;
    if (decrypt) {
      t0 = A[s0 >>> 24] ^ B[(s3 >>> 16) & 0xff] ^ C[(s2 >>> 8) & 0xff] ^ D[s1 & 0xff] ^ keys[ki];
      t1 = A[s1 >>> 24] ^ B[(s0 >>> 16) & 0xff] ^ C[(s3 >>> 8) & 0xff] ^ D[s2 & 0xff] ^ keys[ki + 1];
      t2 = A[s2 >>> 24] ^ B[(s1 >>> 16) & 0xff] ^ C[(s0 >>> 8) & 0xff] ^ D[s3 & 0xff] ^ keys[ki + 2];
      t3 = A[s3 >>> 24] ^ B[(s2 >>> 16) & 0xff] ^ C[(s1 >>> 8) & 0xff] ^ D[s0 & 0xff] ^ keys[ki + 3];
    } else {
      t0 = A[s0 >>> 24] ^ B[(s1 >>> 16) & 0xff] ^ C[(s2 >>> 8) & 0xff] ^ D[s3 & 0xff] ^ keys[ki];
      t1 = A[s1 >>> 24] ^ B[(s2 >>> 16) & 0xff] ^ C[(s3 >>> 8) & 0xff] ^ D[s0 & 0xff] ^ keys[ki + 1];
      t2 = A[s2 >>> 24] ^ B[(s3 >>> 16) & 0xff] ^ C[(s0 >>> 8) & 0xff] ^ D[s1 & 0xff] ^ keys[ki + 2];
      t3 = A[s3 >>> 24] ^ B[(s0 >>> 16) & 0xff] ^ C[(s1 >>> 8) & 0xff] ^ D[s2 & 0xff] ^ keys[ki + 3];
    }
    s0 = t0; s1 = t1; s2 = t2; s3 = t3;
    ki += 4;
  }
  const st = [s0, s1, s2, s3];
  for (let c = 0; c < 4; c++) {
    const a = st[c];
    const b = decrypt ? st[(c + 3) & 3] : st[(c + 1) & 3];
    const cc = st[(c + 2) & 3];
    const d = decrypt ? st[(c + 1) & 3] : st[(c + 3) & 3];
    const w = ((S[a >>> 24] << 24) | (S[(b >>> 16) & 0xff] << 16) | (S[(cc >>> 8) & 0xff] << 8) | S[d & 0xff]) ^ keys[ki + c];
    out[outOff + c * 4] = w >>> 24;
    out[outOff + c * 4 + 1] = (w >>> 16) & 0xff;
    out[outOff + c * 4 + 2] = (w >>> 8) & 0xff;
    out[outOff + c * 4 + 3] = w & 0xff;
  }
}

// `aes-<bits>-<mode>` → [key length, mode]
function cipherInfo(name) {
  const m = /^(?:id-)?aes-?(128|192|256)-(ecb|cbc|ctr|ccm)$/.exec(name.toLowerCase());
  if (m === null) {
    const short = /^aes(128|192|256)$/.exec(name.toLowerCase());
    if (short !== null) return [Number(short[1]) / 8, 'cbc'];
    return undefined;
  }
  return [Number(m[1]) / 8, m[2]];
}

function getCiphers() {
  const out = [];
  for (const bits of [128, 192, 256]) for (const mode of ['cbc', 'ccm', 'ctr', 'ecb']) out.push(`aes-${bits}-${mode}`);
  return out;
}

function badDecrypt() {
  const e = new Error('error:1C800064:Provider routines::bad decrypt');
  e.code = 'ERR_OSSL_BAD_DECRYPT';
  e.library = 'Provider routines';
  e.reason = 'bad decrypt';
  return e;
}

function wrongFinalLength() {
  const e = new Error('error:1C80006B:Provider routines::wrong final block length');
  e.code = 'ERR_OSSL_WRONG_FINAL_BLOCK_LENGTH';
  e.library = 'Provider routines';
  e.reason = 'wrong final block length';
  return e;
}

class CipherBase {
  #decrypt;
  #mode;
  #key;
  #iv;
  #pending = new Uint8Array(0);
  #autoPadding = true;
  #done = false;
  #encoding = undefined;
  // CTR: the counter block, its key stream and the position in it.
  #counter;
  #stream = new Uint8Array(16);
  #streamAt = 16;
  // CCM.
  #tagLength;
  #aad = new Uint8Array(0);
  #plaintextLength;
  #tag;
  #ccmOk = true;

  constructor(decrypt, cipher, key, iv, options) {
    this.#decrypt = decrypt;
    if (typeof cipher !== 'string') throw $$errInvalidArgType('cipher', 'of type string', cipher);
    const info = cipherInfo(cipher);
    if (info === undefined) throw codeError(Error, 'ERR_CRYPTO_UNKNOWN_CIPHER', 'Unknown cipher');
    const [keyLength, mode] = info;
    const keyBytes = bytesOf(key, undefined, 'key', 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, DataView, KeyObject, or CryptoKey');
    const ivBytes = iv === null ? null : bytesOf(iv, undefined, 'iv', 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, or DataView');
    const ivOk = mode === 'ecb' ? ivBytes === null || ivBytes.length === 0
      : mode === 'ccm' ? ivBytes !== null && ivBytes.length >= 7 && ivBytes.length <= 13
        : ivBytes !== null && ivBytes.length === 16;
    if (!ivOk) throw codeError(TypeError, 'ERR_CRYPTO_INVALID_IV', 'Invalid initialization vector');
    if (keyBytes.length !== keyLength) throw codeError(RangeError, 'ERR_CRYPTO_INVALID_KEYLEN', 'Invalid key length');
    if (mode === 'ccm') {
      const t = options !== null && typeof options === 'object' ? options.authTagLength : undefined;
      if (t === undefined) {
        throw codeError(TypeError, 'ERR_CRYPTO_INVALID_AUTH_TAG', `authTagLength required for ${cipher}`);
      }
      if (![4, 6, 8, 10, 12, 14, 16].includes(t)) {
        throw codeError(TypeError, 'ERR_CRYPTO_INVALID_AUTH_TAG', `Invalid authentication tag length: ${t}`);
      }
      this.#tagLength = t;
    }
    this.#mode = mode;
    this.#key = expandKey(Uint8Array.from(keyBytes));
    this.#iv = ivBytes === null ? null : Uint8Array.from(ivBytes);
    if (mode === 'ctr') this.#counter = Uint8Array.from(ivBytes);
  }

  #output(bytes, encoding) {
    if (encoding !== undefined && encoding !== 'buffer') {
      if (this.#encoding !== undefined && this.#encoding !== encoding) {
        throw codeError(TypeError, 'ERR_INVALID_ARG_VALUE',
          `The argument 'outputEncoding' cannot be changed from '${this.#encoding}'. Received '${encoding}'`);
      }
      this.#encoding = encoding;
    }
    return output(bytes, encoding);
  }

  update(data, inputEncoding, outputEncoding) {
    if (this.#done) throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state for operation update');
    const input = bytesOf(data, inputEncoding, 'data', DATA_TYPES);
    let out;
    switch (this.#mode) {
      case 'ctr': out = this.#ctr(input); break;
      case 'ccm': out = this.#ccm(input); break;
      default: out = this.#blocks(input, false);
    }
    return this.#output(out, outputEncoding);
  }

  final(outputEncoding) {
    if (this.#done) throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state for operation final');
    this.#done = true;
    let out = new Uint8Array(0);
    if (this.#mode === 'ccm') {
      if (this.#decrypt && !this.#ccmOk) throw new Error('Unsupported state or unable to authenticate data');
      if (!this.#decrypt && this.#tag === undefined) this.#ccm(new Uint8Array(0));
    } else if (this.#mode !== 'ctr') {
      out = this.#blocks(new Uint8Array(0), true);
    }
    return this.#output(out, outputEncoding);
  }

  setAutoPadding(autoPadding = true) {
    this.#autoPadding = !!autoPadding;
    return this;
  }

  setAAD(buffer, options) {
    this.#aad = Uint8Array.from(bytesOf(buffer, undefined, 'buffer', 'an instance of ArrayBuffer, Buffer, TypedArray, or DataView'));
    if (options !== null && typeof options === 'object' && options.plaintextLength !== undefined) {
      this.#plaintextLength = options.plaintextLength;
    }
    return this;
  }

  getAuthTag() {
    if (this.#decrypt || this.#tag === undefined || !this.#done) {
      throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state for operation getAuthTag');
    }
    return Buffer.from(this.#tag);
  }

  setAuthTag(tag) {
    if (!this.#decrypt || this.#done) {
      throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state for operation setAuthTag');
    }
    const t = bytesOf(tag, undefined, 'buffer', 'an instance of Buffer, TypedArray, or DataView');
    if (this.#mode === 'ccm' && t.length !== this.#tagLength) {
      throw codeError(TypeError, 'ERR_CRYPTO_INVALID_AUTH_TAG', `Invalid authentication tag length: ${t.length}`);
    }
    this.#tag = Uint8Array.from(t);
    return this;
  }

  // ECB/CBC: whole blocks now; on `final`, the padded (or unpadded) last one.
  #blocks(input, last) {
    const all = new Uint8Array(this.#pending.length + input.length);
    all.set(this.#pending);
    all.set(input, this.#pending.length);
    let usable;
    if (!last) {
      usable = all.length - (all.length % 16);
      // A decipher with padding keeps the last block for `final`.
      if (this.#decrypt && this.#autoPadding && usable === all.length && usable > 0) usable -= 16;
      this.#pending = all.slice(usable);
    } else {
      if (!this.#decrypt && this.#autoPadding) {
        const n = 16 - (all.length % 16);
        const padded = new Uint8Array(all.length + n);
        padded.set(all);
        padded.fill(n, all.length);
        return this.#crypt(padded, padded.length);
      }
      if (all.length % 16 !== 0) throw wrongFinalLength();
      usable = all.length;
      this.#pending = new Uint8Array(0);
      const out = this.#crypt(all, usable);
      if (this.#decrypt && this.#autoPadding) {
        if (out.length === 0) throw wrongFinalLength();
        const n = out[out.length - 1];
        if (n === 0 || n > 16) throw badDecrypt();
        for (let i = out.length - n; i < out.length; i++) if (out[i] !== n) throw badDecrypt();
        return out.subarray(0, out.length - n);
      }
      return out;
    }
    return this.#crypt(all, usable);
  }

  #crypt(data, n) {
    const out = new Uint8Array(n);
    const k = this.#key;
    const cbc = this.#mode === 'cbc';
    for (let off = 0; off < n; off += 16) {
      if (!this.#decrypt) {
        if (cbc) {
          const block = new Uint8Array(16);
          for (let i = 0; i < 16; i++) block[i] = data[off + i] ^ this.#iv[i];
          cryptBlock(k, block, 0, out, off, false);
          this.#iv = out.slice(off, off + 16);
        } else {
          cryptBlock(k, data, off, out, off, false);
        }
      } else {
        cryptBlock(k, data, off, out, off, true);
        if (cbc) {
          for (let i = 0; i < 16; i++) out[off + i] ^= this.#iv[i];
          this.#iv = data.slice(off, off + 16);
        }
      }
    }
    return out;
  }

  #ctr(input) {
    const out = new Uint8Array(input.length);
    for (let i = 0; i < input.length; i++) {
      if (this.#streamAt === 16) {
        cryptBlock(this.#key, this.#counter, 0, this.#stream, 0, false);
        for (let j = 15; j >= 0; j--) {
          this.#counter[j] = (this.#counter[j] + 1) & 0xff;
          if (this.#counter[j] !== 0) break;
        }
        this.#streamAt = 0;
      }
      out[i] = input[i] ^ this.#stream[this.#streamAt++];
    }
    return out;
  }

  // CCM (RFC 3610) over the whole message (node allows one `update`).
  #ccm(input) {
    const nonce = this.#iv;
    const L = 15 - nonce.length;
    const M = this.#tagLength;
    const k = this.#key;
    const block = (i) => {
      const a = new Uint8Array(16);
      a[0] = L - 1;
      a.set(nonce, 1);
      for (let j = 0; j < L; j++) a[15 - j] = Math.floor(i / 2 ** (8 * j)) & 0xff;
      return a;
    };
    // Key stream: blocks A_1, A_2, …; A_0 encrypts the tag.
    const out = new Uint8Array(input.length);
    const s = new Uint8Array(16);
    for (let off = 0; off < input.length; off += 16) {
      cryptBlock(k, block(off / 16 + 1), 0, s, 0, false);
      for (let i = 0; i < 16 && off + i < input.length; i++) out[off + i] = input[off + i] ^ s[i];
    }
    const plain = this.#decrypt ? out : input;
    // CBC-MAC of B_0, the AAD (length-prefixed) and the plaintext.
    const b0 = new Uint8Array(16);
    b0[0] = (this.#aad.length > 0 ? 0x40 : 0) | (((M - 2) / 2) << 3) | (L - 1);
    b0.set(nonce, 1);
    for (let j = 0; j < L; j++) b0[15 - j] = Math.floor(plain.length / 2 ** (8 * j)) & 0xff;
    const parts = [b0];
    if (this.#aad.length > 0) {
      const n = this.#aad.length;
      const head = n < 0xff00 ? [n >> 8, n & 0xff] : [0xff, 0xfe, (n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff];
      const a = new Uint8Array(Math.ceil((head.length + n) / 16) * 16);
      a.set(head);
      a.set(this.#aad, head.length);
      parts.push(a);
    }
    const p = new Uint8Array(Math.ceil(plain.length / 16) * 16);
    p.set(plain);
    parts.push(p);
    const macInput = concat(parts);
    let x = new Uint8Array(16);
    for (let off = 0; off < macInput.length; off += 16) {
      for (let i = 0; i < 16; i++) x[i] ^= macInput[off + i];
      const y = new Uint8Array(16);
      cryptBlock(k, x, 0, y, 0, false);
      x = y;
    }
    cryptBlock(k, block(0), 0, s, 0, false);
    const tag = new Uint8Array(M);
    for (let i = 0; i < M; i++) tag[i] = x[i] ^ s[i];
    if (this.#decrypt) {
      let ok = this.#tag !== undefined && this.#tag.length === M;
      for (let i = 0; ok && i < M; i++) if (tag[i] !== this.#tag[i]) ok = false;
      this.#ccmOk = ok;
      // node returns no plaintext from an update whose tag doesn't match.
      if (!ok) return new Uint8Array(0);
    } else {
      this.#tag = tag;
    }
    return out;
  }
}

class Cipheriv extends CipherBase {
  constructor(cipher, key, iv, options) {
    super(false, cipher, key, iv, options);
  }
}

class Decipheriv extends CipherBase {
  constructor(cipher, key, iv, options) {
    super(true, cipher, key, iv, options);
  }
}

function createCipheriv(cipher, key, iv, options) {
  return new Cipheriv(cipher, key, iv, options);
}

function createDecipheriv(cipher, key, iv, options) {
  return new Decipheriv(cipher, key, iv, options);
}

function timingSafeEqual(a, b) {
  const x = viewBytes(a, 'buf1');
  const y = viewBytes(b, 'buf2');
  if (x.length !== y.length) {
    throw codeError(RangeError, 'ERR_CRYPTO_TIMING_SAFE_EQUAL_LENGTH', 'Input buffers must have the same byte length');
  }
  let d = 0;
  for (let i = 0; i < x.length; i++) d |= x[i] ^ y[i];
  return d === 0;
}

module.exports = {
  Hash, Hmac, Cipheriv, Decipheriv,
  createHash, createHmac, hash, getHashes,
  createCipheriv, createDecipheriv, getCiphers,
  randomBytes, pseudoRandomBytes: randomBytes, prng: randomBytes, rng: randomBytes,
  randomFill, randomFillSync, randomInt, randomUUID, getRandomValues,
  timingSafeEqual, webcrypto, Crypto,
  constants: {},
};
