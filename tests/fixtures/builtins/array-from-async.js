// Array.fromAsync and Array.from with a constructor `this` (T35 slice 4).
const show = (p) => p.then((v) => console.log("ok", v), (e) => console.log(e.constructor.name + ": " + e.message));
await show(Array.fromAsync([1, Promise.resolve(2), 3]));
await show(Array.fromAsync({ length: 2, 0: Promise.resolve("a"), 1: "b" }));
async function* g() { yield 1; yield 2; }
await show(Array.fromAsync(g(), async (x, i) => x * 10 + i));
await show(Array.fromAsync(null));
await show(Array.fromAsync([1], 5));
await show(Array.fromAsync("hi"));
function C() { this.made = true; }
await show(Array.fromAsync.call(C, [1, 2]));
await show(Array.fromAsync.call(C, { length: 1, 0: "x" }));
console.log(Array.from.call(C, [1, 2]), Array.from.call(Object, { length: 1, 0: 1 }).constructor === Object);
let order = [];
const p = Array.fromAsync([1, 2]); order.push("sync"); await p; order.push("done"); console.log(order.join());
