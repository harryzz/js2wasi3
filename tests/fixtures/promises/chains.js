// Resolution with promises and thenables (extra ticks), chains, finally.

const a = Promise.resolve(1);
const b = new Promise((res) => res(a)); // resolved with a promise: 2 extra ticks
b.then((v) => console.log("b", v));
a.then((v) => console.log("a", v)).then(() => console.log("a2")).then(() => console.log("a3")).then(() => console.log("a4"));

const thenable = {
  then(res, rej) {
    console.log("thenable.then called");
    res("from thenable");
  },
};
Promise.resolve(thenable).then((v) => console.log("thenable", v));
new Promise((res) => res(thenable)).then((v) => console.log("thenable2", v));

// then returning a promise
Promise.resolve(10)
  .then((v) => Promise.resolve(v * 2))
  .then((v) => console.log("returned promise", v));

// throw in a handler rejects the derived promise
Promise.resolve()
  .then(() => {
    throw new TypeError("oops");
  })
  .then(() => console.log("not reached"))
  .catch((e) => console.log("caught", e.name, e.message))
  .then((v) => console.log("after catch", v));

// finally passes values and reasons through
Promise.resolve("keep")
  .finally(() => console.log("finally 1"))
  .then((v) => console.log("finally value", v));
Promise.reject(new Error("r"))
  .finally(() => console.log("finally 2"))
  .catch((e) => console.log("finally reason", e.message));
Promise.resolve(1)
  .finally(() => {
    throw new RangeError("from finally");
  })
  .catch((e) => console.log("finally threw", e.message));
Promise.resolve(1)
  .finally(() => Promise.reject(2))
  .catch((e) => console.log("finally rejected", e));

// non-function handlers pass through
Promise.resolve(5).then(null, undefined).then(7).then((v) => console.log("passthrough", v));
Promise.reject(6).then((v) => console.log("no")).then(undefined, (e) => console.log("reason passthrough", e));

// executor throws / resolves twice
new Promise(() => {
  throw new Error("executor");
}).catch((e) => console.log("executor threw", e.message));
new Promise((res, rej) => {
  res("first");
  rej("second");
  res("third");
}).then((v) => console.log("settled once", v));
new Promise((res, rej) => {
  res("ok");
  throw new Error("ignored");
}).then((v) => console.log("throw after resolve", v));

// self-resolution
const self = new Promise((res) => setTimeoutLess(() => res(self)));
function setTimeoutLess(f) {
  queueMicrotask(f);
}
self.catch((e) => console.log("cycle", e.constructor.name, e.message));

// the same promise, several handlers, in order
const shared = Promise.resolve("s");
shared.then(() => console.log("h1"));
shared.then(() => console.log("h2"));
shared.then(() => console.log("h3"));

console.log("sync done", typeof Promise, a instanceof Promise, Object.getPrototypeOf(a) === Promise.prototype);
