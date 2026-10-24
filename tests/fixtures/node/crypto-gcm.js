// AES-GCM, ChaCha20-Poly1305 and ChaCha20 over the Rust helper (T151): fixed keys and IVs, node's state machine and errors.
import crypto from 'node:crypto';

function show(label, f) {
  try {
    const r = f();
    console.log(label, Buffer.isBuffer(r) ? r.toString('hex') : r);
  } catch (e) {
    console.log(label, e.name, e.code, e.message);
  }
}

const k16 = Buffer.from('000102030405060708090a0b0c0d0e0f', 'hex');
const k24 = Buffer.concat([k16, Buffer.from('1011121314151617', 'hex')]);
const k32 = Buffer.concat([k16, Buffer.from('101112131415161718191a1b1c1d1e1f', 'hex')]);
const keys = { 'aes-128-gcm': k16, 'aes-192-gcm': k24, 'aes-256-gcm': k32, 'chacha20-poly1305': k32 };
const iv12 = Buffer.from('a0a1a2a3a4a5a6a7a8a9aaab', 'hex');
const text = 'The quick brown fox jumps over the lazy dog, twice: ' + 'abcdefghij'.repeat(13);

function seal(alg, key, iv, pt, aad, options) {
  const c = crypto.createCipheriv(alg, key, iv, options);
  if (aad !== undefined) c.setAAD(Buffer.from(aad));
  const ct = Buffer.concat([c.update(pt), c.final()]);
  return { ct, tag: c.getAuthTag() };
}

function open(alg, key, iv, ct, tag, aad, options) {
  const d = crypto.createDecipheriv(alg, key, iv, options);
  if (aad !== undefined) d.setAAD(Buffer.from(aad));
  d.setAuthTag(tag);
  return Buffer.concat([d.update(ct), d.final()]).toString();
}

