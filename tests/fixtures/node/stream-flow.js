// node's stream module (T60): backpressure, read(n), unshift, pause/resume, cork/writev,
// subclasses (class and util.inherits), the legacy Stream, object mode.
import stream from "stream";
import util from "node:util";
import events from "node:events";
const { Readable, Writable, Transform } = stream;

const log = (...a) => console.log(...a);

// Backpressure: write() returns false past highWaterMark; 'drain' follows.
const slow = new Writable({
  highWaterMark: 4,
  write(c, e, cb) {
    setImmediate(cb);
  },
});
log("w1", slow.write("ab"), slow.write("cd"), slow.writableLength, slow.writableNeedDrain);
slow.on("drain", () => {
  log("drain", slow.writableLength);
  setImmediate(() => {
    log("resume");
    r3.resume();
  });
});

// read(n) in paused mode with 'readable'.
const r = new Readable({ read() {} });
r.push("hello world");
r.push(null);
r.once("readable", () => {
  log("read(5)", String(r.read(5)), "read(1)", String(r.read(1)), "rest", String(r.read()), "after", r.read());
});

// unshift puts data back.
const r2 = new Readable({ read() {} });
r2.push("abc");
r2.once("readable", () => {
  const c = r2.read(1);
  r2.unshift(Buffer.from("Z"));
  log("unshift", String(c), String(r2.read()));
});

// pause/resume and isPaused.
const r3 = Readable.from(["p1", "p2"]);
log("paused?", r3.isPaused());
r3.pause();
log("paused", r3.isPaused(), r3.readableFlowing);
r3.on("data", (d) => log("r3", d));

// cork/uncork with writev.
const batches = [];
const wv = new Writable({
  writev(chunks, cb) {
    batches.push(chunks.map((c) => String(c.chunk)));
    cb();
  },
});
wv.cork();
wv.write("1");
wv.write("2");
wv.write("3");
process.nextTick(() => {
  wv.uncork();
  wv.end(() => log("writev", JSON.stringify(batches)));
});

// A class subclass.
class Counter extends Readable {
  constructor(max) {
    super({ objectMode: true });
    this.n = 0;
    this.max = max;
  }
  _read() {
    this.push(this.n < this.max ? { n: this.n++ } : null);
  }
}
const items = [];
new Counter(3)
  .on("data", (o) => items.push(o.n))
  .on("end", () => log("counter", items));

// util.inherits subclass with _transform (old-style packages).
function Upper(opts) {
  if (!(this instanceof Upper)) return new Upper(opts);
  Transform.call(this, opts);
}
util.inherits(Upper, Transform);
Upper.prototype._transform = function (chunk, enc, cb) {
  this.push(String(chunk).toUpperCase());
  cb();
};
let up = "";
const u = Upper();
u.on("data", (d) => (up += d));
u.on("end", () => log("upper", up, u instanceof Transform, u instanceof stream.Duplex, u instanceof Readable, u instanceof Writable));
u.write("abc");
u.end("def");

// The legacy Stream: an EventEmitter with pipe().
const legacy = new stream.Stream();
log("legacy", legacy instanceof events, typeof legacy.pipe);
const dest = new Writable({
  write(c, e, cb) {
    log("legacy got", String(c));
    cb();
  },
});
legacy.pipe(dest);
legacy.emit("data", "L1");
legacy.emit("end");

// Readable state.
const r4 = new Readable({ read() {}, highWaterMark: 2, objectMode: true });
log("push", r4.push(1), r4.push(2), r4.push(3), r4.readableLength, r4.readableHighWaterMark, r4.readableObjectMode);
log("defaults", stream.getDefaultHighWaterMark(false), stream.getDefaultHighWaterMark(true));
