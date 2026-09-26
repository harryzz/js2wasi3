// Promise.any, Promise.withResolvers and AggregateError (T25 slice 6, F64).
{
  const { promise, resolve, reject } = Promise.withResolvers();
  promise.then((v) => console.log("withResolvers", v));
  resolve(5);
  resolve(6);
  console.log(Object.keys(Promise.withResolvers()), typeof reject);
  const r = Promise.withResolvers();
  r.promise.catch((e) => console.log("rejected", e));
  r.reject("no");
}
Promise.any([Promise.reject(1), Promise.resolve(2), 3]).then((v) => console.log("any", v));
Promise.any([Promise.reject(1), new Promise((r) => setTimeout(() => r("late"), 1))]).then((v) =>
  console.log("any late", v),
);
Promise.any([Promise.reject(1), Promise.reject(new Error("x"))]).catch((e) => {
  console.log(e instanceof AggregateError, e instanceof Error, e.name, e.message);
  console.log(e.errors.length, e.errors[0], e.errors[1].message);
  console.log(String(e), Object.prototype.toString.call(e));
  console.log(e.stack);
});
Promise.any([]).catch((e) => console.log("empty", e.message, e.errors));
Promise.any(5).catch((e) => console.log(e.message));
Promise.any(new Set([Promise.reject("s")])).catch((e) => console.log(e, [e], { e }));
Promise.any(Array.from({ length: 30 }, (_, i) => Promise.reject("reason number " + i))).catch((e) =>
  console.log(e),
);
console.log(Promise.any.length, Promise.withResolvers.length, Promise.any.name);
{
  const a = new AggregateError([1, 2], "msg");
  console.log(a.message, a.errors, a.name, typeof AggregateError, AggregateError.name);
  console.log(AggregateError.length, a instanceof AggregateError, a instanceof Error, String(a));
  console.log(new AggregateError(new Set([3])).errors, new AggregateError([]).message === "");
  try {
    new AggregateError(5);
  } catch (e) {
    console.log(e.name, e.message);
  }
  const b = AggregateError(["x"], "m2", { cause: 7 });
  console.log(b.cause, b.errors, Object.keys(b));
  class E2 extends AggregateError {}
  console.log(new E2([1]).errors, new E2([]) instanceof E2, new E2([]) instanceof AggregateError);
}
