// getCipherInfo, getCurves, getFips/setFips/fips, secureHeapUsed, randomUUIDv7 (T153).
import crypto from 'node:crypto';

function show(label, f) {
  try {
    console.log(label, JSON.stringify(f()));
  } catch (e) {
    console.log(label, 'THROW', e.name, e.code, e.message);
  }
}

const names = [];
for (const b of [128, 192, 256]) for (const m of ['ecb', 'cbc', 'ctr', 'ccm', 'gcm']) names.push(`aes-${b}-${m}`);
names.push('chacha20', 'chacha20-poly1305', 'aes128', 'aes192', 'aes256', 'AES-128-CBC', 'id-aes128-GCM', 'id-aes256-CCM', 'id-aes192-GCM', 'nope', '');
for (const n of names) show(`info ${n}`, () => crypto.getCipherInfo(n));
for (const nid of [418, 419, 422, 423, 426, 427, 895, 896, 898, 899, 901, 902, 904, 905, 906, 1018, 1019, 0, -1, 99999]) {
  show(`nid ${nid}`, () => crypto.getCipherInfo(nid));
}
const opts = [
  ['aes-128-gcm', { ivLength: 16 }], ['aes-128-gcm', { ivLength: 1 }], ['aes-128-gcm', { ivLength: 0 }], ['aes-128-gcm', { ivLength: 100 }],
  ['aes-128-ccm', { ivLength: 7 }], ['aes-128-ccm', { ivLength: 13 }], ['aes-128-ccm', { ivLength: 14 }], ['aes-128-ccm', { ivLength: 6 }],
  ['chacha20-poly1305', { ivLength: 11 }], ['chacha20-poly1305', { ivLength: 12 }], ['chacha20', { ivLength: 16 }], ['chacha20', { ivLength: 12 }],
  ['aes-128-cbc', { ivLength: 15 }], ['aes-128-cbc', { ivLength: 16 }], ['aes-128-ecb', { ivLength: 16 }], ['aes-128-ecb', { ivLength: 0 }],
  ['aes-128-cbc', { keyLength: 16 }], ['aes-128-cbc', { keyLength: 17 }], ['aes-128-cbc', {}], ['aes-128-ctr', { ivLength: 12 }],
  ['aes-128-gcm', { keyLength: 32 }], ['chacha20', { keyLength: 16 }], ['aes-256-gcm', { keyLength: 32, ivLength: 12 }],
];
for (const [n, o] of opts) show(`info ${n} ${JSON.stringify(o)}`, () => crypto.getCipherInfo(n, o));
show('options null', () => crypto.getCipherInfo('aes-128-cbc', null));
show('options number', () => crypto.getCipherInfo('aes-128-cbc', 1));
show('keyLength string', () => crypto.getCipherInfo('aes-128-cbc', { keyLength: 'a' }));
show('keyLength float', () => crypto.getCipherInfo('aes-128-cbc', { keyLength: 1.5 }));
show('ivLength string', () => crypto.getCipherInfo('aes-128-gcm', { ivLength: 'x' }));
show('no args', () => crypto.getCipherInfo());
show('null', () => crypto.getCipherInfo(null));
show('object', () => crypto.getCipherInfo({}));
show('float nid', () => crypto.getCipherInfo(1.5));

// Curves: none claimed, so only the type is compared.
show('getCurves', () => Array.isArray(crypto.getCurves()));
show('fips', () => [crypto.getFips(), typeof crypto.setFips, crypto.secureHeapUsed().total, crypto.secureHeapUsed().min]);
show('setFips(false)', () => crypto.setFips(false));
show('secureHeapUsed', () => Object.keys(crypto.secureHeapUsed()));

// UUID v7: format, version and variant, the timestamp.
const before = Date.now();
const ids = [];
for (let i = 0; i < 20; i++) ids.push(crypto.randomUUIDv7());
const after = Date.now();
show('uuid7 format', () => ids.every((u) => /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(u)));
show('uuid7 time', () => ids.every((u) => {
  const ms = parseInt(u.replace(/-/g, '').slice(0, 12), 16);
  return ms >= before && ms <= after;
}));
show('uuid7 unique', () => new Set(ids).size === ids.length);
show('uuid7 options', () => crypto.randomUUIDv7({ disableEntropyCache: true }).length);
show('uuid7 bad options', () => crypto.randomUUIDv7(1));
show('uuid7 null options', () => crypto.randomUUIDv7(null));
