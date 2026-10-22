// zlib streams (T150): createGzip/createGunzip/createDeflate/..., pipe and pipeline, flush, params, reset, close, events, backpressure,
// async iteration, promisified one-shot functions. Output depends only on the decompressed data.
import zlib from 'node:zlib';
import { pipeline } from 'node:stream/promises';
import { Readable, Writable } from 'node:stream';
import { promisify } from 'node:util';

const text = Buffer.from('stream me, stream me, stream me! '.repeat(400));

function collect() {
  const chunks = [];
  const w = new Writable({ write(c, e, cb) { chunks.push(c); cb(); } });
  w.result = () => Buffer.concat(chunks);
  return w;
}

// pipe / pipeline round trips through every pair.
for (const [c, d] of [['createGzip', 'createGunzip'], ['createDeflate', 'createInflate'], ['createDeflateRaw', 'createInflateRaw'], ['createGzip', 'createUnzip']]) {
  const out = collect();
  await pipeline(Readable.from([text.subarray(0, 5000), text.subarray(5000)]), zlib[c](), zlib[d](), out);
  console.log(c, d, out.result().equals(text));
}

// Small chunks and many writes.
{
  const gz = zlib.createGzip({ chunkSize: 64 });
  const parts = [];
  gz.on('data', (c) => parts.push(c));
  const done = new Promise((r) => gz.on('end', r));
  for (let i = 0; i < text.length; i += 100) gz.write(text.subarray(i, i + 100));
  gz.end();
  await done;
  console.log('small chunks', zlib.gunzipSync(Buffer.concat(parts)).equals(text), parts.every((p) => p.length <= 64));
}

// flush: data is available before end.
{
  const d = zlib.createDeflate();
  const got = [];
  d.on('data', (c) => got.push(c));
  d.write('hello ');
  await new Promise((r) => d.flush(r));
  const first = Buffer.concat(got);
  console.log('after flush', first.length > 0);
  d.write('world');
  await new Promise((r) => d.flush(zlib.constants.Z_FULL_FLUSH, r));
  d.end();
  await new Promise((r) => d.on('end', r));
  console.log('flush total', zlib.inflateSync(Buffer.concat(got)).toString());
}

// params (level/strategy change mid-stream) and reset.
{
  const d = zlib.createDeflate({ level: 1 });
  const got = [];
  d.on('data', (c) => got.push(c));
  d.write(text.subarray(0, 3000));
  await new Promise((r) => d.params(9, zlib.constants.Z_DEFAULT_STRATEGY, r));
  d.write(text.subarray(3000));
  d.end();
  await new Promise((r) => d.on('end', r));
  console.log('params', zlib.inflateSync(Buffer.concat(got)).equals(text));
  const i = zlib.createInflate();
  i.reset();
  console.log('reset', typeof i.reset, i.bytesWritten);
}

// bytesWritten, events, close.
{
  const g = zlib.createGzip();
  const events = [];
  for (const e of ['finish', 'end', 'close']) g.on(e, () => events.push(e));
  g.resume();
  g.end('abc');
  await new Promise((r) => g.on('close', r));
  console.log('events', events.join(), g.bytesWritten, g._closed, g.destroyed);
  const c = zlib.createDeflate();
  await new Promise((r) => c.close(r));
  console.log('close', c.destroyed, c._closed);
}

// Errors on streams.
{
  const g = zlib.createGunzip();
  const err = await new Promise((r) => { g.on('error', r); g.end(Buffer.from('not gzip data at all')); });
  console.log('stream error', err.code, err.errno, err.message, g.destroyed);
  const t = zlib.createInflate();
  const err2 = await new Promise((r) => { t.on('error', r); t.end(zlib.deflateSync(text).subarray(0, 30)); });
  console.log('truncated stream', err2.code, err2.message);
  try {
    await pipeline(Readable.from([Buffer.from('xxxxxxxx')]), zlib.createInflate(), collect());
  } catch (e) {
    console.log('pipeline error', e.code, e.message);
  }
}

// Async iteration and promisified functions.
{
  const chunks = [];
  for await (const c of Readable.from([text]).pipe(zlib.createGzip())) chunks.push(c);
  console.log('async iter', zlib.gunzipSync(Buffer.concat(chunks)).equals(text));
  const gz = await promisify(zlib.gzip)(text);
  console.log('promisify', (await promisify(zlib.gunzip)(gz)).equals(text));
}

// Backpressure with a slow consumer.
{
  const big = Buffer.alloc(300000, 'abcdefghij');
  const g = zlib.createGzip();
  let n = 0;
  const slow = new Writable({ highWaterMark: 16, write(c, e, cb) { n += c.length; setImmediate(cb); } });
  await pipeline(Readable.from([big]), g, slow);
  console.log('backpressure', n > 0, g.bytesWritten);
}

// Constructors.
{
  const g = zlib.Gzip();
  console.log('call without new', g instanceof zlib.Gzip);
  console.log(Object.getPrototypeOf(zlib.Gzip.prototype) === Object.getPrototypeOf(zlib.Deflate.prototype));
  console.log(new zlib.Unzip() instanceof (await import('node:stream')).Transform);
}
