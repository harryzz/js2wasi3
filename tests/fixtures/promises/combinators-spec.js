// Promise statics per the spec for any constructor `this` (T50).

// A subclass: the results are instances of it, and `C.resolve` is called per element.
class Sub extends Promise {
  static resolve(v) {
    console.log("Sub.resolve", v);
    return super.resolve(v);
  }
}
const all = Sub.all([1, Promise.resolve(2)]);
console.log(all instanceof Sub, all.constructor === Sub);
all.then((v) => console.log("Sub.all", v));

// A custom constructor: the executor's resolve/reject are the capability.
function Custom(executor) {
  executor(
    (v) => console.log("Custom resolved", JSON.stringify(v)),
    (e) => console.log("Custom rejected", e.message),
  );
}
Custom.resolve = (v) => ({ then(onF, onR) { console.log("then", typeof onF, onF.length, JSON.stringify(onF.name)); onF(v * 10); } });
Promise.all.call(Custom, [1, 2]);
Promise.allSettled.call(Custom, [3]);
Promise.race.call(Custom, [4]);
Promise.any.call(Custom, [5]);

// `C.resolve` is read once, before iterating.
let reads = 0;
const Counted = function (ex) { return new Promise(ex); };
Object.defineProperty(Counted, "resolve", { get() { reads++; return Promise.resolve.bind(Promise); } });
Promise.all.call(Counted, [1, 2, 3]).then((v) => console.log("Counted", v, reads));

// IteratorClose when `C.resolve` throws; not when `next` throws.
const iter = (throwIn) => ({
  [Symbol.iterator]() {
    return {
      next() {
        if (throwIn === "next") throw new Error("next");
        return { value: 1, done: false };
      },
      return() {
        console.log("return called");
        return {};
      },
    };
  },
});
const Bad = function (ex) { return new Promise(ex); };
Bad.resolve = () => { throw new Error("resolve"); };
Promise.race.call(Bad, iter("resolve")).catch((e) => console.log("race rejected", e.message));
Promise.all(iter("next")).catch((e) => console.log("all rejected", e.message));

// Not a constructor, not an object, executor misuse.
const errors = [
  () => Promise.all.call(1, []),
  () => Promise.all.call({}, []),
  () => Promise.all.call(parseInt, []),
  () => Promise.race.call(function (ex) { ex(1, 2); ex(3, 4); }, []),
  () => Promise.resolve.call(undefined, 1),
  () => Promise.reject.call("x", 1),
  () => Promise.withResolvers.call(null),
  () => Promise.try.call(0, () => 1),
];
for (const f of errors) {
  try {
    f();
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
}

// Rejections of the result for bad iterables.
for (const v of [undefined, 3, {}, { [Symbol.iterator]() { return 1; } }]) {
  Promise.allSettled(v).catch((e) => console.log("allSettled", e.message));
}
Promise.any([]).catch((e) => console.log(e.constructor.name, e.message, e.errors));
Promise.any([Promise.reject(1), Promise.reject(2)]).catch((e) => console.log("any", e.errors));
Promise.allSettled([1, Promise.reject(2)]).then((v) => console.log(JSON.stringify(v)));

// `resolve`/`reject`/`withResolvers`/`try` with a subclass.
const rejected = Sub.reject(1);
rejected.catch(() => {});
console.log(rejected instanceof Sub, Sub.withResolvers().promise instanceof Sub);
const p = Promise.resolve(7);
console.log(Promise.resolve(p) === p, Sub.resolve(p) === p);
Promise.try((a, b) => a + b, 2, 3).then((v) => console.log("try", v));
Promise.try(() => { throw new Error("boom"); }).catch((e) => console.log("try rejected", e.message));
Promise.try(1).catch((e) => console.log("try", e.message));
console.log(Object.getOwnPropertyNames(Promise).join(), Promise.try.length, Promise.all.name);
console.log(Promise.all === Promise.all, Object.getOwnPropertyDescriptor(Promise, "all").value === Promise.all);
