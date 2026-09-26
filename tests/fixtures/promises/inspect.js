// console.log of promises in each state.
const r = Promise.reject(2);
r.catch(() => {});
const p = Promise.resolve({ a: 1, b: [1, 2] });
p.x = 5;
console.log(Promise.resolve(1), new Promise(() => {}), r, p);
console.log([Promise.resolve("s")], { p: Promise.resolve(undefined) }, { d: { e: { f: Promise.resolve(1) } } });
const q = new Promise((res) => queueMicrotask(() => res("later")));
console.log(q);
q.then(() => console.log(q));
console.log(Promise, typeof Promise.prototype.then);
