// T111: number properties kept unboxed in their slots (a number cell, written in place). Every way a value
// leaves an object must see the number, never the cell; non-number writes, attributes and getters replace or
// guard it.
"use strict";

function P(x, y) {
  this.x = x;
  this.y = y;
  this.n = 0;
}

function make() {
  const p = new P(1, 2);
  p.x = p.x * 1.5;      // a numeric write: the slot gets a cell
  p.y -= 0.25;          // compound
  p.n += 0.5;           // `o.k += x`
  return p;
}

let p = make();
console.log(p.x, p.y, p.n, typeof p.x, typeof p.n);

// Reads of every kind.
console.log(JSON.stringify(p));
console.log(Object.keys(p), Object.values(p), Object.entries(p));
console.log({ ...p }, Object.assign({}, p));
for (const k in p) console.log(k, p[k]);
console.log(p["x"], p[["y"][0]], "x" in p, p.hasOwnProperty("n"));
console.log(Object.getOwnPropertyDescriptor(p, "x"));
console.log(p);
console.log(String(p.x), `${p.y}`, p.x + "", p.x === 1.5, Object.is(p.y, 1.75));
const { x, y, n } = p;
console.log(x, y, n);

// A read must be a copy: a later write doesn't change what was read.
const before = p.x;
const q = { v: 0 };
q.v = p.x;
p.x = p.x + 10;
console.log(before, q.v, p.x);

// In-place writes many times, then integers and special values.
for (let i = 0; i < 1000; i++) p.n += 0.001;
console.log(p.n.toFixed(6));
p.n = p.n * 0;          // 0
console.log(p.n, Object.is(p.n, 0));
p.n = -1 * 0;           // -0
console.log(Object.is(p.n, -0), 1 / p.n);
p.n = 0 / 0;            // NaN
console.log(p.n, Number.isNaN(p.n));
p.n = 1 / 0;
console.log(p.n);
p.n = 2 ** 40;
console.log(p.n);

// A non-number write replaces the cell; a number write after it works again.
p.y = "str";
console.log(p.y, typeof p.y);
p.y = p.x * 2;
console.log(p.y);
p.y += "!";
console.log(p.y);

// Shared reads: two objects never share a cell.
const a = make(), b = make();
a.x = a.x * 3;
console.log(a.x, b.x);
b.x = a.x;
a.x = a.x + 1;
console.log(a.x, b.x);

// Attributes: read-only, frozen, defineProperty with the same value, a getter replacing the slot.
const r = make();
Object.defineProperty(r, "x", { writable: false });
try {
  r.x = r.x * 2;
} catch (e) {
  console.log(e.constructor.name, r.x);
}
Object.defineProperty(r, "x", { value: 1.5 });
console.log(r.x, Object.getOwnPropertyDescriptor(r, "x"));
try {
  Object.defineProperty(r, "x", { value: 2.5 });
} catch (e) {
  console.log(e.constructor.name);
}
const f = Object.freeze(make());
try {
  f.y -= 1;
} catch (e) {
  console.log(e.constructor.name, f.y);
}
const g = make();
let hits = 0;
Object.defineProperty(g, "n", {
  get() { hits++; return 4.5; },
  set(v) { hits++; this.y = v; },
});
g.n += 1;
g.n = g.n * 2;
console.log(g.n, g.y, hits);

// delete, then the key again.
const d = make();
delete d.y;
console.log(d.y, "y" in d);
d.y = d.x * 2;
console.log(d.y, Object.keys(d));

// Prototype reads: a cell on the prototype.
const proto = make();
const child = Object.create(proto);
console.log(child.x, child.y);
child.x = child.x * 2;
console.log(child.x, proto.x, Object.keys(child));

// Classes and literals with numeric writes.
class V {
  constructor(a) { this.a = a; this.b = 0; }
  step(dt) { this.b += this.a * dt; this.a -= dt; return this; }
}
const v = new V(2.5);
for (let i = 0; i < 4; i++) v.step(0.125);
console.log(v.a, v.b, JSON.stringify(v));
const lit = { m: 1, k: 2 };
lit.m = lit.k / 3;
lit.k *= 1.5;
console.log(lit, lit.m + lit.k);

// Map/array receivers of the same key names take the generic paths.
const arr = [1, 2];
arr.x = 1;
arr.x = arr.x * 2.5;
const fn = function () {};
fn.y = 1;
fn.y -= 0.5;
console.log(arr.x, fn.y);

// Numbers as keys of other operations.
const s = make();
s.x = s.x / 4;
console.log([10, 20, 30][s.x * 0], Math.max(s.x, s.y), s.x < s.y, s.x == "0.375");
console.log(JSON.parse(JSON.stringify(s)).x);
