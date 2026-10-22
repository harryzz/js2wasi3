// zlib (T150): deflate/inflate/gzip/gunzip/unzip/raw one-shot: round trips, streams made by node decoded here, options, dictionary,
// members, garbage. Compressed sizes are not printed: zlib-rs and node's zlib choose different matches.
import zlib from 'node:zlib';

const text = Buffer.from('The quick brown fox jumps over the lazy dog. '.repeat(20));
const node = {"deflate": "789c0bc94855282ccd4cce56482aca2fcf5348cbaf50c82acd2d2856c82f4b2d5228c94855c849acaa5448c94fd7530819553caa785431b515030047a5431c", "gzip": "1f8b08000000000000030bc94855282ccd4cce56482aca2fcf5348cbaf50c82acd2d2856c82f4b2d5228c94855c849acaa5448c94fd7530819553caa785431b5150300e64a66b084030000", "deflateRaw": "0bc94855282ccd4cce56482aca2fcf5348cbaf50c82acd2d2856c82f4b2d5228c94855c849acaa5448c94fd7530819553caa785431b5150300", "dict": "78bb2ee905d30bc9485528441552c82acd2d2856c82f4b2d5228c94855c849acaa5448c94fd75318553caa785431d515030047a5431c", "multi": "1f8b08000000000000034bcb2c2a2e510000fc7af11c060000001f8b08000000000000032b4e4dcecf4b010069111fb606000000"};
const fromHex = (h) => Buffer.from(h, 'hex');

function show(label, f) {
  try {
    const r = f();
    console.log(label, Buffer.isBuffer(r) ? r.toString('latin1').slice(0, 60) : r);
  } catch (e) {
    console.log(label, e.name, e.code, e.errno, e.message);
  }
}

// Streams node made.
show('node deflate', () => zlib.inflateSync(fromHex(node.deflate)).equals(text));
show('node gzip', () => zlib.gunzipSync(fromHex(node.gzip)).equals(text));
show('node raw', () => zlib.inflateRawSync(fromHex(node.deflateRaw)).equals(text));
show('node unzip gzip', () => zlib.unzipSync(fromHex(node.gzip)).equals(text));
show('node unzip deflate', () => zlib.unzipSync(fromHex(node.deflate)).equals(text));
show('node dict', () => zlib.inflateSync(fromHex(node.dict), { dictionary: Buffer.from('quick brown fox') }).equals(text));
show('node dict missing', () => zlib.inflateSync(fromHex(node.dict)));
show('node dict bad', () => zlib.inflateSync(fromHex(node.dict), { dictionary: Buffer.from('other') }));
show('node members', () => zlib.gunzipSync(fromHex(node.multi)).toString());

// Round trips, every function, levels, strategies.
const kinds = [['deflate', 'inflate'], ['gzip', 'gunzip'], ['deflateRaw', 'inflateRaw'], ['gzip', 'unzip'], ['deflate', 'unzip']];
for (const [c, d] of kinds) {
  show(`${c}/${d}`, () => zlib[d + 'Sync'](zlib[c + 'Sync'](text)).equals(text));
  show(`${c}/${d} empty`, () => zlib[d + 'Sync'](zlib[c + 'Sync'](Buffer.alloc(0))).length);
  show(`${c}/${d} string`, () => zlib[d + 'Sync'](zlib[c + 'Sync']('héllo wörld')).toString());
  show(`${c}/${d} arraybuffer`, () => zlib[d + 'Sync'](zlib[c + 'Sync'](new Uint8Array([1, 2, 3]).buffer)).toString('hex'));
}
for (let level = -1; level <= 9; level++) show('level ' + level, () => zlib.inflateSync(zlib.deflateSync(text, { level })).equals(text));
for (const strategy of [0, 1, 2, 3, 4]) show('strategy ' + strategy, () => zlib.inflateSync(zlib.deflateSync(text, { strategy })).equals(text));
for (const windowBits of [9, 10, 12, 15]) show('windowBits ' + windowBits, () => zlib.inflateSync(zlib.deflateSync(text, { windowBits }), { windowBits }).equals(text));
for (const memLevel of [1, 5, 9]) show('memLevel ' + memLevel, () => zlib.inflateSync(zlib.deflateSync(text, { memLevel })).equals(text));
show('level 0 stored', () => zlib.deflateSync(text, { level: 0 }).length > text.length);
show('dictionary', () => zlib.inflateSync(zlib.deflateSync(text, { dictionary: Buffer.from('lazy dog') }), { dictionary: Buffer.from('lazy dog') }).equals(text));
show('dictionary raw', () => zlib.inflateRawSync(zlib.deflateRawSync(text, { dictionary: Buffer.from('lazy dog') }), { dictionary: Buffer.from('lazy dog') }).equals(text));
show('big', () => {
  const big = Buffer.alloc(1 << 20);
  for (let i = 0; i < big.length; i++) big[i] = (i * 7 + (i >> 8)) & 0xff;
  return zlib.gunzipSync(zlib.gzipSync(big)).equals(big);
});
show('chunkSize', () => zlib.inflateSync(zlib.deflateSync(text, { chunkSize: 64 }), { chunkSize: 64 }).equals(text));
show('info', () => { const r = zlib.deflateSync(text, { info: true }); return Object.keys(r).join() + ' ' + Buffer.isBuffer(r.buffer) + ' ' + r.engine.constructor.name; });
show('maxOutputLength', () => zlib.inflateSync(zlib.deflateSync(text), { maxOutputLength: 100 }));

