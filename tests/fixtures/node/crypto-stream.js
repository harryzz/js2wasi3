// Hash, Hmac, Cipheriv and Decipheriv are streams (T153): pipe, pipeline, events, setEncoding, errors, as node's `LazyTransform`s.
import crypto from 'node:crypto';
import { Readable, Writable, Transform, pipeline, finished } from 'node:stream';
import { pipeline as pipelineP } from 'node:stream/promises';
import { once } from 'node:events';

function collect() {
  const chunks = [];
  const w = new Writable({
    write(chunk, encoding, cb) {
      chunks.push(typeof chunk === 'string' ? chunk : Buffer.from(chunk));
      cb();
    },
  });
  w.chunks = chunks;
  return w;
}

const show = (label, v) => console.log(label, Buffer.isBuffer(v) ? v.toString('hex') : v);

show('instanceof', [crypto.createHash('md5') instanceof Transform, crypto.createHmac('md5', 'k') instanceof Transform,
  crypto.createCipheriv('aes-128-cbc', Buffer.alloc(16), Buffer.alloc(16)) instanceof Transform,
  crypto.createDecipheriv('aes-128-cbc', Buffer.alloc(16), Buffer.alloc(16)) instanceof Transform].join(' '));
show('methods', ['write', 'end', 'pipe', 'on', 'read', 'setEncoding', 'update', 'digest'].map((n) => typeof crypto.createHash('md5')[n]).join(' '));

// pipe into a hash, read the digest after 'finish'
{
  const h = crypto.createHash('sha1');
  Readable.from([Buffer.from('a'), Buffer.from('bc')]).pipe(h);
  await once(h, 'finish');
  show('pipe finish', h.read());
}

// 'data' and 'end' events with write/end
{
  const h = crypto.createHash('md5');
  const seen = [];
  h.on('data', (d) => seen.push(Buffer.isBuffer(d) + ':' + d.toString('hex')));
  h.write('hello');
  h.end();
  await once(h, 'end');
  show('data events', seen.join(' '));
}

// setEncoding and 'readable'
{
  const h = crypto.createHash('sha256');
  h.setEncoding('hex');
  h.end('x');
  await once(h, 'readable');
  show('readable hex', h.read());
}

// write with an encoding
{
  const h = crypto.createHash('sha256');
  h.write('68656c6c6f', 'hex');
  h.end();
  await once(h, 'readable');
  show('write hex', h.read());
}

// digest() before the end, then the stream still ends with the same digest
{
  const h = crypto.createHash('md5');
  h.end('abc');
  show('digest after end', h.digest('hex'));
  await once(h, 'readable');
  show('read after digest', h.read());
}

// pipeline with callbacks and promises
{
  const out = collect();
  await new Promise((resolve) => {
    pipeline(Readable.from([Buffer.from('hello')]), crypto.createHash('sha256').setEncoding('hex'), out, (e) => {
      show('pipeline done', String(e));
      resolve();
    });
  });
  show('pipeline out', out.chunks.join(''));
  const out2 = collect();
  await pipelineP(Readable.from(['a', 'b', 'c']), crypto.createHmac('sha1', 'key'), out2);
  show('pipeline promise hmac', Buffer.concat(out2.chunks));
}

// async iteration
{
  const h = crypto.createHash('sha512');
  h.end('iter');
  const parts = [];
  for await (const chunk of h) parts.push(chunk.toString('hex'));
  show('async iterator', parts.join('|'));
}

// options: stream options with the hash options
{
  const h = crypto.createHash('shake128', { outputLength: 5, encoding: 'hex' });
  h.end('abc');
  await once(h, 'readable');
  show('options', h.read());
}

// ciphers through pipes
{
  const key = Buffer.alloc(16, 1);
  const iv = Buffer.alloc(16, 2);
  const enc = collect();
  await pipelineP(Readable.from([Buffer.from('secret message '), Buffer.from('that is long enough for blocks')]), crypto.createCipheriv('aes-128-cbc', key, iv), enc);
  const encrypted = Buffer.concat(enc.chunks);
  show('cbc stream', encrypted);
  const dec = collect();
  await pipelineP(Readable.from([encrypted.subarray(0, 7), encrypted.subarray(7)]), crypto.createDecipheriv('aes-128-cbc', key, iv), dec);
  show('cbc roundtrip', Buffer.concat(dec.chunks).toString());
  // GCM: the tag after the stream ended
  const c = crypto.createCipheriv('aes-128-gcm', key, Buffer.alloc(12, 3));
  c.setAAD(Buffer.from('aad'));
  const out = collect();
  await pipelineP(Readable.from([Buffer.from('gcm data')]), c, out);
  show('gcm stream', Buffer.concat(out.chunks));
  show('gcm tag', c.getAuthTag());
  const d = crypto.createDecipheriv('aes-128-gcm', key, Buffer.alloc(12, 3));
  d.setAAD(Buffer.from('aad'));
  d.setAuthTag(c.getAuthTag());
  const out2 = collect();
  await pipelineP(Readable.from([Buffer.concat(out.chunks)]), d, out2);
  show('gcm roundtrip', Buffer.concat(out2.chunks).toString());
  // ChaCha20-Poly1305
  const cc = crypto.createCipheriv('chacha20-poly1305', Buffer.alloc(32, 4), Buffer.alloc(12, 5));
  const out3 = collect();
  await pipelineP(Readable.from([Buffer.from('chacha data')]), cc, out3);
  show('chacha stream', Buffer.concat(out3.chunks));
  show('chacha tag', cc.getAuthTag());
}

// the plain API still works on the same objects
show('plain', crypto.createHash('sha1').update('a').update('b').digest('hex'));
show('finished', typeof finished);
