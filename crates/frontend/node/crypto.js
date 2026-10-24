// node's `crypto` module (T65), a subset: random values from `wasi:random` (`randomBytes`,
// `randomFill(Sync)`, `randomInt`, `randomUUID`, `getRandomValues`, `webcrypto`), hashes (`md5`,
// `sha1`, `sha224`, `sha256`, `sha384`, `sha512`: `createHash`, `hash`, `createHmac`), AES with 128,
// 192 and 256-bit keys in ECB, CBC, CTR and CCM modes (`createCipheriv`/`createDecipheriv`) and
// `timingSafeEqual`, with node's errors. Everything else is missing (F280).
'use strict';

const { Buffer } = require('buffer');
const wasi = require('node:internal/wasi_random');
// `pbkdf2`, `scrypt`, `hkdf` and `_ext` (more digests, AES-GCM and ChaCha20 over the Rust helper) when the helper is linked
// (T149/T151: `link::link`); an empty module otherwise.
const { _ext: ext, ...kdfExports } = require('node:internal/crypto_kdf');
const { HASH_INFO, canonicalHash } = require('node:internal/crypto_hashes');
// `LazyTransform`, when `node:stream` is linked for another reason (T153): then Hash, Hmac and the ciphers are streams; else an empty module.
const streams = require('node:internal/crypto_streams');

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

