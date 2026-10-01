// node's stream module (T60): errors, destroy, finished, pipeline failures.
import { Readable, Writable, Transform, PassThrough, pipeline, finished } from "node:stream";

const log = [];
const ev = (name, s, events) => {
  for (const e of events)
    s.on(e, (x) => log.push(`${name}:${e}${x === undefined ? "" : " " + (x instanceof Error ? x.message : String(x))}`));
};

// destroy(err) on a Readable: 'error' then 'close'.
const r1 = new Readable({ read() {} });
ev("r1", r1, ["error", "close", "end"]);
r1.destroy(new Error("boom"));
console.log("r1", r1.destroyed, r1.errored && r1.errored.message, r1.readableAborted);

// destroy() without an error: only 'close'.
const w1 = new Writable({ write(c, e, cb) { cb(); } });
ev("w1", w1, ["error", "close", "finish"]);
w1.destroy();

// A write callback error.
const w2 = new Writable({ write(c, e, cb) { cb(new Error("bad write")); } });
ev("w2", w2, ["error", "close", "finish"]);
w2.write("x", (err) => log.push("w2:cb " + (err && err.message)));

// Write after end.
const w3 = new Writable({ write(c, e, cb) { cb(); } });
ev("w3", w3, ["error", "close"]);
w3.end();
w3.write("late", (err) => log.push("w3:cb " + err.code + " " + err.message));

// Invalid chunks.
const w4 = new Writable({ write(c, e, cb) { cb(); } });
try {
  w4.write(42);
} catch (e) {
  console.log("w4", e.name, e.code, e.message);
}
const r5 = new Readable({ read() {} });
r5.on("error", (e) => console.log("r5", e.code, e.message));
r5.push(42);

// A transform error ends a pipeline; the callback sees it once, all streams destroyed.
const src = Readable.from(["a", "b", "c"]);
const bad = new Transform({
  transform(c, e, cb) {
    if (String(c) === "b") cb(new Error("no b"));
    else cb(null, c);
  },
});
const out = new PassThrough();
out.resume();
pipeline(src, bad, out, (err) => {
  log.push("pipeline " + (err && err.message) + " " + [src.destroyed, bad.destroyed, out.destroyed].join());
});

// finished on a readable that ends, and on one that errors.
const r6 = Readable.from(["1"]);
finished(r6, (err) => log.push("finished r6 " + err));
r6.resume();
const r7 = new Readable({ read() {} });
finished(r7, (err) => log.push("finished r7 " + err.message));
r7.destroy(new Error("r7 failed"));

// Premature close.
const r8 = new Readable({ read() {} });
finished(r8, (err) => log.push("finished r8 " + err.code + " " + err.message));
r8.destroy();

// Errors from the constructor's options.
for (const opts of [{ highWaterMark: -1 }, { highWaterMark: 1.5 }, { highWaterMark: "x" }]) {
  try {
    new Readable(opts);
    console.log("ok", JSON.stringify(opts));
  } catch (e) {
    console.log(e.name, e.code, e.message);
  }
}
try {
  pipeline(Readable.from([]), () => {});
} catch (e) {
  console.log(e.name, e.code, e.message);
}

setTimeout(() => {
  for (const l of log) console.log(l);
}, 20);
