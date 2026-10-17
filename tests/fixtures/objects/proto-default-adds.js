// T128: adds of keys that the prototype holds as writable data (prototype defaults): the write site caches the
// chain, and every later change on the prototype must still be seen.
"use strict";
function P() {}
P.prototype = { x: 0, y: 0, label: "p" };
function make(x, y) {
  const o = Object.create(P.prototype);
  o.x = x;
  o.y = y;
  return o;
}
const a = [];
for (let i = 0; i < 5; i++) a.push(make(i, i * 2));
console.log(a.map((o) => `${o.x},${o.y},${o.label}`).join(" "), Object.keys(a[4]));

// The default becomes read-only: the add throws (strict mode).
Object.defineProperty(P.prototype, "x", { writable: false });
try {
  make(7, 8);
  console.log("no throw");
} catch (e) {
  console.log(e.constructor.name);
}
Object.defineProperty(P.prototype, "x", { writable: true });
console.log(make(9, 10).x);

// The default becomes an accessor: the setter runs, no own property.
let seen = [];
Object.defineProperty(P.prototype, "y", { set(v) { seen.push(v); }, get() { return -1; }, configurable: true });
const b = make(1, 2);
console.log(seen, Object.keys(b), b.y);

// Deleted from the prototype: a setter further up the chain now runs.
Object.defineProperty(P.prototype, "y", { value: 0, writable: true, configurable: true });
console.log(Object.keys(make(3, 4)));
const up = Object.getPrototypeOf(P.prototype);
let upSeen = 0;
Object.defineProperty(up, "y", { set(v) { upSeen += v; }, configurable: true });
delete P.prototype.y;
const c = make(5, 6);
console.log(upSeen, Object.keys(c));
delete up.y;

// Many keys added to and deleted from the prototype (entries move): still right.
for (let i = 0; i < 40; i++) P.prototype["k" + i] = i;
for (let i = 0; i < 40; i++) delete P.prototype["k" + i];
P.prototype.y = 0;
console.log(Object.keys(make(11, 12)));

// Frozen prototype: adds of its keys throw.
Object.freeze(P.prototype);
try {
  make(1, 1);
  console.log("no throw");
} catch (e) {
  console.log(e.constructor.name);
}

// Another prototype at the same site, and a non-plain holder (a function's prototype chain).
function Q() {}
Q.prototype.x = 1;
Q.prototype.y = 2;
function setXY(o) { o.x = 5; o.y = 6; return o; }
const qs = [];
for (let i = 0; i < 3; i++) qs.push(setXY(new Q()));
const fn = function () {};
fn.x = 1;
const viaFn = Object.create(fn);
for (let i = 0; i < 3; i++) qs.push(setXY(Object.create(fn)));
console.log(qs.map((o) => Object.keys(o).join("")).join(" "), fn.x);
const arr = Object.create([1, 2]);
console.log(Object.keys(setXY(arr)));
