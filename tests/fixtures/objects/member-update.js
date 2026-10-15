// T119: `o.k++` / `--o.k` with a constant key (named paths) on every kind of receiver and value.
"use strict";

// Prefix and postfix, both directions; the result is the old or new number.
const o = { a: 1, b: 2.5, c: "7", d: null, e: undefined, f: true, g: [3], h: {} };
console.log(o.a++, o.a, ++o.a, o.a, o.a--, o.a, --o.a, o.a);
console.log(o.b++, o.b, --o.b, o.b);
// Non-numbers convert first: the old value is the number, not the string.
console.log(o.c++, o.c, typeof o.c);
console.log(o.d++, o.d, o.e++, o.e, o.f--, o.f, o.g++, o.g, o.h++, o.h);
// A missing key: NaN, and the key is added.
const m = {};
console.log(m.x++, m.x, "x" in m);
// Large and fractional numbers, -0, overflow of the integer range.
const n = { big: 2 ** 31 - 1, neg: -(2 ** 30), frac: 0.5, z: -0, max: Number.MAX_SAFE_INTEGER };
n.big++; n.neg--; n.frac++; n.z++; n.max++;
console.log(n.big, n.neg, n.frac, n.z, n.max);
const nz = { z: 1 };
--nz.z;
console.log(Object.is(nz.z, 0), Object.is(-nz.z, -0));

// Constructor objects (layouts, number cells) in a hot loop.
class P {
  constructor() { this.count = 0; this.f = 0.25; this.s = 1; }
  tick() { this.count++; this.f++; --this.s; return this.count; }
}
const p = new P();
for (let i = 0; i < 1000; i++) p.tick();
console.log(p.count, p.f, p.s);
const objs = [new P(), { count: 5 }, Object.create({ count: 10 })];
for (const q of objs) q.count++;
console.log(objs.map(q => q.count), Object.keys(objs[2]));

// Getters and setters run once each, in order.
const log = [];
const acc = {
  _v: 1,
  get v() { log.push("get"); return this._v; },
  set v(x) { log.push("set " + x); this._v = x; },
};
console.log(acc.v++, ++acc.v, acc._v, log.join(","));
// A setter on the prototype, a getter only (strict: TypeError), read-only, frozen.
const base = { set w(x) { log.push("proto set " + x); } };
const child = Object.create(base);
child.w++;
console.log(log.at(-1), Object.keys(child));
for (const [name, target] of [
  ["getter only", { get r() { return 1; } }],
  ["read-only", Object.defineProperty({}, "r", { value: 1, writable: false })],
  ["frozen", Object.freeze({ r: 1 })],
  ["non-extensible", Object.preventExtensions({})],
]) {
  try { target.r++; console.log(name, "no error", target.r); } catch (e) { console.log(name, e.constructor.name); }
}

// Other receivers: arrays (`length`), functions, Maps, strings, null.
const arr = [1, 2];
arr.length++;
console.log(arr.length, arr);
function fn() {}
fn.calls = 0;
fn.calls++;
console.log(fn.calls);
const map = new Map();
map.size2 = 1;
map.size2++;
console.log(map.size2);
try { "abc".length++; } catch (e) { console.log("string", e.constructor.name); }
try { const z = null; z.k++; } catch (e) { console.log("null", e.constructor.name); }
let side = 0;
function obj() { side++; return o; }
obj().a++;
console.log(side, o.a);

// Private and super members.
class Q {
  #n = 1;
  static #s = 5;
  bump() { this.#n++; return ++this.#n; }
  static sbump() { return Q.#s--; }
}
console.log(new Q().bump(), Q.sbump(), Q.sbump());
class R { constructor() { this.k = 1; } }
class S extends R { inc() { return super.k++; } }
const s = new S();
console.log(s.inc(), s.k);

// BigInt values keep their kind.
const bi = { v: 1n };
console.log(bi.v++, bi.v, --bi.v, typeof bi.v);