// A copy of `bytes` as a plain `Uint8Array` (`Buffer#slice` is a view; `new Uint8Array(view)` and `Uint8Array.from` copy element by element).
function copyBytes(bytes) {
  const out = new Uint8Array(bytes.length);
  out.set(bytes);
  return out;
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

// UUID version 7 (RFC 9562): a 48-bit millisecond timestamp, 74 random bits.
function randomUUIDv7(options) {
  if (options !== undefined && (options === null || typeof options !== 'object')) {
    throw $$errInvalidArgType('options', 'of type object', options);
  }
  const b = wasi.getRandomBytes(16);
  const ms = Date.now();
  const hi = Math.floor(ms / 4294967296);
  const lo = ms >>> 0;
  b[0] = (hi >>> 8) & 0xff; b[1] = hi & 0xff;
  b[2] = (lo >>> 24) & 0xff; b[3] = (lo >>> 16) & 0xff; b[4] = (lo >>> 8) & 0xff; b[5] = lo & 0xff;
  b[6] = (b[6] & 0x0f) | 0x70;
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
function pad(bytes, block, littleEndian, total = bytes.length) {
  const lenBytes = block === 128 ? 16 : 8;
  const n = Math.ceil((bytes.length + 1 + lenBytes) / block) * block;
  const out = new Uint8Array(n);
  out.set(bytes);
  out[bytes.length] = 0x80;
  const bits = total * 8;
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

// The hashes run their compression functions natively (`*_blocks`, T152): the state is the digest's words as bytes (SHA-2: big-endian,
// MD5: little-endian), whole blocks go straight from the input, the padded tail through a second call.
function md5(bytes) {
  const state = wordsOut([0x67452301, 0xefcdab89 | 0, 0x98badcfe | 0, 0x10325476], 4, true);
  const full = bytes.length - (bytes.length % 64);
  if (full > 0) __intrinsic_md5_blocks(state, bytes, 0, full / 64);
  const tail = pad(bytes.subarray(full), 64, true, bytes.length);
  __intrinsic_md5_blocks(state, tail, 0, tail.length / 64);
  return state;
}

function sha1(bytes) {
  const state = wordsOut([0x67452301, 0xefcdab89 | 0, 0x98badcfe | 0, 0x10325476, 0xc3d2e1f0 | 0], 5, false);
  const full = bytes.length - (bytes.length % 64);
  if (full > 0) __intrinsic_sha1_blocks(state, bytes, 0, full / 64);
  const tail = pad(bytes.subarray(full), 64, false, bytes.length);
  __intrinsic_sha1_blocks(state, tail, 0, tail.length / 64);
  return state;
}

// SHA-256 and SHA-224: the compression function is native (`sha256_blocks`, T152); the state is the big-endian words as bytes.
function sha256(bytes, is224) {
  const state = wordsOut(is224
    ? [0xc1059ed8, 0x367cd507, 0x3070dd17, 0xf70e5939, 0xffc00b31, 0x68581511, 0x64f98fa7, 0xbefa4fa4]
    : [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19], 8, false);
  const full = bytes.length - (bytes.length % 64);
  if (full > 0) __intrinsic_sha256_blocks(state, bytes, 0, full / 64);
  const tail = pad(bytes.subarray(full), 64, false, bytes.length);
  __intrinsic_sha256_blocks(state, tail, 0, tail.length / 64);
  return state.slice(0, is224 ? 28 : 32);
}


const IV512 = [
  0x6a09e667, 0xf3bcc908, 0xbb67ae85, 0x84caa73b, 0x3c6ef372, 0xfe94f82b, 0xa54ff53a, 0x5f1d36f1,
  0x510e527f, 0xade682d1, 0x9b05688c, 0x2b3e6c1f, 0x1f83d9ab, 0xfb41bd6b, 0x5be0cd19, 0x137e2179,
];
const IV384 = [
  0xcbbb9d5d, 0xc1059ed8, 0x629a292a, 0x367cd507, 0x9159015a, 0x3070dd17, 0x152fecd8, 0xf70e5939,
  0x67332667, 0xffc00b31, 0x8eb44a87, 0x68581511, 0xdb0c2e0d, 0x64f98fa7, 0x47b5481d, 0xbefa4fa4,
];

function sha512State(state, bytes) {
  const full = bytes.length - (bytes.length % 128);
  if (full > 0) __intrinsic_sha512_blocks(state, bytes, 0, full / 128);
  const tail = pad(bytes.subarray(full), 128, false, bytes.length);
  __intrinsic_sha512_blocks(state, tail, 0, tail.length / 128);
  return state;
}

function sha512(bytes, is384) {
  return sha512State(wordsOut(is384 ? IV384 : IV512, 16, false), bytes).slice(0, is384 ? 48 : 64);
}

// SHA-512/224 and SHA-512/256 (FIPS 180-4 5.3.6): SHA-512 from the initial state with every byte xored with 0xa5 hashes "SHA-512/t"; the result is the
// initial state.
const SHA512T_IV = {};
function sha512t(bytes, t) {
  if (SHA512T_IV[t] === undefined) {
    const seed = wordsOut(IV512, 16, false);
    for (let i = 0; i < 64; i++) seed[i] ^= 0xa5;
    SHA512T_IV[t] = sha512State(seed, Buffer.from(`SHA-512/${t}`));
  }
  return sha512State(SHA512T_IV[t].slice(), bytes).slice(0, t / 8);
}

// Keccak (FIPS 202): `rate` bytes per block, the domain bits `suffix` (0x06 SHA-3, 0x1f SHAKE) and the 0x80 end bit go in the last block.
function keccak(bytes, rate, suffix, length) {
  const state = new Uint8Array(200);
  const full = bytes.length - (bytes.length % rate);
  if (full > 0) __intrinsic_keccak_blocks(state, bytes, 0, full / rate, rate);
  const tail = new Uint8Array(rate);
  tail.set(bytes.subarray(full));
  tail[bytes.length - full] ^= suffix;
  tail[rate - 1] ^= 0x80;
  __intrinsic_keccak_blocks(state, tail, 0, 1, rate);
  const out = new Uint8Array(length);
  for (let at = 0; ;) {
    const n = Math.min(rate, length - at);
    out.set(state.subarray(0, n), at);
    at += n;
    if (at >= length) break;
    __intrinsic_keccak_blocks(state, tail, 0, 0, 0);
  }
  return out;
}

// name → [digest(bytes, length), block size, output size, extendable output]; the digests the helper adds are in `HELPER_HASHES`.
const HASHES = {
  md5: [md5, 64, 16],
  sha1: [sha1, 64, 20],
  sha224: [(b) => sha256(b, true), 64, 28],
  sha256: [(b) => sha256(b, false), 64, 32],
  sha384: [(b) => sha512(b, true), 128, 48],
  sha512: [(b) => sha512(b, false), 128, 64],
  'sha512-224': [(b) => sha512t(b, 224), 128, 28],
  'sha512-256': [(b) => sha512t(b, 256), 128, 32],
  'sha3-224': [(b) => keccak(b, 144, 0x06, 28), 144, 28],
  'sha3-256': [(b) => keccak(b, 136, 0x06, 32), 136, 32],
  'sha3-384': [(b) => keccak(b, 104, 0x06, 48), 104, 48],
  'sha3-512': [(b) => keccak(b, 72, 0x06, 64), 72, 64],
  shake128: [(b, length) => keccak(b, 168, 0x1f, length === undefined ? 16 : length), 168, 16, true],
  shake256: [(b, length) => keccak(b, 136, 0x1f, length === undefined ? 32 : length), 136, 32, true],
  'md5-sha1': [(b) => concat([md5(b), sha1(b)]), 64, 36],
};
const HELPER_HASHES = ['ripemd160', 'blake2b512', 'blake2s256', 'sm3'];

function hashFunction(algorithm) {
  if (typeof algorithm !== 'string') throw $$errInvalidArgType('algorithm', 'of type string', algorithm);
  const name = canonicalHash(algorithm);
  if (name !== undefined) {
    if (Object.prototype.hasOwnProperty.call(HASHES, name)) return HASHES[name];
    if (ext !== undefined) {
      const [size, block] = HASH_INFO[name];
      return [(bytes) => ext.digest(name, bytes, size), block, size, false];
    }
  }
  throw new Error('Digest method not supported');
}

function finalized() {
  return codeError(Error, 'ERR_CRYPTO_HASH_FINALIZED', 'Digest already called');
}

// The output length of a hash: `outputLength` is for extendable-output functions only (node: same size is fine).
function hashLength(entry, options, current) {
  const length = options !== null && typeof options === 'object' ? options.outputLength : undefined;
  if (length === undefined) return current;
  if (typeof length !== 'number') throw $$errInvalidArgType('options.outputLength', 'of type number', length);
  if (!Number.isInteger(length)) throw $$errOutOfRange('options.outputLength', 'an integer', length);
  if (length < 0 || length > 4294967295) throw $$errOutOfRange('options.outputLength', '>= 0 && <= 4294967295', length);
  if (entry[3] !== true && length !== entry[2]) {
    const e = new Error('error:030000B2:digital envelope routines::not XOF or invalid length');
    e.code = 'ERR_OSSL_EVP_NOT_XOF_OR_INVALID_LENGTH';
    e.library = 'digital envelope routines';
    e.reason = 'not XOF or invalid length';
    throw e;
  }
  return length;
}

class Hash {
  #entry;
  #length;
  #chunks = [];
  #done = false;
  #result;

  constructor(algorithm, options) {
    if (streams.LazyTransform !== undefined) this._options = options;
    this.#entry = hashFunction(algorithm);
    this.#length = hashLength(this.#entry, options, this.#entry[2]);
  }

  // The stream side (T153): the digest of everything written, also when `digest()` was called before the end.
  _transform(chunk, encoding, callback) {
    try {
      this.update(chunk, encoding);
    } catch (e) {
      callback(codeError(Error, 'ERR_CRYPTO_HASH_UPDATE_FAILED', 'Hash update failed'));
      return;
    }
    callback();
  }

  _flush(callback) {
    this.push(output(this.#compute()));
    callback();
  }

  update(data, encoding) {
    if (this.#done) throw finalized();
    // after the stream ended (`_flush`): OpenSSL's context is finished
    if (this.#result !== undefined) throw codeError(Error, 'ERR_CRYPTO_HASH_UPDATE_FAILED', 'Hash update failed');
    this.#chunks.push(copyBytes(bytesOf(data, encoding, 'data', DATA_TYPES)));
    return this;
  }

  #compute() {
    if (this.#result === undefined) this.#result = this.#entry[0](concat(this.#chunks), this.#length);
    return this.#result;
  }

  digest(encoding) {
    if (this.#done) throw finalized();
    this.#done = true;
    return output(this.#compute(), encoding);
  }

  copy(options) {
    if (this.#done) throw finalized();
    const h = new Hash('md5');
    h.#entry = this.#entry;
    h.#length = hashLength(this.#entry, options, this.#entry[2]);
    h.#chunks = this.#chunks.slice();
    return h;
  }
}

class Hmac {
  #fn;
  #size;
  #inner;
  #outer;
  #chunks = [];
  #done = false;
  #result;

  constructor(algorithm, key, options) {
    if (streams.LazyTransform !== undefined) this._options = options;
    let spec;
    try {
      spec = hashFunction(algorithm);
    } catch (e) {
      if (typeof algorithm === 'string') throw codeError(TypeError, 'ERR_CRYPTO_INVALID_DIGEST', `Invalid digest: ${algorithm}`);
      throw e;
    }
    const [fn, block, size, xof] = spec;
    if (xof) throw new Error('error:00000000:lib(0)::reason(0)');
    if (key === undefined || key === null || (typeof key !== 'string' && !ArrayBuffer.isView(key) && !(key instanceof ArrayBuffer))) {
      throw $$errInvalidArgType('key', 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, DataView, KeyObject, or CryptoKey', key);
    }
    let k = bytesOf(key, undefined, 'key', 'ArrayBuffer,');
    if (k.length > block) k = fn(k, size);
    const ipad = new Uint8Array(block), opad = new Uint8Array(block);
    for (let i = 0; i < block; i++) {
      const b = i < k.length ? k[i] : 0;
      ipad[i] = b ^ 0x36;
      opad[i] = b ^ 0x5c;
    }
    this.#fn = fn;
    this.#size = size;
    this.#inner = ipad;
    this.#outer = opad;
  }

  update(data, encoding) {
    if (this.#done) throw codeError(Error, 'ERR_CRYPTO_HASH_FINALIZED', 'Digest already called');
    this.#chunks.push(copyBytes(bytesOf(data, encoding, 'data', DATA_TYPES)));
    return this;
  }

  digest(encoding) {
    if (this.#done) return output(new Uint8Array(0), encoding);
    return output(this.#finish(), encoding);
  }

  #finish() {
    this.#done = true;
    const inner = this.#fn(concat([this.#inner, ...this.#chunks]), this.#size);
    this.#result = this.#fn(concat([this.#outer, inner]), this.#size);
    return this.#result;
  }

  // The stream side (T153).
  _transform(chunk, encoding, callback) {
    try {
      this.update(chunk, encoding);
    } catch (e) {
      callback(codeError(Error, 'ERR_CRYPTO_HASH_UPDATE_FAILED', 'Hash update failed'));
      return;
    }
    callback();
  }

  _flush(callback) {
    this.push(output(this.#done ? this.#result : this.#finish()));
    callback();
  }
}

function createHash(algorithm, options) {
  return new Hash(algorithm, options);
}

function createHmac(algorithm, key, options) {
  return new Hmac(algorithm, key, options);
}

function hash(algorithm, data, options = 'hex') {
  if (typeof data !== 'string' && !ArrayBuffer.isView(data)) {
    throw $$errInvalidArgType('data', 'of type string or an instance of Buffer, TypedArray, or DataView', data);
  }
  const object = options !== null && typeof options === 'object';
  const outputEncoding = object ? (options.outputEncoding === undefined ? 'hex' : options.outputEncoding) : options;
  return new Hash(algorithm, object ? options : undefined).update(data).digest(outputEncoding);
}

function getHashes() {
  return Object.keys(HASHES).concat(ext === undefined ? [] : HELPER_HASHES);
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
function expandKey(key, decrypt = true) {
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
  if (!decrypt) return { rounds, ek, dk: undefined };
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

// --- ChaCha20 and Poly1305, natively (T152) ---

// The sixteen words of ChaCha20's state (little-endian bytes: the constants "expand 32-byte k", the key, the block counter, the nonce), then room for
// the keystream block and its used count (64 = none left).
const CHACHA_SIGMA = Uint8Array.of(0x65, 0x78, 0x70, 0x61, 0x6e, 0x64, 0x20, 0x33, 0x32, 0x2d, 0x62, 0x79, 0x74, 0x65, 0x20, 0x6b);
function chachaState(key, counter, nonce) {
  const st = new Uint8Array(129);
  st.set(CHACHA_SIGMA);
  st.set(key, 16);
  st[48] = counter & 0xff;
  st[49] = (counter >>> 8) & 0xff;
  st[50] = (counter >>> 16) & 0xff;
  st[51] = (counter >>> 24) & 0xff;
  st.set(nonce, 52);
  st[128] = 64;
  return st;
}

// Feeds a 16-byte-block authenticator (GHASH or Poly1305) byte by byte.
class BlockMac {
  #poly;
  #state;
  #aux;
  #block = new Uint8Array(16);
  #filled = 0;

  // GHASH: `state` the 16-byte accumulator, `aux` the table; Poly1305: `state` the `i32s`.
  constructor(poly, state, aux) {
    this.#poly = poly;
    this.#state = state;
    this.#aux = aux;
  }

  #run(bytes, offset, blocks) {
    if (this.#poly) __intrinsic_poly1305_blocks(this.#state, bytes, offset, blocks);
    else __intrinsic_ghash_blocks(this.#aux, this.#state, bytes, offset, blocks);
  }

  feed(bytes) {
    const n = bytes.length;
    let i = 0;
    if (this.#filled > 0) {
      const take = Math.min(16 - this.#filled, n);
      this.#block.set(bytes.subarray(0, take), this.#filled);
      this.#filled += take;
      i = take;
      if (this.#filled < 16) return;
      this.#run(this.#block, 0, 1);
      this.#filled = 0;
    }
    const whole = (n - i) - ((n - i) % 16);
    if (whole > 0) {
      this.#run(bytes, i, whole / 16);
      i += whole;
    }
    if (i < n) {
      this.#block.set(bytes.subarray(i), 0);
      this.#filled = n - i;
    }
  }

  // Zero-pad the open block.
  pad() {
    if (this.#filled > 0) {
      this.#block.fill(0, this.#filled);
      this.#run(this.#block, 0, 1);
      this.#filled = 0;
    }
  }

  get state() {
    return this.#state;
  }
}

class Poly1305 extends BlockMac {
  #s;

  // `key`: 32 bytes, r (clamped below) and s.
  constructor(key) {
    const t0 = key[0] | (key[1] << 8) | (key[2] << 16) | (key[3] << 24);
    const t1 = key[4] | (key[5] << 8) | (key[6] << 16) | (key[7] << 24);
    const t2 = key[8] | (key[9] << 8) | (key[10] << 16) | (key[11] << 24);
    const t3 = key[12] | (key[13] << 8) | (key[14] << 16) | (key[15] << 24);
    const st = __intrinsic_i32_new(14);
    const r1 = ((t0 >>> 26) | (t1 << 6)) & 0x3ffff03;
    const r2 = ((t1 >>> 20) | (t2 << 12)) & 0x3ffc0ff;
    const r3 = ((t2 >>> 14) | (t3 << 18)) & 0x3f03fff;
    const r4 = (t3 >>> 8) & 0x00fffff;
    __intrinsic_i32_set(st, 0, t0 & 0x3ffffff);
    __intrinsic_i32_set(st, 1, r1);
    __intrinsic_i32_set(st, 2, r2);
    __intrinsic_i32_set(st, 3, r3);
    __intrinsic_i32_set(st, 4, r4);
    __intrinsic_i32_set(st, 5, r1 * 5);
    __intrinsic_i32_set(st, 6, r2 * 5);
    __intrinsic_i32_set(st, 7, r3 * 5);
    __intrinsic_i32_set(st, 8, r4 * 5);
    super(true, st, undefined);
    this.#s = key.slice(16, 32);
  }

  // The 16-byte tag: (h mod 2^130 - 5) + s mod 2^128, little-endian (poly1305-donna's finish).
  tag() {
    const st = this.state;
    const mask = 0x3ffffff;
    let h0 = __intrinsic_i32_get(st, 9);
    let h1 = __intrinsic_i32_get(st, 10);
    let h2 = __intrinsic_i32_get(st, 11);
    let h3 = __intrinsic_i32_get(st, 12);
    let h4 = __intrinsic_i32_get(st, 13);
    let c = h1 >>> 26; h1 &= mask;
    h2 += c; c = h2 >>> 26; h2 &= mask;
    h3 += c; c = h3 >>> 26; h3 &= mask;
    h4 += c; c = h4 >>> 26; h4 &= mask;
    h0 += c * 5; c = h0 >>> 26; h0 &= mask;
    h1 += c;
    // g = h + 5 - 2^130; h is kept if that is negative
    let g0 = h0 + 5; c = g0 >>> 26; g0 &= mask;
    let g1 = h1 + c; c = g1 >>> 26; g1 &= mask;
    let g2 = h2 + c; c = g2 >>> 26; g2 &= mask;
    let g3 = h3 + c; c = g3 >>> 26; g3 &= mask;
    const g4 = (h4 + c - (1 << 26)) | 0;
    const choose = (g4 >>> 31) - 1; // all ones when g4 >= 0
    const keep = ~choose;
    h0 = (h0 & keep) | (g0 & choose);
    h1 = (h1 & keep) | (g1 & choose);
    h2 = (h2 & keep) | (g2 & choose);
    h3 = (h3 & keep) | (g3 & choose);
    h4 = (h4 & keep) | (g4 & mask & choose);
    // h mod 2^128 as four 32-bit words, plus s
    const w = [
      (h0 | (h1 << 26)) >>> 0,
      ((h1 >>> 6) | (h2 << 20)) >>> 0,
      ((h2 >>> 12) | (h3 << 14)) >>> 0,
      ((h3 >>> 18) | (h4 << 8)) >>> 0,
    ];
    const out = new Uint8Array(16);
    let carry = 0;
    for (let i = 0; i < 4; i++) {
      const sw = (this.#s[4 * i] | (this.#s[4 * i + 1] << 8) | (this.#s[4 * i + 2] << 16) | (this.#s[4 * i + 3] << 24)) >>> 0;
      const f = w[i] + sw + carry;
      carry = f >= 4294967296 ? 1 : 0;
      const v = f >>> 0;
      out[4 * i] = v & 0xff;
      out[4 * i + 1] = (v >>> 8) & 0xff;
      out[4 * i + 2] = (v >>> 16) & 0xff;
      out[4 * i + 3] = (v >>> 24) & 0xff;
    }
    return out;
  }
}

// The cipher interface (`aad`, `update`, `setTag`, `finish`) for `chacha20-poly1305` (RFC 8439).
class ChaChaPoly {
  #st;
  #mac;
  #encrypt;
  #tagLength;
  #tag;
  #aadLength = 0;
  #dataLength = 0;
  #inAad = true;

  constructor(key, nonce, encrypt, tagLength) {
    this.#encrypt = encrypt;
    this.#tagLength = tagLength;
    const polyKey = new Uint8Array(32);
    __intrinsic_chacha20_xor(chachaState(key, 0, nonce), new Uint8Array(32), 0, polyKey, 0, 32);
    this.#mac = new Poly1305(polyKey);
    this.#st = chachaState(key, 1, nonce);
  }

  aad(data) {
    this.#aadLength += data.length;
    this.#mac.feed(data);
  }

  update(data) {
    if (this.#inAad) {
      this.#inAad = false;
      this.#mac.pad();
    }
    const out = new Uint8Array(data.length);
    __intrinsic_chacha20_xor(this.#st, data, 0, out, 0, data.length);
    this.#mac.feed(this.#encrypt ? out : data);
    this.#dataLength += data.length;
    return out;
  }

  setTag(tag) {
    this.#tag = tag;
  }

  finish() {
    this.#mac.pad();
    const lengths = new Uint8Array(16);
    for (const [at, n] of [[0, this.#aadLength], [8, this.#dataLength]]) {
      const lo = n >>> 0;
      const hi = Math.floor(n / 4294967296);
      for (let i = 0; i < 4; i++) {
        lengths[at + i] = (lo >>> (8 * i)) & 0xff;
        lengths[at + 4 + i] = (hi >>> (8 * i)) & 0xff;
      }
    }
    this.#mac.feed(lengths);
    const full = this.#mac.tag();
    if (this.#encrypt) return full.slice(0, this.#tagLength);
    const tag = this.#tag;
    let diff = tag === undefined || tag.length === 0 ? 1 : 0;
    for (let i = 0; i < 16 && tag !== undefined && i < tag.length; i++) diff |= full[i] ^ tag[i];
    if (diff !== 0) throw new Error('unable to authenticate data');
    return new Uint8Array(0);
  }
}

// `chacha20`: the stream alone; the 16-byte IV is the block counter (little-endian) and the 12-byte nonce.
class ChaCha20Stream {
  #st;

  constructor(key, iv) {
    this.#st = chachaState(key, iv[0] | (iv[1] << 8) | (iv[2] << 16) | (iv[3] << 24), iv.subarray(4));
  }

  update(data) {
    const out = new Uint8Array(data.length);
    __intrinsic_chacha20_xor(this.#st, data, 0, out, 0, data.length);
    return out;
  }

  finish() {
    return new Uint8Array(0);
  }
}

// --- AES-GCM, natively (T152): `aes_ctr` makes the keystream, `ghash_blocks` the authenticator ---

function i32sOf(words) {
  const a = __intrinsic_i32_new(words.length);
  for (let i = 0; i < words.length; i++) __intrinsic_i32_set(a, i, words[i]);
  return a;
}

class Ghash extends BlockMac {
  constructor(table) {
    super(false, new Uint8Array(16), table);
  }
}

// The cipher of the helper's interface (`aad`, `update`, `setTag`, `finish`) for `aes-<bits>-gcm`.
class Gcm {
  #rk;
  #rounds;
  #counter = new Uint8Array(33);
  #mask = new Uint8Array(16);
  #ghash;
  #encrypt;
  #tag;
  #aadLength = 0;
  #dataLength = 0;
  #inAad = true;

  #tagLength;

  constructor(key, iv, encrypt, tagLength) {
    this.#tagLength = tagLength;
    const k = expandKey(key, false);
    this.#rk = i32sOf(k.ek);
    this.#rounds = k.rounds;
    this.#encrypt = encrypt;
    const zero = new Uint8Array(16);
    // H = E_K(0)
    const first = new Uint8Array(33);
    first[32] = 16;
    const h = new Uint8Array(16);
    __intrinsic_aes_ctr(this.#rk, this.#rounds, first, zero, 0, h, 0, 16);
    const table = __intrinsic_i32_new(64);
    __intrinsic_ghash_init(h, table);
    this.#ghash = new Ghash(table);
    // J0: the IV and 0^31 1, or GHASH of the IV.
    const j0 = new Uint8Array(33);
    if (iv.length === 12) {
      j0.set(iv);
      j0[15] = 1;
    } else {
      const g = new Ghash(table);
      g.feed(iv);
      g.pad();
      const lengths = new Uint8Array(16);
      const bits = iv.length * 8;
      for (let i = 0; i < 4; i++) lengths[15 - i] = (bits >>> (8 * i)) & 0xff;
      lengths[11] = Math.floor(bits / 4294967296) & 0xff;
      g.feed(lengths);
      j0.set(g.state);
    }
    j0[32] = 16;
    // The tag mask E_K(J0), then the counter continues at inc32(J0).
    __intrinsic_aes_ctr(this.#rk, this.#rounds, j0, zero, 0, this.#mask, 0, 16);
    // `j0` advanced by one block (inc32); its keystream byte count is 16 again after a full block.
    this.#counter.set(j0);
    this.#counter[32] = 16;
  }

  aad(data) {
    this.#aadLength += data.length;
    this.#ghash.feed(data);
  }

  update(data) {
    if (this.#inAad) {
      this.#inAad = false;
      this.#ghash.pad();
    }
    const out = new Uint8Array(data.length);
    __intrinsic_aes_ctr(this.#rk, this.#rounds, this.#counter, data, 0, out, 0, data.length);
    this.#ghash.feed(this.#encrypt ? out : data);
    this.#dataLength += data.length;
    return out;
  }

  setTag(tag) {
    this.#tag = tag;
  }

  finish() {
    if (this.#inAad) this.#ghash.pad();
    this.#ghash.pad();
    const lengths = new Uint8Array(16);
    for (const [at, n] of [[0, this.#aadLength * 8], [8, this.#dataLength * 8]]) {
      const hi = Math.floor(n / 4294967296);
      const lo = n >>> 0;
      for (let i = 0; i < 4; i++) {
        lengths[at + 3 - i] = (hi >>> (8 * i)) & 0xff;
        lengths[at + 7 - i] = (lo >>> (8 * i)) & 0xff;
      }
    }
    this.#ghash.feed(lengths);
    const full = this.#ghash.state.slice();
    for (let i = 0; i < 16; i++) full[i] ^= this.#mask[i];
    if (this.#encrypt) return full.slice(0, this.#tagLength);
    const tag = this.#tag;
    let diff = tag === undefined || tag.length === 0 ? 1 : 0;
    for (let i = 0; i < 16 && tag !== undefined && i < tag.length; i++) diff |= full[i] ^ tag[i];
    if (diff !== 0) throw new Error('unable to authenticate data');
    return new Uint8Array(0);
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

// The ciphers of the helper (T151): `aes-<bits>-gcm`, `chacha20-poly1305`, `chacha20` → [key length, mode, helper name].
function helperCipherInfo(name) {
  const l = name.toLowerCase();
  const m = /^(?:id-aes(128|192|256)|aes-(128|192|256))-gcm$/.exec(l);
  if (m !== null) {
    const bits = m[1] === undefined ? m[2] : m[1];
    return [Number(bits) / 8, 'gcm', `aes-${bits}-gcm`];
  }
  if (l === 'chacha20-poly1305') return [32, 'chacha', l];
  if (l === 'chacha20') return [32, 'chacha20', l];
  return undefined;
}

function getCiphers() {
  const out = [];
  for (const bits of [128, 192, 256]) {
    for (const mode of ['cbc', 'ccm', 'ctr', 'ecb']) out.push(`aes-${bits}-${mode}`);
    out.push(`aes-${bits}-gcm`);
  }
  out.push('chacha20', 'chacha20-poly1305');
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
  // GCM and ChaCha20 (T151): the helper's cipher and what the state machine needs.
  #h;
  #tagOption;
  #started = false;

  constructor(decrypt, cipher, key, iv, options) {
    if (streams.LazyTransform !== undefined) this._options = options;
    this.#decrypt = decrypt;
    if (typeof cipher !== 'string') throw $$errInvalidArgType('cipher', 'of type string', cipher);
    const helper = cipherInfo(cipher) === undefined ? helperCipherInfo(cipher) : undefined;
    const info = helper === undefined ? cipherInfo(cipher) : helper;
    if (info === undefined) throw codeError(Error, 'ERR_CRYPTO_UNKNOWN_CIPHER', 'Unknown cipher');
    const [keyLength, mode] = info;
    const keyBytes = bytesOf(key, undefined, 'key', 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, DataView, KeyObject, or CryptoKey');
    const ivBytes = iv === null ? null : bytesOf(iv, undefined, 'iv', 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, or DataView');
    const ivOk = mode === 'ecb' ? ivBytes === null || ivBytes.length === 0
      : mode === 'gcm' ? ivBytes !== null && ivBytes.length > 0
        : mode === 'chacha' ? ivBytes !== null && ivBytes.length === 12
          : mode === 'chacha20' ? ivBytes !== null && ivBytes.length === 16
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
    if (helper !== undefined) {
      if (mode !== 'chacha20' && options !== null && typeof options === 'object' && options.authTagLength !== undefined) {
        const t = options.authTagLength;
        if (!this.#tagLengthOk(t, true)) {
          throw codeError(TypeError, 'ERR_CRYPTO_INVALID_AUTH_TAG', `Invalid authentication tag length: ${t}`);
        }
        this.#tagOption = t;
      }
      const k = copyBytes(keyBytes);
      const v = copyBytes(ivBytes);
      const tagLength = this.#tagOption === undefined ? 16 : this.#tagOption;
      this.#h = mode === 'gcm' ? new Gcm(k, v, !decrypt, tagLength)
        : mode === 'chacha' ? new ChaChaPoly(k, v, !decrypt, tagLength)
          : new ChaCha20Stream(k, v);
      return;
    }
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

  // Is `t` a tag length of this cipher? `option`: as `authTagLength`; else as the tag given to `setAuthTag`.
  #tagLengthOk(t, option) {
    if (typeof t !== 'number') return false;
    const gcm = this.#mode === 'gcm';
    const gcmLength = t === 4 || t === 8 || (t >= 12 && t <= 16);
    if (option) return gcm ? gcmLength : t >= 1 && t <= 16;
    if (this.#tagOption !== undefined) return t === this.#tagOption;
    return gcm ? gcmLength : t === 16;
  }

  update(data, inputEncoding, outputEncoding) {
    if (this.#done) throw new Error('Trying to add data in unsupported state');
    const input = bytesOf(data, inputEncoding, 'data', DATA_TYPES);
    if (this.#h !== undefined) {
      this.#started = true;
      return this.#output(this.#h.update(input), outputEncoding);
    }
    let out;
    switch (this.#mode) {
      case 'ctr': out = this.#ctr(input); break;
      case 'ccm': out = this.#ccm(input); break;
      default: out = this.#blocks(input, false);
    }
    return this.#output(out, outputEncoding);
  }

  final(outputEncoding) {
    if (this.#done) throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state');
    this.#done = true;
    if (this.#h !== undefined) {
      let tag;
      try {
        tag = this.#h.finish();
      } catch (e) {
        // OpenSSL's ChaCha20-Poly1305 finishes a decryption that was given no tag.
        if (this.#mode !== 'chacha' || this.#tag !== undefined) throw new Error('Unsupported state or unable to authenticate data');
      }
      if (!this.#decrypt && this.#mode !== 'chacha20') this.#tag = tag;
      return this.#output(new Uint8Array(0), outputEncoding);
    }
    let out = new Uint8Array(0);
    if (this.#mode === 'ccm') {
      if (this.#decrypt && !this.#ccmOk) throw new Error('Unsupported state or unable to authenticate data');
      if (!this.#decrypt && this.#tag === undefined) this.#ccm(new Uint8Array(0));
    } else if (this.#mode !== 'ctr') {
      out = this.#blocks(new Uint8Array(0), true);
    }
    return this.#output(out, outputEncoding);
  }

  // The stream side (T153).
  _transform(chunk, encoding, callback) {
    this.push(this.update(chunk, encoding));
    callback();
  }

  _flush(callback) {
    this.push(this.final());
    callback();
  }

  setAutoPadding(autoPadding = true) {
    this.#autoPadding = !!autoPadding;
    return this;
  }

  setAAD(buffer, options) {
    if (this.#h !== undefined) {
      if (this.#started || this.#done || this.#mode === 'chacha20') {
        throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state for operation setAAD');
      }
      this.#h.aad(bytesOf(buffer, undefined, 'buffer', 'an instance of ArrayBuffer, Buffer, TypedArray, or DataView'));
      return this;
    }
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
    if (this.#h !== undefined) {
      if (this.#tag !== undefined || this.#mode === 'chacha20') {
        throw codeError(Error, 'ERR_CRYPTO_INVALID_STATE', 'Invalid state for operation setAuthTag');
      }
      if (!this.#tagLengthOk(t.length, false)) {
        throw codeError(TypeError, 'ERR_CRYPTO_INVALID_AUTH_TAG', `Invalid authentication tag length: ${t.length}`);
      }
      this.#tag = Uint8Array.from(t);
      this.#h.setTag(this.#tag);
      return this;
    }
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

// --- Information (T153) ---

// [name, OpenSSL's name, nid, mode, block size, iv length, key length] of the ciphers `createCipheriv` knows.
const CIPHER_INFO = [];
for (const [bits, ecb, cbc, ctr, ccm, gcm] of [[128, 418, 419, 904, 896, 895], [192, 422, 423, 905, 899, 898], [256, 426, 427, 906, 902, 901]]) {
  const key = bits / 8;
  CIPHER_INFO.push(
    [`aes-${bits}-ecb`, `aes-${bits}-ecb`, ecb, 'ecb', 16, 0, key],
    [`aes-${bits}-cbc`, `aes-${bits}-cbc`, cbc, 'cbc', 16, 16, key],
    [`aes-${bits}-ctr`, `aes-${bits}-ctr`, ctr, 'ctr', 1, 16, key],
    [`aes-${bits}-ccm`, `id-aes${bits}-ccm`, ccm, 'ccm', 1, 12, key],
    [`aes-${bits}-gcm`, `id-aes${bits}-gcm`, gcm, 'gcm', 1, 12, key],
  );
}
CIPHER_INFO.push(['chacha20-poly1305', 'chacha20-poly1305', 1018, 'stream', 0, 12, 32], ['chacha20', 'chacha20', 1019, 'stream', 0, 16, 32]);

function validateInt32Option(options, name) {
  const v = options[name];
  if (v === undefined) return undefined;
  if (typeof v !== 'number') throw $$errInvalidArgType(`options.${name}`, 'of type number', v);
  if (!Number.isInteger(v)) throw $$errOutOfRange(`options.${name}`, 'an integer', v);
  if (v < -2147483648 || v > 2147483647) throw $$errOutOfRange(`options.${name}`, '>= -2147483648 && <= 2147483647', v);
  return v;
}

function getCipherInfo(nameOrNid, options) {
  if (typeof nameOrNid !== 'string' && typeof nameOrNid !== 'number') {
    throw $$errInvalidArgType('nameOrNid', 'one of type string or number', nameOrNid);
  }
  if (typeof nameOrNid === 'number' && !Number.isInteger(nameOrNid)) throw $$errOutOfRange('nameOrNid', 'an integer', nameOrNid);
  let keyLength, ivLength;
  if (options !== undefined) {
    if (options === null || typeof options !== 'object') throw $$errInvalidArgType('options', 'of type object', options);
    keyLength = validateInt32Option(options, 'keyLength');
    ivLength = validateInt32Option(options, 'ivLength');
  }
  let entry;
  if (typeof nameOrNid === 'number') {
    entry = CIPHER_INFO.find((e) => e[2] === nameOrNid);
  } else {
    const n = nameOrNid.toLowerCase();
    const short = /^aes(128|192|256)$/.exec(n);
    const lookup = short === null ? n : `aes-${short[1]}-cbc`;
    entry = CIPHER_INFO.find((e) => e[0] === lookup || e[1] === lookup);
  }
  if (entry === undefined) return undefined;
  const [, name, nid, mode, blockSize, defaultIv, defaultKey] = entry;
  if (keyLength !== undefined && keyLength !== defaultKey) return undefined;
  let iv = defaultIv;
  if (ivLength !== undefined) {
    if (mode === 'gcm') iv = ivLength;
    else if (mode === 'ccm') {
      if (ivLength < 7 || ivLength > 13) return undefined;
      iv = ivLength;
    } else if (ivLength !== defaultIv) return undefined;
  }
  const info = { mode, name, nid };
  if (blockSize !== 0) info.blockSize = blockSize;
  if (iv > 0) info.ivLength = iv;
  info.keyLength = defaultKey;
  return info;
}

// No curves yet: `createECDH`, signing and key pairs are missing (F493), so none is claimed.
function getCurves() {
  return [];
}

// FIPS mode doesn't exist here (node's own default is 0 too).
function getFips() {
  return 0;
}

function setFips(enable) {
  if (enable) throw new Error('error:12800067:DSO support routines::could not load the shared library');
}

function secureHeapUsed() {
  return { total: 0, used: 0, utilization: NaN, min: 2 };
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

if (streams.LazyTransform !== undefined) {
  for (const C of [Hash, Hmac, CipherBase]) Object.setPrototypeOf(C.prototype, streams.LazyTransform.prototype);
}

module.exports = {
  Hash, Hmac, Cipheriv, Decipheriv,
  createHash, createHmac, hash, getHashes,
  createCipheriv, createDecipheriv, getCiphers,
  randomBytes, pseudoRandomBytes: randomBytes, prng: randomBytes, rng: randomBytes,
  randomFill, randomFillSync, randomInt, randomUUID, randomUUIDv7, getRandomValues,
  getCipherInfo, getCurves, getFips, setFips, secureHeapUsed,
  timingSafeEqual, webcrypto, Crypto,
  ...kdfExports,
  constants: {},
};

Object.defineProperty(module.exports, 'fips', { get: getFips, set: setFips, enumerable: true, configurable: true });
