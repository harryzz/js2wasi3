// TypeErrors of the Promise API, and values that aren't promises.
const t = (f) => {
  try {
    f();
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
};
t(() => Promise.prototype.then.call(1));
t(() => Promise.prototype.then.call({}));
t(() => Promise.prototype.then.call([1]));
t(() => Promise.prototype.finally.call(1));
t(() => Promise.prototype.finally.call({}));
t(() => Promise.prototype.catch.call({}));
t(() => Promise());
t(() => new Promise(1));
t(() => new Promise({}));
t(() => new Promise());
t(() => Promise.resolve.call(1, 2));
t(() => Promise.all.call(1, []));
t(() => queueMicrotask(1));
t(() => queueMicrotask());
t(() => queueMicrotask({}));
t(() => queueMicrotask("s"));
t(() => queueMicrotask(null));
t(() => queueMicrotask([]));
t(() => queueMicrotask(Object.create(null)));
t(() => queueMicrotask(new (class Foo {})()));

console.log(Promise.name, Promise.length, typeof Promise.resolve, Promise.prototype.then.length);
console.log(Promise.prototype.constructor === Promise, Promise.resolve.name, queueMicrotask.name);
const p = Promise.resolve(1);
const r1 = Promise.reject(1);
const r2 = Promise.reject(1);
console.log(Promise.resolve(p) === p, r1 === r2);
r1.catch(() => {});
r2.catch(() => {});
Promise.resolve(p).catch(() => {});
const q = new Promise(() => {});
console.log(q.then() instanceof Promise, q.then() === q);

// a subclass instance made by `new` gets the subclass prototype
class MyPromise extends Promise {}
const m = new MyPromise((res) => res(3));
console.log(m instanceof MyPromise, m instanceof Promise);
m.then((v) => console.log("subclass value", v));

// handled later in the same drain: not an unhandled rejection
const late = Promise.reject(new Error("late"));
queueMicrotask(() => late.catch((e) => console.log("handled late", e.message)));
