// T80: node:buffer's UTF-8 length, encoding and decoding (native for Buffers), byte for byte as node.
import { Buffer } from 'node:buffer';

const hex = (b) => b.toString('hex');
const units = (s) => Array.from(s, (c) => c.codePointAt(0).toString(16)).join(' ');

const strings = [
  '', 'a', 'hello world', '\x7f', '\x80', 'é', 'ß€', '߿', 'ࠀ', '￿', '日本語',
  '😀', 'a😀b', '𝄞x', '\u{10ffff}',
  '\ud800', '\udc00', 'a\ud800', 'a\udc00b', '\ud800\ud800', '\udc00\ud800', '\ud800a', 'x\ud83d',
  '😀\ud83d', 'mixed é€😀\ud800 end',
];

for (const s of strings) {
  const b = Buffer.from(s);
  console.log(JSON.stringify(s), Buffer.byteLength(s), Buffer.byteLength(s, 'utf8'), Buffer.byteLength(s, 'UTF-8'), hex(b),
    b.length, units(b.toString()));
}

// write with offset and length limits: never a partial character.
const w = ['abc', 'é', '€', '😀', 'a😀', '\ud800', 'ab\udc00', 'é€😀x'];
for (const s of w) {
  for (let len = 0; len <= 6; len++) {
    const b = Buffer.alloc(8, 0x2e);
    const n = b.write(s, 1, len);
    console.log(JSON.stringify(s), len, n, hex(b));
  }
  const b = Buffer.alloc(3, 0x2e);
  console.log('end', JSON.stringify(s), b.write(s, 1), hex(b), b.write(s), hex(b), b.write(s, 'utf8'), b.write(s, 2, 'utf-8'), hex(b));
}

// Decoding: invalid, truncated, overlong and surrogate sequences.
const bytes = [
  [0x61, 0x80, 0x62], [0xc0, 0x80], [0xc1, 0xbf], [0xc2], [0xc2, 0x41], [0xe0, 0x80, 0x80], [0xe0, 0xa0],
  [0xe0, 0xa0, 0x80], [0xed, 0xa0, 0x80], [0xed, 0x9f, 0xbf], [0xef, 0xbf, 0xbf], [0xf0, 0x8f, 0xbf, 0xbf],
  [0xf0, 0x90, 0x80, 0x80], [0xf0, 0x90, 0x80], [0xf4, 0x8f, 0xbf, 0xbf], [0xf4, 0x90, 0x80, 0x80], [0xf5, 0x80],
  [0xff, 0xfe, 0x41], [0xe2, 0x82], [0xe2, 0x82, 0xac, 0xe2], [0xf0, 0x9f, 0x98, 0x80, 0x80], [0xef, 0xbb, 0xbf, 0x41],
];
for (const a of bytes) {
  const b = Buffer.from(a);
  console.log(hex(b), units(b.toString()), units(b.toString('utf8', 1)), units(b.toString('utf-8', 0, a.length - 1)),
    units(b.subarray(1).toString()));
}
console.log(JSON.stringify(Buffer.from([0x68, 0x69, 0xe2, 0x82, 0xac]).toString('utf8', 1, 100)));
console.log(JSON.stringify(Buffer.from('héllo').toString('utf8', 3, 2)));

// Uint8Array views and TextEncoder / TextDecoder.
const u = new Uint8Array([0xe2, 0x82, 0xac, 0x41]);
console.log(Buffer.from(u.buffer, 1, 3).toString(), units(Buffer.prototype.toString.call(u)));
const te = new TextEncoder();
for (const s of strings) console.log(Array.from(te.encode(s)).join(','));
console.log(te.encode().length, te.encode(12.5).join(','));
console.log(new TextDecoder().decode(te.encode('round trip é😀')));

// Large strings (≥ 1 MiB).
const big = 'aé€😀'.repeat(100000) + '\ud800';
const bb = Buffer.from(big);
console.log(big.length, Buffer.byteLength(big), bb.length, bb.toString() === big.slice(0, -1) + '�',
  bb.subarray(5, 1000005).toString().length);
const ascii = 'x'.repeat(1 << 20);
console.log(Buffer.from(ascii).toString() === ascii, Buffer.byteLength(ascii));
