// Brotli (T150 slice 2): the Rust brotli crate gives node's bytes at the same quality, window and mode; round trips, streams, errors.
import zlib from 'node:zlib';
import { pipeline } from 'node:stream/promises';
import { Readable, Writable } from 'node:stream';

const { constants: c } = zlib;
const text = Buffer.from('Brotli compresses text well, text like this text, text and more text. '.repeat(40));
const small = Buffer.from('hello brotli');

function show(label, f) {
  try {
    const r = f();
    console.log(label, Buffer.isBuffer(r) ? r.toString('hex').slice(0, 80) : r);
  } catch (e) {
    console.log(label, e.name, e.code, e.message);
  }
}

// Bytes identical to node's (the same algorithm and parameters).
show('small', () => zlib.brotliCompressSync(small));
show('empty', () => zlib.brotliCompressSync(Buffer.alloc(0)));
for (let q = 0; q <= 11; q++) show('quality ' + q, () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_QUALITY]: q } }));
for (const w of [10, 16, 22, 24]) show('lgwin ' + w, () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_LGWIN]: w } }));
for (const m of [0, 1, 2]) show('mode ' + m, () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_MODE]: m } }));
show('size hint', () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_SIZE_HINT]: text.length } }));
show('lgblock', () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_LGBLOCK]: 16 } }));
show('no context modeling', () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING]: 1 } }));

// Round trips.
show('roundtrip', () => zlib.brotliDecompressSync(zlib.brotliCompressSync(text)).equals(text));
show('roundtrip string', () => zlib.brotliDecompressSync(zlib.brotliCompressSync('héllo wörld')).toString());
show('roundtrip arraybuffer', () => zlib.brotliDecompressSync(zlib.brotliCompressSync(new Uint8Array([1, 2, 3]).buffer)).toString('hex'));
show('roundtrip empty', () => zlib.brotliDecompressSync(zlib.brotliCompressSync(Buffer.alloc(0))).length);
show('big', () => {
  const big = Buffer.alloc(300000);
  for (let i = 0; i < big.length; i++) big[i] = (i * 31 + (i >> 7)) & 0xff;
  return zlib.brotliDecompressSync(zlib.brotliCompressSync(big, { params: { [c.BROTLI_PARAM_QUALITY]: 4 } })).equals(big);
});
show('trailing junk ignored', () => zlib.brotliDecompressSync(Buffer.concat([zlib.brotliCompressSync(text), Buffer.from('junk')])).length);
show('decoder large window', () => zlib.brotliDecompressSync(zlib.brotliCompressSync(text), { params: { [c.BROTLI_DECODER_PARAM_LARGE_WINDOW]: 1 } }).length);

// Errors.
show('truncated', () => zlib.brotliDecompressSync(zlib.brotliCompressSync(text).subarray(0, 10)));
show('empty input', () => zlib.brotliDecompressSync(Buffer.alloc(0)));
show('garbage', () => { try { zlib.brotliDecompressSync(Buffer.from('this is not brotli')); } catch (e) { return [e.message, e.code.startsWith('ERR__ERROR_FORMAT_'), e.errno < 0].join(); } });
show('bad param key', () => zlib.brotliCompressSync(text, { params: { 999: 1 } }));
show('bad param type', () => zlib.brotliCompressSync(text, { params: { [c.BROTLI_PARAM_QUALITY]: 'x' } }));
show('maxOutputLength', () => zlib.brotliDecompressSync(zlib.brotliCompressSync(text), { maxOutputLength: 100 }));
show('bad arg', () => zlib.brotliCompressSync(5));

// Callbacks and streams.
console.log('start');
zlib.brotliCompress(text, (err, z) => {
  console.log('compress cb', err, z.toString('hex').slice(0, 40));
  zlib.brotliDecompress(z, async (err2, back) => {
    console.log('decompress cb', err2, back.equals(text));
    const out = [];
    await pipeline(Readable.from([text.subarray(0, 700), text.subarray(700)]), zlib.createBrotliCompress(), zlib.createBrotliDecompress(),
      new Writable({ write(chunk, e, cb) { out.push(chunk); cb(); } }));
    console.log('pipeline', Buffer.concat(out).equals(text));
    const enc = zlib.createBrotliCompress({ params: { [c.BROTLI_PARAM_QUALITY]: 3 } });
    const parts = [];
    enc.on('data', (d) => parts.push(d));
    enc.write('flush me ');
    await new Promise((r) => enc.flush(r));
    console.log('flushed', Buffer.concat(parts).length > 0);
    enc.end('end');
    await new Promise((r) => enc.on('end', r));
    console.log('stream total', zlib.brotliDecompressSync(Buffer.concat(parts)).toString());
    const bad = zlib.createBrotliDecompress();
    const err3 = await new Promise((r) => { bad.on('error', r); bad.end(Buffer.from('this is not brotli')); });
    console.log('stream error', err3.message, err3.code.startsWith('ERR__ERROR_'));
    console.log(zlib.createBrotliCompress.name, zlib.BrotliCompress.name, new zlib.BrotliDecompress() instanceof zlib.BrotliDecompress);
  });
});
console.log('end');
