// pbkdf2, scrypt, hkdf (T149): RFC vectors, node's argument checks and errors, the callbacks.
import crypto from 'node:crypto';

function show(label, f) {
  try {
    const r = f();
    console.log(label, r instanceof ArrayBuffer ? 'ArrayBuffer ' + Buffer.from(r).toString('hex') : Buffer.isBuffer(r) ? r.toString('hex') : r);
  } catch (e) {
    console.log(label, e.name, e.code, e.message);
  }
}

// RFC 6070 (pbkdf2-hmac-sha1) and the same inputs with the other digests.
show('rfc6070-1', () => crypto.pbkdf2Sync('password', 'salt', 1, 20, 'sha1'));
show('rfc6070-2', () => crypto.pbkdf2Sync('password', 'salt', 2, 20, 'sha1'));
show('rfc6070-3', () => crypto.pbkdf2Sync('password', 'salt', 4096, 20, 'sha1'));
show('rfc6070-5', () => crypto.pbkdf2Sync('passwordPASSWORDpassword', 'saltSALTsaltSALTsaltSALTsaltSALTsalt', 4096, 25, 'sha1'));
show('rfc6070-6', () => crypto.pbkdf2Sync('pass\0word', 'sa\0lt', 4096, 16, 'sha1'));
for (const d of ['md5', 'sha1', 'sha224', 'sha256', 'sha384', 'sha512', 'SHA256', 'sha-512']) {
  show('pbkdf2 ' + d, () => crypto.pbkdf2Sync('secret', 'pepper', 1000, 70, d));
}
show('pbkdf2 buffer', () => crypto.pbkdf2Sync(Buffer.from('pw'), new Uint8Array([1, 2, 3]), 3, 10, 'sha256'));
show('pbkdf2 arraybuffer', () => crypto.pbkdf2Sync(new Uint16Array([1, 2]), new ArrayBuffer(3), 3, 10, 'sha256'));
show('pbkdf2 empty', () => crypto.pbkdf2Sync('', '', 1, 8, 'sha1'));
show('pbkdf2 10^5', () => crypto.pbkdf2Sync('password', 'salt', 100000, 32, 'sha256'));

// Argument checks, in node's order.
show('no digest', () => crypto.pbkdf2Sync('p', 's', 1, 8));
show('digest 5', () => crypto.pbkdf2Sync('p', 's', 1, 8, 5));
show('digest foo', () => crypto.pbkdf2Sync('p', 's', 1, 8, 'foo'));
show('password 5', () => crypto.pbkdf2Sync(5, 's', 1, 8, 'sha1'));
show('salt null', () => crypto.pbkdf2Sync('p', null, 1, 8, 'sha1'));
show('iterations 0', () => crypto.pbkdf2Sync('p', 's', 0, 8, 'sha1'));
show('iterations 1.5', () => crypto.pbkdf2Sync('p', 's', 1.5, 8, 'sha1'));
show('iterations str', () => crypto.pbkdf2Sync('p', 's', '1', 8, 'sha1'));
show('iterations 2^31', () => crypto.pbkdf2Sync('p', 's', 2 ** 31, 8, 'sha1'));
show('iterations NaN', () => crypto.pbkdf2Sync('p', 's', NaN, 8, 'sha1'));
show('keylen -1', () => crypto.pbkdf2Sync('p', 's', 1, -1, 'sha1'));
show('keylen 0', () => crypto.pbkdf2Sync('p', 's', 1, 0, 'sha1'));
show('keylen 2^31', () => crypto.pbkdf2Sync('p', 's', 1, 2 ** 31, 'sha1'));
show('no callback', () => crypto.pbkdf2('p', 's', 1, 8, 'sha1'));
show('callback 5', () => crypto.pbkdf2('p', 's', 1, 8, 'sha1', 5));
show('async digest foo', () => crypto.pbkdf2('p', 's', 1, 8, 'foo', () => {}));
console.log('lengths', crypto.pbkdf2.length, crypto.pbkdf2Sync.length, crypto.scrypt.length, crypto.scryptSync.length, crypto.hkdf.length, crypto.hkdfSync.length);

