// `yield*` in an async generator (T29).
async function* inner() {
  const a = yield 1;
  console.log("inner got", a);
  yield Promise.resolve(2);
  return "inner done";
}
async function* outer() {
  const r = yield* inner();
  console.log("yield* value", r);
  yield* [3, Promise.resolve(4)]; // a sync iterable: values awaited
  yield* "ab";
}
const log = [];
for await (const v of outer()) log.push(v);
console.log(log);

// next(x) forwarding and the job order against other promise jobs.
const g = outer();
Promise.resolve().then(() => console.log("tick 1")).then(() => console.log("tick 2"));
console.log(await g.next(), await g.next("sent"), await g.next());

// return() forwarding: the inner iterator's `return` runs (and a `finally`).
async function* withFinally() {
  try {
    yield "a";
    yield "b";
  } finally {
    console.log("inner finally");
  }
}
async function* wrap(it) {
  try {
    return yield* it;
  } finally {
    console.log("outer finally");
  }
}
const w = wrap(withFinally());
console.log(await w.next(), await w.return("early"), await w.next());

// throw() forwarding, caught inside the inner generator.
async function* catcher() {
  try {
    yield 1;
  } catch (e) {
    console.log("caught", e);
    yield "recovered";
  }
}
const t = wrap(catcher());
console.log(await t.next(), await t.throw("boom"), await t.next());

// An inner iterator without `throw`: closed, then a TypeError.
let closed = 0;
const noThrow = {
  [Symbol.asyncIterator]() {
    return {
      next() { return Promise.resolve({ value: "x", done: false }); },
      return() { closed++; return Promise.resolve({ done: true }); },
    };
  },
};
const n = wrap(noThrow);
await n.next();
try {
  await n.throw(new Error("no"));
} catch (e) {
  console.log(e.constructor.name, e.message, closed);
}

// A sync iterator without `return`: return(v) awaits v.
const s = wrap([1, 2]);
console.log(await s.next(), await s.return(Promise.resolve("r")), await s.next());

// Errors: not iterable, a result that isn't an object.
async function* bad(v) { yield* v; }
for (const v of [1, { [Symbol.asyncIterator]() { return { next() { return 5; } }; } }]) {
  try {
    await bad(v).next();
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
}

// Job order: a delegating, a direct and a sync-wrapped generator consumed side by side.
const order = [];
async function* src() { yield 1; yield 2; }
async function* viaAsync() { yield* src(); }
async function* direct() { yield 1; yield 2; }
async function* viaSync() { yield* [1, 2]; }
const runs = [
  (async () => { for await (const v of viaAsync()) order.push("a" + v); })(),
  (async () => { for await (const v of direct()) order.push("d" + v); })(),
  (async () => { for await (const v of viaSync()) order.push("s" + v); })(),
];
let ticks = 0;
const tick = () => { order.push("t" + ticks); if (++ticks < 12) return Promise.resolve().then(tick); };
tick();
await Promise.all(runs);
console.log(order.join(" "));
