// The digests added by the Rust helper (T151): sha3, shake, ripemd160, blake2, sha512-224/256, md5-sha1, sm3, their names and
// aliases, HMAC, `hash`, `Hash.copy`, `outputLength`, pbkdf2/hkdf over them; the old digests answer the same through the helper.
import crypto from 'node:crypto';

function show(label, f) {
  try {
    const r = f();
    console.log(label, Buffer.isBuffer(r) ? r.toString('hex') : r);
  } catch (e) {
    console.log(label, e.name, e.code, e.message);
  }
}

const names = ['md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512', 'sha512-224', 'sha512-256', 'sha3-224', 'sha3-256', 'sha3-384',
  'sha3-512', 'shake128', 'shake256', 'ripemd160', 'blake2b512', 'blake2s256', 'md5-sha1', 'sm3'];
const inputs = ['', 'abc', 'The quick brown fox jumps over the lazy dog', 'x'.repeat(200), 'é中'];
for (const n of names) {
  for (const i of inputs) show(`${n} ${i.length}`, () => crypto.createHash(n).update(i).digest('hex'));
  show(`${n} chunks`, () => crypto.createHash(n).update('ab').update(Buffer.from('cd')).update(new Uint8Array([101, 102])).digest('base64'));
  show(`${n} hash()`, () => crypto.hash(n, 'abc'));
  show(`${n} hash() buffer`, () => crypto.hash(n, 'abc', 'buffer').length);
  show(`${n} hmac`, () => crypto.createHmac(n, 'key').update('abc').digest('hex'));
  show(`${n} hmac long key`, () => crypto.createHmac(n, 'k'.repeat(300)).update('abc').update('def').digest('hex'));
  show(`${n} hmac empty`, () => crypto.createHmac(n, '').digest('hex'));
  show(`${n} pbkdf2`, () => crypto.pbkdf2Sync('password', 'salt', 5, 70, n).toString('hex'));
  show(`${n} hkdf`, () => Buffer.from(crypto.hkdfSync(n, 'key', 'salt', 'info', 70)).toString('hex'));
  show(`${n} hkdf nosalt`, () => Buffer.from(crypto.hkdfSync(n, 'key', '', 'info', 10)).toString('hex'));
}

// Names: case and OpenSSL aliases.
for (const n of ['SHA3-256', 'Sha3-256', 'rmd160', 'ripemd', 'RIPEMD160', 'RSA-SHA256', 'rsa-sha1', 'RSA-SHA512/256', 'sha256WithRSAEncryption',
  'ssl3-md5', 'ssl3-sha1', 'sha-1', 'sha-256', 'SHA-512', 'sha-512/256', 'sha-512/224', 'sha512-256WithRSAEncryption', 'ripemd160WithRSA',
  'id-rsassa-pkcs1-v1_5-with-sha3-256', 'sm3WithRSAEncryption', 'RSA-SM3', 'sha3_256', 'SHA512/256', 'sha-512-256', 'blake2b', 'nope', '']) {
  show(`alias ${n}`, () => crypto.createHash(n).update('abc').digest('hex'));
}
show('alias type', () => crypto.createHash(1));
show('hmac alias', () => crypto.createHmac('RSA-SHA3-256', 'k').update('abc').digest('hex'));
show('hmac nope', () => crypto.createHmac('nope', 'k'));

// XOF output lengths.
for (const n of ['shake128', 'shake256']) {
  for (const len of [0, 1, 5, 16, 32, 64, 200, 1000]) {
    show(`${n} outputLength ${len}`, () => crypto.createHash(n, { outputLength: len }).update('abc').digest('hex'));
  }
  show(`${n} default`, () => crypto.createHash(n).update('abc').digest('hex'));
  show(`${n} hash() opts`, () => crypto.hash(n, 'abc', { outputEncoding: 'hex' }));
  show(`${n} hash() opts len`, () => crypto.hash(n, 'abc', { outputEncoding: 'hex', outputLength: 7 }));
  show(`${n} hmac`, () => crypto.createHmac(n, 'key'));
  show(`${n} pbkdf2`, () => crypto.pbkdf2Sync('p', 's', 1, 10, n));
  show(`${n} hkdf`, () => crypto.hkdfSync(n, 'k', 's', 'i', 10));
}
show('bad length type', () => crypto.createHash('shake128', { outputLength: '5' }));
show('bad length frac', () => crypto.createHash('shake128', { outputLength: 1.5 }));
show('bad length neg', () => crypto.createHash('shake128', { outputLength: -1 }));
for (const n of ['sha256', 'sha3-256', 'md5', 'blake2b512']) {
  show(`${n} same length`, () => crypto.createHash(n, { outputLength: { sha256: 32, 'sha3-256': 32, md5: 16, blake2b512: 64 }[n] }).update('a').digest('hex'));
  show(`${n} other length`, () => crypto.createHash(n, { outputLength: 5 }));
}

// copy and finalisation.
show('copy', () => {
  const h = crypto.createHash('sha3-256').update('a');
  const g = h.copy();
  h.update('b');
  return [h.digest('hex'), g.update('b').digest('hex'), crypto.createHash('sha3-256').update('ab').digest('hex')].join(' ');
});
show('copy shake length', () => crypto.createHash('shake128', { outputLength: 3 }).update('a').copy({ outputLength: 7 }).digest('hex'));
show('copy shake keeps', () => crypto.createHash('shake256', { outputLength: 3 }).update('a').copy().digest('hex'));
show('copy sha256 bad', () => crypto.createHash('sha256').copy({ outputLength: 5 }));
show('copy after digest', () => {
  const h = crypto.createHash('sha3-256');
  h.digest();
  return h.copy();
});
show('digest twice', () => {
  const h = crypto.createHash('blake2s256');
  h.digest();
  return h.digest();
});
show('update after digest', () => {
  const h = crypto.createHash('ripemd160');
  h.digest();
  return h.update('a');
});
show('update string enc', () => crypto.createHash('sha3-512').update('00ff', 'hex').digest('hex'));
show('digest encodings', () => [crypto.createHash('sm3').update('a').digest('base64url'), crypto.createHash('sm3').update('a').digest('latin1').length].join(' '));
show('getHashes', () => {
  const l = crypto.getHashes();
  return ['md5', 'sha256', 'sha3-256', 'shake128', 'ripemd160', 'blake2b512', 'sm3'].map((n) => l.includes(n)).join(' ');
});