// RFC 7914 scrypt vectors (N, r, p).
show('scrypt rfc1', () => crypto.scryptSync('', '', 64, { N: 16, r: 1, p: 1 }));
show('scrypt rfc2', () => crypto.scryptSync('password', 'NaCl', 64, { N: 1024, r: 8, p: 16 }));
show('scrypt rfc3', () => crypto.scryptSync('pleaseletmein', 'SodiumChloride', 64, { N: 16384, r: 8, p: 1 }));
show('scrypt defaults', () => crypto.scryptSync('pw', 'salt', 16));
show('scrypt names', () => crypto.scryptSync('pw', 'salt', 16, { cost: 32, blockSize: 4, parallelization: 2 }));
show('scrypt zeros', () => crypto.scryptSync('pw', 'salt', 16, { N: 0, r: 0, p: 0, maxmem: 0 }));
show('scrypt keylen 0', () => crypto.scryptSync('pw', 'salt', 0));
show('scrypt N 3', () => crypto.scryptSync('pw', 'salt', 16, { N: 3 }));
show('scrypt N 1', () => crypto.scryptSync('pw', 'salt', 16, { N: 1 }));
show('scrypt N 2^20', () => crypto.scryptSync('pw', 'salt', 16, { N: 2 ** 20 }));
show('scrypt maxmem', () => crypto.scryptSync('pw', 'salt', 16, { N: 16, r: 1, p: 1, maxmem: 128 * 16 + 128 + 1 }));
show('scrypt maxmem ok', () => crypto.scryptSync('pw', 'salt', 16, { N: 16, r: 1, p: 1, maxmem: 128 * 18 + 128 }));
show('scrypt N+cost', () => crypto.scryptSync('pw', 'salt', 16, { N: 16, cost: 16 }));
show('scrypt r+blockSize', () => crypto.scryptSync('pw', 'salt', 16, { N: 16, r: 1, blockSize: 2 }));
show('scrypt N str', () => crypto.scryptSync('pw', 'salt', 16, { N: '16' }));
show('scrypt N 16.5', () => crypto.scryptSync('pw', 'salt', 16, { N: 16.5 }));
show('scrypt r -1', () => crypto.scryptSync('pw', 'salt', 16, { N: 16, r: -1 }));
show('scrypt maxmem -1', () => crypto.scryptSync('pw', 'salt', 16, { maxmem: -1 }));
show('scrypt keylen str', () => crypto.scryptSync('pw', 'salt', '16'));
show('scrypt options 5', () => crypto.scryptSync('pw', 'salt', 16, 5));
show('scrypt no callback', () => crypto.scrypt('pw', 'salt', 16));
show('scrypt callback 5', () => crypto.scrypt('pw', 'salt', 16, {}, 5));

// RFC 5869 HKDF vectors (SHA-256 test cases 1 and 3, SHA-1 test case 7).
const hex = (h) => Buffer.from(h, 'hex');
show('hkdf rfc1', () => crypto.hkdfSync('sha256', hex('0b'.repeat(22)), hex('000102030405060708090a0b0c'), hex('f0f1f2f3f4f5f6f7f8f9'), 42));
show('hkdf rfc3', () => crypto.hkdfSync('sha256', hex('0b'.repeat(22)), '', '', 42));
show('hkdf rfc7', () => crypto.hkdfSync('sha1', hex('0c'.repeat(22)), '', '', 42));
for (const d of ['md5', 'sha224', 'sha384', 'sha512', 'SHA256']) show('hkdf ' + d, () => crypto.hkdfSync(d, 'key', 'salt', 'info', 33));
show('hkdf max', () => crypto.hkdfSync('sha256', 'k', 's', 'i', 255 * 32).byteLength);
show('hkdf too long', () => crypto.hkdfSync('sha256', 'k', 's', 'i', 255 * 32 + 1));
show('hkdf length 0', () => crypto.hkdfSync('sha256', 'k', 's', 'i', 0));
show('hkdf length -1', () => crypto.hkdfSync('sha256', 'k', 's', 'i', -1));
show('hkdf length str', () => crypto.hkdfSync('sha256', 'k', 's', 'i', '16'));
show('hkdf length 1.5', () => crypto.hkdfSync('sha256', 'k', 's', 'i', 1.5));
show('hkdf digest foo', () => crypto.hkdfSync('foo', 'k', 's', 'i', 16));
show('hkdf digest 5', () => crypto.hkdfSync(5, 'k', 's', 'i', 16));
show('hkdf ikm 5', () => crypto.hkdfSync('sha256', 5, 's', 'i', 16));
show('hkdf salt 5', () => crypto.hkdfSync('sha256', 'k', 5, 'i', 16));
show('hkdf info 5', () => crypto.hkdfSync('sha256', 'k', 's', 5, 16));
show('hkdf info long', () => crypto.hkdfSync('sha256', 'k', 's', 'i'.repeat(1025), 1));
show('hkdf empty ikm', () => crypto.hkdfSync('sha256', '', 's', 'i', 8));
show('hkdf no callback', () => crypto.hkdf('sha256', 'k', 's', 'i', 16));

// Callbacks run later, with a Buffer or ArrayBuffer (one job at a time: node finishes parallel jobs in any order).
console.log('start');
crypto.pbkdf2('password', 'salt', 2, 20, 'sha1', (err, key) => {
  console.log('pbkdf2 cb', err, key.toString('hex'), Buffer.isBuffer(key));
  crypto.scrypt('pw', 'salt', 16, { N: 16 }, (err, key) => {
    console.log('scrypt cb', err, key.toString('hex'));
    crypto.scrypt('pw', 'salt', 16, (err, key) => {
      console.log('scrypt cb2', err, key.toString('hex'));
      crypto.hkdf('sha256', 'k', 's', 'i', 16, (err, key) => {
        console.log('hkdf cb', err, key instanceof ArrayBuffer, Buffer.from(key).toString('hex'));
        crypto.pbkdf2('p', 's', 1, 0, 'sha1', (err, key) => console.log('pbkdf2 keylen 0', err && err.message, key));
      });
    });
  });
});
console.log('end');
