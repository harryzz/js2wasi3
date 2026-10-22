// node's `crypto.pbkdf2`, `scrypt` and `hkdf` with their `Sync` forms (T149), computed by the Rust helper
// component (`node:internal/crypto_helper`). Linked into a program only when it names one of them
// (`link::link`); `crypto.js` spreads this module's exports. Checks and errors are node's.
'use strict';

const { Buffer } = require('buffer');

function codeError(Base, code, message) {
  const e = new Base(message);
  if (code !== undefined) e.code = code;
  return e;
}

// The bytes of a `string | ArrayBufferView | ArrayBuffer` argument.
function bytesOf(data, name, expected) {
  if (typeof data === 'string') return Buffer.from(data, 'utf8');
  if (ArrayBuffer.isView(data)) return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  if (data instanceof ArrayBuffer) return new Uint8Array(data);
  throw $$errInvalidArgType(name, expected, data);
}

const ALIASES = { 'sha-1': 'sha1', 'sha-224': 'sha224', 'sha-256': 'sha256', 'sha-384': 'sha384', 'sha-512': 'sha512' };


const kdf = require('node:internal/crypto_helper');

function validateInt(value, name, min, max) {
  if (typeof value !== 'number') throw $$errInvalidArgType(name, 'of type number', value);
  if (!Number.isInteger(value)) throw $$errOutOfRange(name, 'an integer', value);
  if (value < min || value > max) throw $$errOutOfRange(name, `>= ${min} && <= ${max}`, value);
}

function validateCallback(callback) {
  if (typeof callback !== 'function') throw $$errInvalidArgType('callback', 'of type function', callback);
}

const KDF_BYTES = 'of type string or an instance of ArrayBuffer, Buffer, TypedArray, or DataView';
const HKDF_BYTES = 'of type string or an instance of ArrayBuffer, TypedArray, DataView, or Buffer';
const HASH_BYTES = { md5: 16, sha1: 20, sha224: 28, sha256: 32, sha384: 48, sha512: 64 };

function kdfDigest(digest) {
  let name = digest.toLowerCase();
  if (ALIASES[name] !== undefined) name = ALIASES[name];
  if (!Object.prototype.hasOwnProperty.call(HASH_BYTES, name)) {
    throw codeError(TypeError, 'ERR_CRYPTO_INVALID_DIGEST', `Invalid digest: ${digest}`);
  }
  return name;
}

// The helper's error text as node's error.
function derive(f) {
  try {
    return f();
  } catch (e) {
    throw new Error('Deriving bits failed');
  }
}

// `done(error, value)` after the current turn, as node's thread pool completes jobs.
function later(callback, f) {
  setImmediate(() => {
    let result;
    try {
      result = f();
    } catch (e) {
      callback(e);
      return;
    }
    callback(null, result);
  });
}

function pbkdf2Params(password, salt, iterations, keylen, digest) {
  if (typeof digest !== 'string') throw $$errInvalidArgType('digest', 'of type string', digest);
  const p = bytesOf(password, 'password', KDF_BYTES);
  const s = bytesOf(salt, 'salt', KDF_BYTES);
  validateInt(iterations, 'iterations', 1, 2147483647);
  validateInt(keylen, 'keylen', 0, 2147483647);
  return [p, s, iterations, keylen, digest];
}

function pbkdf2Run(p, s, iterations, keylen, digest) {
  const name = kdfDigest(digest);
  return () => {
    if (keylen === 0) throw new Error('Deriving bits failed');
    const out = derive(() => kdf.pbkdf2(p, s, iterations, keylen, name));
    return Buffer.from(out.buffer, out.byteOffset, out.byteLength);
  };
}

function pbkdf2Sync(password, salt, iterations, keylen, digest) {
  const [p, s, n, len, d] = pbkdf2Params(password, salt, iterations, keylen, digest);
  return pbkdf2Run(p, s, n, len, d)();
}

function pbkdf2(password, salt, iterations, keylen, digest, callback) {
  if (typeof digest === 'function') {
    callback = digest;
    digest = undefined;
  }
  const [p, s, n, len, d] = pbkdf2Params(password, salt, iterations, keylen, digest);
  validateCallback(callback);
  later(callback, pbkdf2Run(p, s, n, len, d));
}

const SCRYPT_DEFAULTS = { N: 16384, r: 8, p: 1, maxmem: 32 << 20 };