// Garbage and members.
const gz = zlib.gzipSync('abc');
show('two members', () => zlib.gunzipSync(Buffer.concat([zlib.gzipSync('abc'), zlib.gzipSync('def')])).toString());
show('trailing zeros', () => zlib.gunzipSync(Buffer.concat([gz, Buffer.alloc(8)])).toString());
show('trailing garbage gunzip', () => zlib.gunzipSync(Buffer.concat([gz, Buffer.from('junk')])).toString());
show('trailing garbage inflate', () => zlib.inflateSync(Buffer.concat([zlib.deflateSync('abc'), Buffer.from('junk')])).toString());
show('rejectGarbageAfterEnd', () => zlib.inflateSync(Buffer.concat([zlib.deflateSync('abc'), Buffer.from('junk')]), { rejectGarbageAfterEnd: true }));

// Errors.
show('bad header', () => zlib.inflateSync(Buffer.from('hello world')));
show('bad gzip', () => zlib.gunzipSync(Buffer.from('hello world')));
show('truncated', () => zlib.inflateSync(zlib.deflateSync(text).subarray(0, 20)));
show('truncated gzip', () => zlib.gunzipSync(zlib.gzipSync(text).subarray(0, 20)));
show('truncated finishFlush sync', () => zlib.inflateSync(zlib.deflateSync(text).subarray(0, 20), { finishFlush: zlib.constants.Z_SYNC_FLUSH }).length > 0);
show('corrupt', () => { const z = zlib.deflateSync(text); z[10] ^= 0xff; return zlib.inflateSync(z); });
show('empty input inflate', () => zlib.inflateSync(Buffer.alloc(0)));
show('bad arg', () => zlib.deflateSync(5));
show('bad arg null', () => zlib.gzipSync(null));
show('bad level', () => zlib.deflateSync(text, { level: 10 }));
show('bad level type', () => zlib.deflateSync(text, { level: 'x' }));
show('bad windowBits', () => zlib.deflateSync(text, { windowBits: 7 }));
show('bad memLevel', () => zlib.deflateSync(text, { memLevel: 0 }));
show('bad strategy', () => zlib.deflateSync(text, { strategy: 9 }));
show('bad chunkSize', () => zlib.deflateSync(text, { chunkSize: 10 }));
show('bad dictionary', () => zlib.deflateSync(text, { dictionary: 'str' }));
show('bad flush', () => zlib.deflateSync(text, { flush: 99 }));
show('bad callback', () => zlib.deflate(text, {}, 5));

// Callbacks.
console.log('start');
zlib.gzip(text, (err, z) => {
  console.log('gzip cb', err, Buffer.isBuffer(z));
  zlib.gunzip(z, (err2, back) => {
    console.log('gunzip cb', err2, back.equals(text));
    zlib.inflate(Buffer.from('bad'), (err3, r) => {
      console.log('inflate cb', err3 && err3.code, err3 && err3.message, r);
      zlib.deflate(text, { level: 9 }, (e4, z4) => zlib.inflate(z4, (e5, b5) => console.log('options cb', e4, e5, b5.equals(text))));
    });
  });
});
console.log('end');
console.log(zlib.crc32('hello'), zlib.crc32(Buffer.from('world'), 5), zlib.crc32('', 0));
console.log(Object.keys(zlib.codes).length, zlib.constants.Z_BEST_COMPRESSION, zlib.Z_DEFAULT_COMPRESSION, typeof zlib.createGzip, zlib.createGzip.name);
