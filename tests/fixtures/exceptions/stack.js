// T63 (F261): `stack` is a string: V8's header (Error.prototype.toString at construction) and frames
// (js2wasi3 has one, `    at <anonymous>`); Promise.any's error has none.
const e = new Error("boom");
console.log("stack" in e, Object.keys(e), Object.getOwnPropertyNames(e));
console.log(typeof e.stack, e.stack.split("\n")[0]);
console.log(new TypeError().stack.split("\n")[0], new RangeError(42).stack.split("\n")[0]);
class MyErr extends Error {}
console.log(new MyErr("m").stack.split("\n")[0], /^\s+at /.test(new MyErr("m").stack.split("\n")[1]));
try { null.x; } catch (err) { console.log(err.stack.split("\n")[0]); }
try { undefinedName; } catch (err) { console.log(err.stack.split("\n")[0]); }
console.log(new Error("c", { cause: 1 }).stack.split("\n")[0]);
console.log(Object.getOwnPropertyDescriptor(e, "stack").enumerable);
Promise.any([Promise.reject(1)]).catch((a) => { console.log(a); console.log(a.stack); });
console.log(new AggregateError([1], "agg").stack.split("\n")[0]);
const named = new Error("x");
named.name = "Custom";
console.log(named.stack.split("\n")[0], String(named));
const lines = new Error("a\nb").stack.split("\n");
console.log(lines[0], lines[1], lines[2].startsWith("    at "));
// Made on the first read, as V8 does: a `name` set before it counts, after it doesn't.
class Named extends Error { constructor(m) { super(m); this.name = "Named"; } }
console.log(new Named("n").stack.split("\n")[0]);
const late = new Error("late");
late.name = "Late";
console.log(late.stack.split("\n")[0]);
late.name = "Again";
console.log(late.stack.split("\n")[0], String(late));
const w = new Error("w");
w.stack = "custom";
console.log(w.stack);
const fz = Object.freeze(new Error("fz"));
console.log(fz.stack.split("\n")[0]);