function scryptParams(password, salt, keylen, options) {
  const pw = bytesOf(password, 'password', KDF_BYTES);
  const s = bytesOf(salt, 'salt', KDF_BYTES);
  validateInt(keylen, 'keylen', 0, 2147483647);
  let N = SCRYPT_DEFAULTS.N, r = SCRYPT_DEFAULTS.r, p = SCRYPT_DEFAULTS.p, maxmem = SCRYPT_DEFAULTS.maxmem;
  if (options !== null && typeof options === 'object') {
    const pick = (short, long, current) => {
      let value = current;
      let has = false;
      if (options[short] !== undefined) {
        has = true;
        value = options[short];
        validateInt(value, short, 0, 4294967295);
      }
      if (options[long] !== undefined) {
        if (has) throw codeError(TypeError, 'ERR_INCOMPATIBLE_OPTION_PAIR', `Option "${short}" cannot be used in combination with option "${long}"`);
        value = options[long];
        validateInt(value, long, 0, 4294967295);
      }
      return value === 0 ? current : value;
    };
    N = pick('N', 'cost', N);
    r = pick('r', 'blockSize', r);
    p = pick('p', 'parallelization', p);
    if (options.maxmem !== undefined) {
      validateInt(options.maxmem, 'maxmem', 0, Number.MAX_SAFE_INTEGER);
      if (options.maxmem !== 0) maxmem = options.maxmem;
    }
  }
  if (N < 2 || (N & (N - 1)) !== 0) throw codeError(RangeError, 'ERR_CRYPTO_INVALID_SCRYPT_PARAMS', 'Invalid scrypt params');
  if (128 * r * (p + N + 2) > maxmem) {
    throw codeError(RangeError, 'ERR_CRYPTO_INVALID_SCRYPT_PARAMS',
      'Invalid scrypt params: error:030000AC:digital envelope routines::memory limit exceeded');
  }
  return [pw, s, keylen, N, r, p, maxmem];
}

function scryptRun(pw, s, keylen, N, r, p, maxmem) {
  return () => {
    if (keylen === 0) return Buffer.alloc(0);
    const out = derive(() => kdf.scrypt(pw, s, keylen, N, r, p, maxmem));
    return Buffer.from(out.buffer, out.byteOffset, out.byteLength);
  };
}

function scryptSync(password, salt, keylen, options = SCRYPT_DEFAULTS) {
  return scryptRun(...scryptParams(password, salt, keylen, options))();
}

function scrypt(password, salt, keylen, options, callback = options) {
  if (typeof options === 'function') options = undefined;
  const params = scryptParams(password, salt, keylen, options);
  validateCallback(callback);
  later(callback, scryptRun(...params));
}

function hkdfParams(digest, ikm, salt, info, length) {
  if (typeof digest !== 'string') throw $$errInvalidArgType('digest', 'of type string', digest);
  const k = bytesOf(ikm, 'ikm', 'of type string or an instance of SecretKeyObject, ArrayBuffer, TypedArray, DataView, or Buffer');
  const s = bytesOf(salt, 'salt', HKDF_BYTES);
  const i = bytesOf(info, 'info', HKDF_BYTES);
  validateInt(length, 'length', 0, Number.MAX_SAFE_INTEGER);
  if (i.byteLength > 1024) {
    throw $$errOutOfRange('info', 'must not contain more than 1024 bytes', i.byteLength);
  }
  return [kdfDigest(digest), k, s, i, length];
}

function hkdfRun(name, k, s, i, length) {
  if (length > 255 * HASH_BYTES[name]) throw codeError(RangeError, 'ERR_CRYPTO_INVALID_KEYLEN', 'Invalid key length');
  return () => {
    if (length === 0) throw new Error('Deriving bits failed');
    const out = derive(() => kdf.hkdf(name, k, s, i, length));
    return out.buffer.slice(out.byteOffset, out.byteOffset + out.byteLength);
  };
}

function hkdfSync(digest, ikm, salt, info, length) {
  return hkdfRun(...hkdfParams(digest, ikm, salt, info, length))();
}

function hkdf(digest, ikm, salt, info, length, callback) {
  const params = hkdfParams(digest, ikm, salt, info, length);
  validateCallback(callback);
  later(callback, hkdfRun(...params));
}

module.exports = { pbkdf2, pbkdf2Sync, scrypt, scryptSync, hkdf, hkdfSync };
