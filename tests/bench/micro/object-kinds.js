// T113: generic object paths per receiver kind (ns per operation, the loop's own cost subtracted): the paths a
// change to how objects are told apart (root R1) touches. Plain objects, arrays, functions and Maps each.
"use strict";
const N = 1000000;
const plain = { a: 1, b: 2, c: 3 };
const arr = [1, 2, 3]; arr.a = 1;
function fn() {} fn.a = 1;
const map = new Map([[1, 2]]); map.a = 1;
const kinds = { plain, array: arr, function: fn, Map: map };
let sink = 0;
function time(f) { let best = Infinity; for (let r = 0; r < 5; r++) { const t0 = Date.now(); f(); best = Math.min(best, Date.now() - t0); } return best * 1e6 / N; }
const loop = time(() => { for (let i = 0; i < N; i++) sink += i & 1; });
const k = "a";
const ops = {
  "o[k] (computed key)": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += o[k] === 1 ? 1 : 0; },
  "o[k] = v": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, o[k] = 1; },
  "k in o": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += k in o ? 1 : 0; },
  "o.hasOwnProperty(k)": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += o.hasOwnProperty(k) ? 1 : 0; },
  "Object.keys(o).length": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += Object.keys(o).length; },
  "o instanceof Object": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += o instanceof Object ? 1 : 0; },
  "typeof o": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += typeof o === "object" ? 1 : 0; },
  "Object.getPrototypeOf(o)": (o) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += Object.getPrototypeOf(o) ? 1 : 0; },
};
console.log("loop", loop.toFixed(1));
for (const [op, mk] of Object.entries(ops))
  for (const [kind, o] of Object.entries(kinds)) console.log(`${op} [${kind}]`, (time(mk(o)) - loop).toFixed(1));
console.log(sink > 0);
