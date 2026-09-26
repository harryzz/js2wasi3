// Promise.all / allSettled / race, interleaved with other jobs.
const later = (v) => new Promise((res) => queueMicrotask(() => queueMicrotask(() => res(v))));

Promise.all([1, Promise.resolve(2), later(3), { then: (r) => r(4) }]).then((vs) =>
  console.log("all", vs.join(","), vs.length),
);
Promise.all([]).then((vs) => console.log("all empty", vs.length));
Promise.all([1, Promise.reject(new Error("bad")), later(3)]).catch((e) =>
  console.log("all rejected", e.message),
);
Promise.all("abc").then((vs) => console.log("all string", vs.join("|")));

Promise.allSettled([Promise.resolve(1), Promise.reject("no"), 3]).then((rs) => {
  for (const r of rs) console.log("settled", r.status, r.value, r.reason);
});

Promise.race([later("slow"), Promise.resolve("fast")]).then((v) => console.log("race", v));
Promise.race([later("slow"), Promise.reject("fast reject")]).catch((v) => console.log("race rejected", v));
Promise.race([]).then(() => console.log("never"));

const it = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next() {
        i++;
        return { value: i * 10, done: i > 3 };
      },
    };
  },
};
Promise.all(it).then((vs) => console.log("iterable", vs.join(",")));

Promise.all(5).catch((e) => console.log("not iterable", e.constructor.name, e.message));
Promise.all().catch((e) => console.log("not iterable", e.message));
Promise.allSettled({}).catch((e) => console.log("not iterable", e.message));

// an iterator whose step throws rejects; a throwing then closes the iterator
const throwing = {
  [Symbol.iterator]() {
    return {
      next() {
        throw new Error("next threw");
      },
    };
  },
};
Promise.all(throwing).catch((e) => console.log("step threw", e.message));
const closing = {
  [Symbol.iterator]() {
    return {
      next() {
        const p = Promise.resolve();
        p.then = () => {
          throw new Error("then threw");
        };
        return { value: p, done: false };
      },
      return() {
        console.log("closed");
        return {};
      },
    };
  },
};
Promise.all(closing).catch((e) => console.log("then threw", e.message));
console.log("sync done");
