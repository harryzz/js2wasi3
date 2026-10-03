// T68 (F295): `Error.captureStackTrace(o, fn)` gives `o` an own `stack` accessor (get/set, not
// enumerable, configurable), made on its first read: `Error.prototype.toString`'s steps on `o`
// and frames (js2wasi3 has one, `    at <anonymous>`; `fn` hides nothing).
console.log(typeof Error.captureStackTrace, Error.captureStackTrace.length, Error.captureStackTrace.name);
const o = { a: 1 };
console.log(Error.captureStackTrace(o));
const d = Object.getOwnPropertyDescriptor(o, "stack");
console.log(typeof d.get, typeof d.set, d.enumerable, d.configurable, Object.keys(o), "stack" in o);
console.log(o.stack.split("\n")[0], /^\s+at /.test(o.stack.split("\n")[1]));
console.log(o, JSON.stringify(o));
o.stack = "custom";
console.log(o.stack);
// zigbee-herdsman's `TimeoutError`-style class.
class TimeoutError extends Error {
  constructor(message) {
    super(message);
    this.name = "TimeoutError";
    Error.captureStackTrace(this, TimeoutError);
  }
}
const t = new TimeoutError("Timeout - 6000ms");
console.log(t.stack.split("\n")[0], t instanceof TimeoutError, t instanceof Error, String(t));
console.log(Object.keys(t), Object.getOwnPropertyNames(t));
try {
  throw new TimeoutError("thrown");
} catch (err) {
  console.log(err.name, err.message, err.stack.split("\n")[0]);
}
// The header is made on the first read, from `name` and `message` then (not the object's toString).
const p = { name: "N", message: "M", toString() { return "ignored"; } };
Error.captureStackTrace(p, function unused() {});
p.name = "Late";
console.log(p.stack.split("\n")[0]);
const q = { name: 1, message: {} };
Error.captureStackTrace(q);
console.log(q.stack.split("\n")[0]);
const e = new Error("before");
e.stack;
Error.captureStackTrace(e);
e.message = "after";
console.log(e.stack.split("\n")[0]);
const empty = { name: "" , message: "only message" };
Error.captureStackTrace(empty);
console.log(empty.stack.split("\n")[0]);
const f = function () {};
Error.captureStackTrace(f);
console.log(typeof f.stack, f.stack.split("\n")[0]);
for (const v of [1, "s", null, undefined, Symbol("x")]) {
  try {
    Error.captureStackTrace(v);
  } catch (err) {
    console.log(err.constructor.name, err.message);
  }
}
for (const v of [Object.freeze({}), Object.preventExtensions({})]) {
  try {
    Error.captureStackTrace(v);
  } catch (err) {
    console.log(err.constructor.name, err.message);
  }
}
// As a value: feature tests like `if (Error.captureStackTrace)`.
const capture = Error.captureStackTrace;
if (Error.captureStackTrace) capture(o);
console.log(o.stack.split("\n")[0]);
