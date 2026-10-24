// zstd (T150 slice 3): libzstd gives node's bytes at the same level, parameters and dictionary; round trips, streams, errors.
import zlib from 'node:zlib';
import { pipeline } from 'node:stream/promises';
import { Readable, Writable } from 'node:stream';

const { constants: C } = zlib;
const text = Buffer.from('zstd test data, zstd test data! '.repeat(40));
const dict = Buffer.from('zstd test data');

function show(label, f) {
  try {
    const r = f();
    console.log(label, Buffer.isBuffer(r) ? r.toString('hex').slice(0, 100) : r);
  } catch (e) {
    console.log(label, e.name, e.code, e.errno, e.message);
  }
}

// Bytes identical to node's.
const c = zlib.zstdCompressSync(text);
show('default', () => c);
show('empty', () => zlib.zstdCompressSync(Buffer.alloc(0)));
for (const level of [1, 3, 9, 19, 22, -5, -100]) show('level ' + level, () => zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_compressionLevel]: level } }));
show('checksum', () => zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_checksumFlag]: 1 } }));
show('no content size', () => zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_contentSizeFlag]: 0 } }));
show('window log', () => zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_windowLog]: 10 } }));
show('strategy', () => zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_strategy]: C.ZSTD_btultra } }));
show('pledged', () => zlib.zstdCompressSync(text, { pledgedSrcSize: text.length }));
show('dictionary', () => zlib.zstdCompressSync(text, { dictionary: dict }));

// Round trips.
show('roundtrip', () => zlib.zstdDecompressSync(c).equals(text));
show('roundtrip string', () => zlib.zstdDecompressSync(zlib.zstdCompressSync('héllo wörld')).toString());
show('roundtrip arraybuffer', () => zlib.zstdDecompressSync(zlib.zstdCompressSync(new Uint8Array([1, 2, 3]).buffer)).toString('hex'));
show('roundtrip empty', () => zlib.zstdDecompressSync(zlib.zstdCompressSync(Buffer.alloc(0))).length);
show('dictionary roundtrip', () => zlib.zstdDecompressSync(zlib.zstdCompressSync(text, { dictionary: dict }), { dictionary: dict }).equals(text));
show('big', () => {
  const big = Buffer.alloc(300000);
  for (let i = 0; i < big.length; i++) big[i] = (i * 31 + (i >> 7)) & 0xff;
  return zlib.zstdDecompressSync(zlib.zstdCompressSync(big)).equals(big);
});
show('trailing junk ignored', () => zlib.zstdDecompressSync(Buffer.concat([c, Buffer.from('junk')])).length);
show('two frames', () => zlib.zstdDecompressSync(Buffer.concat([c, c])).length);
show('decoder windowLogMax', () => zlib.zstdDecompressSync(c, { params: { [C.ZSTD_d_windowLogMax]: 10 } }).length);

// Errors.
show('garbage', () => zlib.zstdDecompressSync(Buffer.from('this is not zstd data')));
show('truncated', () => zlib.zstdDecompressSync(c.subarray(0, c.length - 4)));
show('empty input', () => zlib.zstdDecompressSync(Buffer.alloc(0)));
show('bad checksum', () => { const z = zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_checksumFlag]: 1 } }); z[z.length - 1] ^= 1; return zlib.zstdDecompressSync(z); });
show('dictionary missing', () => zlib.zstdDecompressSync(zlib.zstdCompressSync(text, { dictionary: dict })));
show('pledged wrong', () => zlib.zstdCompressSync(text, { pledgedSrcSize: 5 }));
show('bad param key', () => zlib.zstdCompressSync(text, { params: { 999: 1 } }));
show('bad param type', () => zlib.zstdCompressSync(text, { params: { [C.ZSTD_c_compressionLevel]: 'x' } }));
show('maxOutputLength', () => zlib.zstdDecompressSync(c, { maxOutputLength: 100 }));
show('bad arg', () => zlib.zstdCompressSync(5));

// Callbacks and streams.
console.log('start');
zlib.zstdCompress(text, (err, z) => {
  console.log('compress cb', err, z.toString('hex').slice(0, 40));
  zlib.zstdDecompress(z, async (err2, back) => {
    console.log('decompress cb', err2, back.equals(text));
    const out = [];
    await pipeline(Readable.from([text.subarray(0, 700), text.subarray(700)]), zlib.createZstdCompress(), zlib.createZstdDecompress(),
      new Writable({ write(chunk, e, cb) { out.push(chunk); cb(); } }));
    console.log('pipeline', Buffer.concat(out).equals(text));
    const enc = zlib.createZstdCompress();
    const parts = [];
    enc.on('data', (d) => parts.push(d));
    enc.write('flush me ');
    await new Promise((r) => enc.flush(r));
    console.log('flushed', Buffer.concat(parts).length > 0);
    enc.end('end');
    await new Promise((r) => enc.on('end', r));
    console.log('stream total', zlib.zstdDecompressSync(Buffer.concat(parts)).toString());
    const bad = zlib.createZstdDecompress();
    const err3 = await new Promise((r) => { bad.on('error', r); bad.end(Buffer.from('this is not zstd data')); });
    console.log('stream error', err3.message, err3.code, err3.errno);
    console.log(zlib.createZstdCompress.name, zlib.ZstdCompress.name, new zlib.ZstdDecompress() instanceof zlib.ZstdDecompress);
  });
});
console.log('end');
