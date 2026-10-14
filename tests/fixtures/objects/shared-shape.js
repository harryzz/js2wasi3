// T117: objects of one layout share its keys and attributes; any change to one object's shape gives it its
// own copies first, so its siblings never see the change.
"use strict";

function P(x, y, z) {
  this.x = x;
  this.y = y;
  this.z = z;
}
const lit = () => ({ a: 1, b: 2, c: 3 });
const show = (label, o) => console.log(label, Object.keys(o), JSON.stringify(o), Object.getOwnPropertyNames(o).length);

// Every kind of shape change on one object, a sibling checked after each.
const changes = {
  add: (o) => { o.w = 9; },
  delete: (o) => { delete o.y; },
  "delete first": (o) => { delete o.x; delete o.a; },
  "delete then re-add": (o) => { delete o.y; o.y = 7; },
  "defineProperty readonly": (o) => { Object.defineProperty(o, "x", { writable: false }); },
  "defineProperty hidden": (o) => { Object.defineProperty(o, "y", { enumerable: false }); Object.defineProperty(o, "b", { enumerable: false }); },
  "defineProperty new": (o) => { Object.defineProperty(o, "n", { value: 1, enumerable: true }); },
  accessor: (o) => { Object.defineProperty(o, "z", { get() { return 42; }, enumerable: true }); Object.defineProperty(o, "c", { get() { return 43; }, enumerable: true }); },
  freeze: (o) => { Object.freeze(o); },
  seal: (o) => { Object.seal(o); },
  preventExtensions: (o) => { Object.preventExtensions(o); },
  "many keys (grow)": (o) => { for (let i = 0; i < 20; i++) o["k" + i] = i; },
  "__proto__ key": (o) => { Object.defineProperty(o, "__proto__", { value: 5, enumerable: true }); },
};
for (const [name, change] of Object.entries(changes)) {
  const a = new P(1, 2, 3), b = new P(4, 5, 6), c = lit(), d = lit();
  change(a);
  change(c);
  show(`${name}: changed`, a);
  show(`${name}: sibling`, b);
  show(`${name}: changed literal`, c);
  show(`${name}: sibling literal`, d);
  // A new object after the change still has the plain layout.
  show(`${name}: new`, new P(7, 8, 9));
  console.log(name, Object.isFrozen(b), Object.isSealed(b), Object.isExtensible(b),
    Object.getOwnPropertyDescriptor(b, "x"), Object.getOwnPropertyDescriptor(d, "a"));
  // Writes to the changed object follow its own attributes; the sibling stays writable.
  try { a.x = 100; } catch (e) { console.log(name, "a.x:", e.constructor.name); }
  b.x = 100;
  console.log(name, a.x, b.x, a.z, c.c);
}

// Seen while the constructor runs: before all keys are added, keys past `count` don't exist yet.
function Q(probe) {
  probe(this, "start");
  this.p = 1;
  probe(this, "after p");
  this.q = 2;
  this.r = 3;
}
const seen = [];
new Q((o, at) => seen.push(`${at}: ${JSON.stringify(Object.keys(o))} q in o=${"q" in o} ${o.q} ${o.hasOwnProperty("r")}`));
console.log(seen);

// A key added out of order in the constructor, then the usual order again.
function R(swap) {
  if (swap) this.v = 0;
  this.u = 1;
  this.v = 2;
}
show("R swapped", new R(true));
show("R plain", new R(false));
show("R plain again", new R(false));

// Prototype objects of a layout: methods added, keys of instances unaffected.
function S() { this.m = 1; this.n = 2; }
S.prototype.sum = function () { return this.m + this.n; };
const s1 = new S(), s2 = new S();
s1.n = 10;
delete s2.m;
console.log(s1.sum(), Object.keys(s1), Object.keys(s2), Object.keys(new S()));

// Iteration and copies see the shared keys.
const t = new P(1, 2, 3);
const copy = { ...t };
const assigned = Object.assign({}, t);
const entries = [];
for (const k in t) entries.push(k);
console.log(copy, assigned, entries, Object.entries(t), structuredClone(t));