for (const alg of Object.keys(keys)) {
  const key = keys[alg];
  for (const n of [0, 1, 15, 16, 17, 31, 64, 100, text.length]) {
    for (const aad of [undefined, '', 'h', 'header-of-seventeen', 'x'.repeat(40)]) {
      show(`${alg} ${n} ${aad === undefined ? '-' : aad.length}`, () => {
        const { ct, tag } = seal(alg, key, iv12, text.slice(0, n), aad);
        const back = open(alg, key, iv12, ct, tag, aad);
        return `${ct.toString('hex')} ${tag.toString('hex')} ${back === text.slice(0, n)}`;
      });
    }
  }
  show(`${alg} chunked`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.setAAD(Buffer.from('ab'));
    c.setAAD(Buffer.from('cdefghijklmnopqrstuvwxyz'));
    const parts = [];
    for (let i = 0; i < text.length; i += 7) parts.push(c.update(text.slice(i, i + 7)).toString('hex'));
    parts.push(c.final().toString('hex'), c.getAuthTag().toString('hex'));
    return parts.join(' ');
  });
  show(`${alg} update output`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    return [c.update('abc'), c.update(''), c.update(Buffer.alloc(33, 1)), c.update('héllo', 'utf8', 'hex'), c.final('hex')].map((x) => (x.toString('hex'))).join(' ');
  });
  show(`${alg} tag lengths`, () => {
    const r = [];
    for (let n = 0; n <= 18; n++) {
      try {
        crypto.createCipheriv(alg, key, iv12, { authTagLength: n });
        r.push(n);
      } catch (e) {
        // not allowed
      }
    }
    return r.join(',');
  });
  for (const n of [4, 8, 12, 14, 16]) {
    show(`${alg} tag ${n}`, () => {
      const { ct, tag } = seal(alg, key, iv12, 'short message', 'aad', { authTagLength: n });
      return `${ct.toString('hex')} ${tag.toString('hex')} ${open(alg, key, iv12, ct, tag, 'aad', { authTagLength: n })}`;
    });
  }
  show(`${alg} dec tag lengths`, () => {
    const r = [];
    for (let n = 0; n <= 18; n++) {
      try {
        crypto.createDecipheriv(alg, key, iv12).setAuthTag(Buffer.alloc(n));
        r.push(n);
      } catch (e) {
        // not allowed
      }
    }
    return r.join(',');
  });
  show(`${alg} dec tag mismatch option`, () => crypto.createDecipheriv(alg, key, iv12, { authTagLength: 8 }).setAuthTag(Buffer.alloc(16)));
  const { ct, tag } = seal(alg, key, iv12, 'secret message', 'aad');
  show(`${alg} bad tag`, () => open(alg, key, iv12, ct, Buffer.alloc(16), 'aad'));
  show(`${alg} bad aad`, () => open(alg, key, iv12, ct, tag, 'aax'));
  show(`${alg} no aad`, () => open(alg, key, iv12, ct, tag));
  show(`${alg} bad ct`, () => open(alg, key, iv12, Buffer.concat([ct.subarray(0, 3), Buffer.from([0]), ct.subarray(4)]), tag, 'aad'));
  show(`${alg} bad key`, () => open(alg, Buffer.alloc(key.length, 9), iv12, ct, tag, 'aad'));
  show(`${alg} bad iv`, () => open(alg, key, Buffer.alloc(12, 7), ct, tag, 'aad'));
  show(`${alg} no tag`, () => {
    const d = crypto.createDecipheriv(alg, key, iv12);
    d.setAAD(Buffer.from('aad'));
    d.update(ct);
    return d.final();
  });
  show(`${alg} update before final`, () => {
    const d = crypto.createDecipheriv(alg, key, iv12);
    d.setAAD(Buffer.from('aad'));
    d.setAuthTag(tag);
    return d.update(ct).toString();
  });
  show(`${alg} empty message`, () => {
    const { ct: c0, tag: t0 } = seal(alg, key, iv12, '');
    const d = crypto.createDecipheriv(alg, key, iv12);
    d.setAuthTag(t0);
    return `${c0.length} ${t0.toString('hex')} ${d.final().length}`;
  });
  // State machine.
  show(`${alg} getAuthTag early`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.update('a');
    return c.getAuthTag();
  });
  show(`${alg} getAuthTag twice`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.final();
    return c.getAuthTag().equals(c.getAuthTag());
  });
  show(`${alg} setAuthTag twice`, () => {
    const d = crypto.createDecipheriv(alg, key, iv12);
    d.setAuthTag(tag);
    d.setAuthTag(tag);
  });
  show(`${alg} setAuthTag after final`, () => {
    const d = crypto.createDecipheriv(alg, key, iv12);
    try {
      d.final();
    } catch (e) {
      // expected
    }
    d.setAuthTag(tag);
  });
  show(`${alg} setAAD after update`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.update('a');
    c.setAAD(Buffer.from('x'));
  });
  show(`${alg} setAAD after final`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.final();
    c.setAAD(Buffer.from('x'));
  });
  show(`${alg} update after final`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.final();
    c.update('a');
  });
  show(`${alg} final twice`, () => {
    const c = crypto.createCipheriv(alg, key, iv12);
    c.final();
    c.final();
  });
  show(`${alg} update after failed final`, () => {
    const d = crypto.createDecipheriv(alg, key, iv12);
    try {
      d.final();
    } catch (e) {
      // expected
    }
    d.update('a');
  });
  show(`${alg} setAAD type`, () => crypto.createCipheriv(alg, key, iv12).setAAD('str') !== undefined);
  show(`${alg} setAuthTag type`, () => crypto.createDecipheriv(alg, key, iv12).setAuthTag('str') !== undefined);
  show(`${alg} setAutoPadding`, () => crypto.createCipheriv(alg, key, iv12).setAutoPadding(false) !== undefined);
  show(`${alg} bad key length`, () => crypto.createCipheriv(alg, Buffer.alloc(5), iv12));
  show(`${alg} string key`, () => crypto.createCipheriv(alg, key.toString('latin1'), iv12).update('a'));
  show(`${alg} key type`, () => crypto.createCipheriv(alg, 5, iv12));
  show(`${alg} iv type`, () => crypto.createCipheriv(alg, key, 5));
  show(`${alg} null iv`, () => crypto.createCipheriv(alg, key, null));
  show(`${alg} empty iv`, () => crypto.createCipheriv(alg, key, Buffer.alloc(0)));
}

// GCM IV lengths.
for (const n of [1, 5, 8, 11, 12, 13, 16, 17, 60, 100]) {
  show(`gcm iv ${n}`, () => {
    const { ct, tag } = seal('aes-256-gcm', k32, Buffer.alloc(n, n), 'hello gcm', 'aad');
    return `${ct.toString('hex')} ${tag.toString('hex')} ${open('aes-256-gcm', k32, Buffer.alloc(n, n), ct, tag, 'aad')}`;
  });
}
for (const n of [0, 8, 11, 13, 16]) show(`chacha iv ${n}`, () => crypto.createCipheriv('chacha20-poly1305', k32, Buffer.alloc(n)));
show('gcm key 32 for 128', () => crypto.createCipheriv('aes-128-gcm', k32, iv12));
show('chacha key 16', () => crypto.createCipheriv('chacha20-poly1305', k16, iv12));

