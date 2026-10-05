// T86: `.length` of typed arrays and Buffers through the prototype cache (G_LEN_IC) and its guards.
const b = Buffer.from("hello");
const u = new Uint8Array(3);
let s = 0;
for (let i = 0; i < 5; i++) s += b.length + u.length;
console.log(s, b.length, u.length);
// own property shadows
const o = new Uint8Array(4);
Object.defineProperty(o, "length", { value: 99 });
console.log(o.length, u.length);
// prototype change on the instance
const w = new Uint8Array(2);
Object.setPrototypeOf(w, { length: "proto" });
console.log(w.length, u.length);
// a getter on a prototype in the chain (Buffer.prototype) after the cache filled
Object.defineProperty(Buffer.prototype, "length", { get() { return "buf-getter"; }, configurable: true });
console.log(b.length, u.length);
delete Buffer.prototype.length;
console.log(b.length);
// replacing the TypedArray getter
const TA = Object.getPrototypeOf(Uint8Array.prototype);
const d = Object.getOwnPropertyDescriptor(TA, "length");
Object.defineProperty(TA, "length", { get() { return "ta-getter"; }, configurable: true });
console.log(u.length, b.length);
Object.defineProperty(TA, "length", d);
console.log(u.length, b.length, new Float64Array(7).length);
const sub = u.subarray(1);
console.log(sub.length);

// T86: `o.length` reads and writes on plain objects (own data properties directly, the rest generic).
const st = { highWaterMark: 4, length: 0 };
for (let i = 0; i < 6; i++) { st.length += 1; if (st.length >= st.highWaterMark) st.length = 0; }
console.log(st.length, JSON.stringify(st));
const arr = [1, 2, 3, 4];
arr.length = 2;
console.log(arr, arr.length);
const acc = { get length() { return 7; }, set length(v) { this.seen = v; } };
acc.length = 5;
console.log(acc.length, acc.seen);
const viaProto = Object.create({ set length(v) { this.got = v; } });
viaProto.length = 3;
console.log(viaProto.got, Object.keys(viaProto), viaProto.length);
const frozen = Object.freeze({ length: 1 });
try { frozen.length = 9; } catch (e) { console.log(e.constructor.name, e.message); }
console.log(frozen.length);
(() => {
  "use strict";
  try { frozen.length = 2; } catch (e) { console.log(e.constructor.name, e.message); }
  const ro = {};
  Object.defineProperty(ro, "length", { value: 1, writable: false });
  try { ro.length = 2; } catch (e) { console.log(e.constructor.name, e.message); }
})();
class Q { constructor() { this.length = 0; } push() { this.length++; return this; } }
console.log(new Q().push().push().length);
const noLen = { a: 1 };
noLen.length = 4;
console.log(noLen.length, Object.keys(noLen));
const s2 = "abc";
console.log(s2.length, Object.create(arr).length);
