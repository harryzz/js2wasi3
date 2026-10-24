// The digests' names and sizes, for `crypto.js` and the helper-backed functions (`crypto_kdf.js`) (T151, T152).
'use strict';

// name → [output bytes (shake: the default), block bytes, extendable output]
const HASH_INFO = {
  md5: [16, 64], sha1: [20, 64], sha224: [28, 64], sha256: [32, 64], sha384: [48, 128], sha512: [64, 128],
  'sha512-224': [28, 128], 'sha512-256': [32, 128],
  'sha3-224': [28, 144], 'sha3-256': [32, 136], 'sha3-384': [48, 104], 'sha3-512': [64, 72],
  shake128: [16, 168, true], shake256: [32, 136, true],
  ripemd160: [20, 64], blake2b512: [64, 128], blake2s256: [32, 64], 'md5-sha1': [36, 64], sm3: [32, 64],
};
// lower-cased OpenSSL names → ours
const HASH_ALIASES = {
  "rsa-md5": "md5",
  "rsa-ripemd160": "ripemd160",
  "rsa-sha1": "sha1",
  "rsa-sha1-2": "sha1",
  "rsa-sha224": "sha224",
  "rsa-sha256": "sha256",
  "rsa-sha3-224": "sha3-224",
  "rsa-sha3-256": "sha3-256",
  "rsa-sha3-384": "sha3-384",
  "rsa-sha3-512": "sha3-512",
  "rsa-sha384": "sha384",
  "rsa-sha512": "sha512",
  "rsa-sha512/224": "sha512-224",
  "rsa-sha512/256": "sha512-256",
  "rsa-sm3": "sm3",
  "id-rsassa-pkcs1-v1_5-with-sha3-224": "sha3-224",
  "id-rsassa-pkcs1-v1_5-with-sha3-256": "sha3-256",
  "id-rsassa-pkcs1-v1_5-with-sha3-384": "sha3-384",
  "id-rsassa-pkcs1-v1_5-with-sha3-512": "sha3-512",
  "md5withrsaencryption": "md5",
  "ripemd": "ripemd160",
  "ripemd160withrsa": "ripemd160",
  "rmd160": "ripemd160",
  "sha1withrsaencryption": "sha1",
  "sha224withrsaencryption": "sha224",
  "sha256withrsaencryption": "sha256",
  "sha384withrsaencryption": "sha384",
  "sha512-224withrsaencryption": "sha512-224",
  "sha512-256withrsaencryption": "sha512-256",
  "sha512withrsaencryption": "sha512",
  "sm3withrsaencryption": "sm3",
  "ssl3-md5": "md5",
  "ssl3-sha1": "sha1",
  "sha-1": "sha1",
  "sha-224": "sha224",
  "sha-256": "sha256",
  "sha-384": "sha384",
  "sha-512": "sha512",
  "sha-512/224": "sha512-224",
  "sha-512/256": "sha512-256",
  "ripemd-160": "ripemd160",
  "sha2-256": "sha256"
};

function canonicalHash(name) {
  let n = name.toLowerCase();
  if (Object.prototype.hasOwnProperty.call(HASH_ALIASES, n)) n = HASH_ALIASES[n];
  return Object.prototype.hasOwnProperty.call(HASH_INFO, n) ? n : undefined;
}

module.exports = { HASH_INFO, HASH_ALIASES, canonicalHash };
