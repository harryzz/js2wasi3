// T113: the object test on primitives (ns per operation, the loop's own cost subtracted): a change to how
// objects are told apart may not make a primitive dearer. Each kind of value, through `==`, `typeof` and a
// BigInt/number `+` (paths that ask "is it an object?" first).
"use strict";
const N = 1000000;
const values = {
  null: null, undefined: undefined, boolean: true, "small int": 7, double: 1.5, string: "abc",
  symbol: Symbol("s"), bigint: 12345678901234567890n, plain: { a: 1 },
};
let sink = 0;
function time(f) { let best = Infinity; for (let r = 0; r < 5; r++) { const t0 = Date.now(); f(); best = Math.min(best, Date.now() - t0); } return best * 1e6 / N; }
const loop = time(() => { for (let i = 0; i < N; i++) sink += i & 1; });
const ops = {
  "v == null": (v) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += v == null ? 1 : 0; },
  "v == 1": (v) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += v == 1 ? 1 : 0; },
  "typeof v": (v) => () => { for (let i = 0; i < N; i++) sink += i & 1, sink += typeof v === "object" ? 1 : 0; },
};
console.log("loop", loop.toFixed(1));
for (const [op, mk] of Object.entries(ops))
  for (const [kind, v] of Object.entries(values)) console.log(`${op} [${kind}]`, (time(mk(v)) - loop).toFixed(1));
let b = 1n;
console.log("bigint a + b", (time(() => { for (let i = 0; i < N; i++) sink += i & 1, b = b + 1n; }) - loop).toFixed(1));
console.log("bigint a * b", (time(() => { let c = 3n; for (let i = 0; i < N; i++) sink += i & 1, c = (c * 3n) % 1000003n; }) - loop).toFixed(1));
console.log(sink > 0, b > 0n);