// Names.
for (const n of ['id-aes128-GCM', 'AES-128-GCM', 'aes128-gcm', 'id-aes256-GCM', 'CHACHA20-POLY1305', 'aes-128-gcm1', 'id-aes192-GCM']) {
  show(`name ${n}`, () => {
    const key = /256|CHACHA/i.test(n) ? k32 : /192/.test(n) ? k24 : k16;
    return crypto.createCipheriv(n, key, iv12).update('abc');
  });
}

// RFC 8439 2.8.2 (ChaCha20-Poly1305) and the NIST GCM test case 4.
show('rfc8439', () => {
  const key = Buffer.from('808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f', 'hex');
  const nonce = Buffer.from('070000004041424344454647', 'hex');
  const aad = Buffer.from('50515253c0c1c2c3c4c5c6c7', 'hex');
  const pt = "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
  const c = crypto.createCipheriv('chacha20-poly1305', key, nonce);
  c.setAAD(aad);
  const ct = Buffer.concat([c.update(pt), c.final()]);
  return `${ct.toString('hex')} ${c.getAuthTag().toString('hex')}`;
});
show('nist gcm 4', () => {
  const key = Buffer.from('feffe9928665731c6d6a8f9467308308', 'hex');
  const iv = Buffer.from('cafebabefacedbaddecaf888', 'hex');
  const pt = Buffer.from('d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39', 'hex');
  const c = crypto.createCipheriv('aes-128-gcm', key, iv);
  c.setAAD(Buffer.from('feedfacedeadbeeffeedfacedeadbeefabaddad2', 'hex'));
  const ct = Buffer.concat([c.update(pt), c.final()]);
  return `${ct.toString('hex')} ${c.getAuthTag().toString('hex')}`;
});

// Raw ChaCha20 (16-byte IV: block counter, nonce).
show('chacha20', () => {
  const iv = Buffer.from('01000000000000000000000000000004', 'hex');
  const c = crypto.createCipheriv('chacha20', k32, iv);
  const ct = Buffer.concat([c.update('hello chacha20 '.repeat(10)), c.final()]);
  const d = crypto.createDecipheriv('chacha20', k32, iv);
  return `${ct.toString('hex')} ${Buffer.concat([d.update(ct), d.final()]).toString().length}`;
});
show('chacha20 counter', () => crypto.createCipheriv('chacha20', k32, Buffer.from('ffffffff000000000000000000000001', 'hex')).update(Buffer.alloc(200)));
show('chacha20 iv12', () => crypto.createCipheriv('chacha20', k32, iv12));
show('chacha20 tag', () => crypto.createCipheriv('chacha20', k32, Buffer.alloc(16)).getAuthTag());

// Old modes through the same class.
show('cbc update after final', () => {
  const c = crypto.createCipheriv('aes-128-cbc', k16, Buffer.alloc(16));
  c.final();
  c.update('a');
});
show('cbc final twice', () => {
  const c = crypto.createCipheriv('aes-128-cbc', k16, Buffer.alloc(16));
  c.final();
  c.final();
});
show('ciphers', () => ['aes-128-gcm', 'aes-256-gcm', 'chacha20-poly1305', 'aes-128-cbc'].map((n) => crypto.getCiphers().includes(n)).join(' '));

// A buffer changed after `update` does not change the hash or the cipher.
show('mutation', () => {
  const b = Buffer.from('abcdef');
  const h = crypto.createHash('sha256').update(b);
  b[0] = 0;
  return h.digest('hex') === crypto.createHash('sha256').update('abcdef').digest('hex');
});
// Streams of odd chunk sizes across block boundaries.
for (const alg of ['aes-128-gcm', 'chacha20-poly1305']) {
  show(`${alg} odd chunks`, () => {
    const key = alg === 'aes-128-gcm' ? k16 : k32;
    const c = crypto.createCipheriv(alg, key, iv12);
    c.setAAD(Buffer.alloc(37, 7));
    const parts = [];
    let at = 0;
    for (const n of [1, 15, 16, 17, 31, 33, 63, 64, 65, 100, 129, 5]) {
      parts.push(c.update(Buffer.alloc(n, at++)));
    }
    parts.push(c.final());
    return `${Buffer.concat(parts).toString('hex')} ${c.getAuthTag().toString('hex')}`;
  });
}
