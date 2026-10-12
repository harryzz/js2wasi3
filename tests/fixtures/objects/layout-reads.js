// T110: constant-key reads and writes of keys some layout has, tried inline at the layouts' slots = node.
"use strict";

class P { constructor(x, y) { this.x = x; this.y = y; } }
function Q(y, x) { this.y = y; this.x = x; }     // the same keys at other slots
const lit = (a) => ({ x: a, z: a * 2 });           // `x` at slot 0 again, `z` only here
const sum = (o) => o.x + o.y;

const objs = [new P(1, 2), new Q(3, 4), { y: 5, x: 6 }, lit(7), Object.create(new P(8, 9))];
for (const o of objs) console.log(o.x, o.y, o.z, sum(o));

// A deleted key, a key re-added at the end, an accessor and a read-only key on a layout object.
const d = new P(1, 2);
delete d.x;
console.log(d.x, d.y, "x" in d);
d.x = 10;
console.log(d.x, Object.keys(d).join());
const a = new P(1, 2);
Object.defineProperty(a, "x", { get() { return "getter " + this.y; } });
console.log(a.x, sum(a));
Object.defineProperty(a, "y", { value: 7, writable: false });
console.log(a.y, Object.isFrozen(Object.freeze(new P(3, 4))), Object.freeze(new P(3, 4)).x);

// Not plain objects at a layout site: arrays, functions, strings, numbers, Maps, class instances with extra keys.
const arr = [1, 2]; arr.x = "array x";
function fn() {} fn.x = "function x";
const m = new Map(); m.x = "map x";
for (const o of [arr, fn, "str", 42, true, m, Object.assign(new P(0, 0), { w: 1 })]) {
  try { console.log(typeof o, o.x, o.y); } catch (e) { console.log(e.constructor.name); }
}
for (const o of [null, undefined]) {
  try { console.log(o.x); } catch (e) { console.log(e.constructor.name, e.message); }
}

// A prototype key with the same name; a getter on the prototype; inherited only.
class R extends P { get z() { return "proto z " + this.x; } }
const r = new R(5, 6);
console.log(r.x, r.y, r.z, Object.create(r).x);
const proto = { x: "proto x" };
const child = Object.create(proto);
console.log(child.x, child.y);
child.y = 1;
console.log(child.x, child.y);

// A polymorphic hot loop: layout objects and dictionaries.
const many = [];
for (let i = 0; i < 40; i++) {
  if (i % 4 === 0) many.push(new P(i, -i));
  else if (i % 4 === 1) many.push(new Q(-i, i));
  else if (i % 4 === 2) { const o = {}; o.k = 1; o.y = i; o.x = -i; many.push(o); }
  else many.push(lit(i));
}
let t = 0;
for (let k = 0; k < 100; k++) for (const o of many) t += o.x + (o.y || 0) + (o.z || 0);
console.log(t);

// Objects past 8 keys (own hash index) and with many adds.
const big = new P(1, 2);
for (let i = 0; i < 20; i++) big["k" + i] = i;
console.log(big.x, big.y, big.k19);
// Iterator results and descriptors (the prelude's layouts: `value`, `done`, `get`, `enumerable`).
const it = [1, 2][Symbol.iterator]();
let step;
while (!(step = it.next()).done) console.log(step.value, step.done);
const desc = Object.getOwnPropertyDescriptor({ get g() { return 1; } }, "g");
console.log(typeof desc.get, desc.enumerable, desc.configurable, desc.value);

// Writes at layout keys: in place, read-only, accessors, frozen/sealed, non-objects, prototype setters.
const w = new P(1, 2);
w.x = 11; w.y = "twelve";
console.log(w.x, w.y, Object.keys(w).join());
const ro = new P(1, 2);
Object.defineProperty(ro, "x", { writable: false });
try { ro.x = 5; } catch (e) { console.log(e.constructor.name, ro.x); }
const ac = new P(1, 2);
Object.defineProperty(ac, "y", { set(v) { console.log("setter", v); }, get() { return "g"; } });
ac.y = 9;
console.log(ac.y);
const fr = Object.freeze(new P(1, 2));
try { fr.x = 3; } catch (e) { console.log(e.constructor.name, fr.x); }
const se = Object.seal(new P(1, 2));
se.x = 4;
try { se.z = 1; } catch (e) { console.log(e.constructor.name); }
console.log(se.x, se.z);
const deleted = new P(1, 2);
delete deleted.x;
deleted.x = "back";
console.log(Object.keys(deleted).join(), deleted.x);
const ws = new Map(); ws.x = 1; const wa = [0]; wa.x = 2;
console.log(ws.x, wa.x, wa.length);
try { (5).x = 1; } catch (e) { console.log(e.constructor.name); }
try { null.x = 1; } catch (e) { console.log(e.constructor.name); }
class S { set x(v) { console.log("proto setter", v); } }
const sp = new S();
sp.x = 1;
console.log(Object.keys(sp).length);
let cnt = 0;
const wp = new P(0, 0);
for (let i = 0; i < 1000; i++) { wp.x = wp.x + i; wp.y = i; cnt += wp.y; }
console.log(wp.x, wp.y, cnt);
