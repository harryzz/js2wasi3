// node's stream module (T60): Readable, Writable, Duplex, Transform, PassThrough, pipe; event order.
const stream = require("stream");
const { Readable, Writable, Duplex, Transform, PassThrough } = stream;

console.log(typeof stream, stream === stream.Stream, Object.keys(stream).sort().join());
console.log(require("node:stream") === stream, require("stream/promises") === stream.promises);
console.log(require("_stream_readable") === Readable, require("_stream_transform") === Transform);

const log = [];
const ev = (name, s, events) => {
  for (const e of events) s.on(e, (x) => log.push(`${name}:${e}${x === undefined ? "" : " " + String(x)}`));
};

// A Readable pushing synchronously in read(): flowing mode with 'data'.
let n = 0;
const r1 = new Readable({
  read() {
    n++;
    this.push(n <= 3 ? `c${n}` : null);
  },
});
ev("r1", r1, ["data", "end", "close", "readable"]);

// A Writable recording chunks with their encodings; 'finish' and 'close' order.
const chunks = [];
const w1 = new Writable({
  write(chunk, enc, cb) {
    chunks.push([Buffer.isBuffer(chunk), String(chunk), enc]);
    cb();
  },
  final(cb) {
    log.push("w1:final");
    cb();
  },
});
ev("w1", w1, ["finish", "close", "drain"]);
w1.write("a");
w1.write(Buffer.from("b"));
console.log("write returns", w1.write("c"), w1.writableLength, w1.writableHighWaterMark);
w1.end("d", () => log.push("w1:end-cb"));

// decodeStrings: false, objectMode.
const objs = [];
const w2 = new Writable({
  objectMode: true,
  write(o, enc, cb) {
    objs.push(o);
    setTimeout(cb, 1);
  },
});
w2.write({ a: 1 });
w2.write([1, 2]);
w2.end(42);
w2.on("finish", () => log.push("w2:finish " + JSON.stringify(objs)));

// Transform and PassThrough with pipe().
const upper = new Transform({
  transform(chunk, enc, cb) {
    cb(null, chunk.toString().toUpperCase());
  },
  flush(cb) {
    this.push("!");
    cb();
  },
});
const pt = new PassThrough();
let piped = "";
const sink = new Writable({
  write(c, e, cb) {
    piped += c;
    cb();
  },
});
sink.on("finish", () => log.push("piped " + piped));
Readable.from(["x", "y", "z"]).pipe(upper).pipe(pt).pipe(sink);

// Duplex: independent sides.
const d = new Duplex({
  read() {},
  write(c, e, cb) {
    this.push("echo:" + c);
    cb();
  },
  final(cb) {
    this.push(null);
    cb();
  },
});
d.setEncoding("utf8");
d.on("data", (c) => log.push("d:data " + c));
d.on("end", () => log.push("d:end"));
d.write("1");
d.end("2");

process.nextTick(() => log.push("tick"));
Promise.resolve().then(() => log.push("micro"));
setTimeout(() => {
  for (const l of log) console.log(l);
  console.log(JSON.stringify(chunks));
  console.log(r1.readableEnded, r1.destroyed, w1.writableFinished, w1.writableEnded, d.readableEnded);
}, 20);
