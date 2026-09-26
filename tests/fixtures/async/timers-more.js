// Timeout objects, clearing by id, refresh, unref, setImmediate and process.nextTick
// (T25 slice 6, F74). Delays are far apart: node's order of timers due together is racy.
console.log(typeof process.nextTick, process.nextTick.name, process.nextTick.length);
console.log(setImmediate.length, clearImmediate.length, setImmediate.name);

// Ordering after the main module (an ES module): microtasks first, then the ticks.
process.nextTick(
  (a, b) => {
    console.log("tick1", a, b);
    process.nextTick(() => console.log("tick nested"));
    Promise.resolve().then(() => console.log("micro from tick"));
  },
  1,
  2,
);
Promise.resolve().then(() => {
  console.log("micro1");
  process.nextTick(() => console.log("tick from micro1"));
});
queueMicrotask(() => console.log("qm"));

// Timeout methods and ids.
const t = setTimeout(() => console.log("t fired"), 10);
console.log(t.hasRef(), t.unref() === t, t.hasRef(), t.ref() === t, t.hasRef());
console.log(typeof t[Symbol.toPrimitive], Number.isInteger(+t), `${t}` === String(+t), +t === +t);
console.log(typeof t.refresh, Object.prototype.toString.call(t));
const c = setTimeout(() => console.log("never c"), 5);
clearTimeout(+c);
const d = setTimeout(() => console.log("never d"), 5);
clearInterval(String(+d));
const e = setTimeout(() => console.log("never e"), 5);
clearTimeout(e);
console.log(e.refresh() === e);
setTimeout(() => console.log("unref'd: never"), 500).unref();
setInterval(() => console.log("unref'd interval: never"), 400).unref();

// refresh: pushes the deadline; after firing it runs again.
let rc = 0;
const r = setTimeout(function () {
  console.log("r fired", ++rc, this === r);
  if (rc === 1) setTimeout(() => r.refresh(), 10);
}, 40);
setTimeout(() => r.refresh(), 20);
setTimeout(() => console.log("50: before r"), 50);

// Immediates.
const im = setImmediate(
  function (a, b) {
    console.log("imm1", a, b, this === im);
    setImmediate(() => console.log("imm3 (next pass)"));
    process.nextTick(() => console.log("tick in imm1"));
    Promise.resolve().then(() => console.log("micro in imm1"));
    clearImmediate(im4);
  },
  "x",
  "y",
);
console.log(typeof im, im.hasRef(), im.unref().hasRef(), im.ref().hasRef(), typeof im.refresh);
console.log(typeof im[Symbol.toPrimitive]);
setImmediate(() => console.log("imm2"));
const cleared = setImmediate(() => console.log("never cleared imm"));
clearImmediate(cleared);
const im4 = setImmediate(() => console.log("never im4"));

// After a timer callback: ticks, microtasks, then the immediates before the next timers.
setTimeout(() => {
  console.log("timeout A");
  setTimeout(() => console.log("timeout from A"), 0);
  setImmediate(() => console.log("imm from A"));
  process.nextTick(() => console.log("tick from A"));
  Promise.resolve().then(() => console.log("micro from A"));
}, 120);
console.log("sync end");
