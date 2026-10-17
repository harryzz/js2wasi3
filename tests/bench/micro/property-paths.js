// T112: hit and miss cost of the property fast paths (ns per operation, the loop's own cost subtracted).
// Run under js2wasi3 builds with and without a fast path; a miss may not cost more than the old path.
"use strict";
class P { constructor(x, y) { this.x = x; this.y = y; } }        // layout [x, y]: x at slot 0
const N = 2000000;
const plain = new P(1, 2);                                       // inline hit
const other = {}; other.a = 1; other.b = 2; other.x = 3;         // x at slot 2: inline miss on a plain object
const map = new Map([[1, 2]]); map.x = 4;                        // a Map with its own x: an $object subtype
const arr = [1, 2]; arr.x = 5;                                   // an array with its own x
const str = "abc";                                               // not an object
const child = Object.create(plain);                              // x only on the prototype
const defaults = { a: 0 };                                       // T128: a default on a plain prototype
const fnDefaults = function () {}; fnDefaults.a = 0;             // the same on a function (not cached)
let sink = 0;
function time(f) { let best = Infinity; for (let r = 0; r < 5; r++) { const t0 = Date.now(); f(); best = Math.min(best, Date.now() - t0); } return best * 1e6 / N; }
const loop = time(() => { for (let i = 0; i < N; i++) sink += i & 1; });
const cases = {
  "read hit (plain, layout slot)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += plain.x; },
  "read miss plain (other slot)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += other.x; },
  "read miss proto (inherited)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += child.x; },
  "read Map own x (subtype)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += map.x; },
  "read array own x (subtype)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += arr.x; },
  "read string .x (not object)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += str.x === undefined ? 1 : 0; },
  "call Map get (subtype, key get)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += map.get(1); },
  "write hit (plain, layout slot)": () => { for (let i = 0; i < N; i++) sink += i & 1, plain.y = i; },
  "write miss plain (other slot)": () => { for (let i = 0; i < N; i++) sink += i & 1, other.x = i; },
  "write Map own x (subtype)": () => { for (let i = 0; i < N; i++) sink += i & 1, map.x = i; },
  "new P (layout_set adds)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += new P(i, i).y & 0; },
  "add absent key ({} .a =)": () => { for (let i = 0; i < N; i++) { sink += i & 1; const o = {}; o.a = i; sink += o.a & 0; } },
  "add over proto default (plain)": () => { for (let i = 0; i < N; i++) { sink += i & 1; const o = Object.create(defaults); o.a = i; sink += o.a & 0; } },
  "add over proto default (function: miss)": () => { for (let i = 0; i < N; i++) { sink += i & 1; const o = Object.create(fnDefaults); o.a = i; sink += o.a & 0; } },
  "literal {x, y} (layout_define)": () => { for (let i = 0; i < N; i++) sink += i & 1, sink += ({ x: i, y: i }).y & 0; },
};
console.log("loop", loop.toFixed(1));
for (const [name, f] of Object.entries(cases)) console.log(name, (time(f) - loop).toFixed(1));
console.log(sink > 0);
