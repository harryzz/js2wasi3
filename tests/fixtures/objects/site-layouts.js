// T109: objects made by constructors with a layout (their `this.k = …` keys) = node.
"use strict";

const show = (o) => JSON.stringify(Object.keys(o)) + " " + JSON.stringify(o);

// Plain functions and classes, stores in order, out of order, conditional, repeated.
function P(x, y) { this.x = x; this.y = y; }
class Q { constructor(x, y) { this.x = x; this.y = y; this.x = x + y; } }
function C(a) { if (a) this.x = 1; this.y = 2; this.x = 3; this.z = this.x + this.y; }
console.log(show(new P(1, 2)), show(new Q(1, 2)), show(new C(true)), show(new C(false)));
for (let i = 0; i < 3; i++) console.log(show(new P(i, -i)), show(new C(i & 1)));

// A layout object changed afterwards: adds, deletes, re-adds, attributes.
const p = new P(1, 2);
p.w = 4;
delete p.x;
p.x = 5;
console.log(show(p), Object.getOwnPropertyNames(p).join());
const p2 = new P(1, 2);
Object.defineProperty(p2, "x", { writable: false });
try { P.call(p2, 7, 8); } catch (e) { console.log(e.constructor.name, e.message); }
console.log(show(p2));
const p3 = new P(1, 2);
Object.defineProperty(p3, "y", { get() { return "got"; }, set(v) { console.log("set y", v); } });
P.call(p3, 9, 10);
console.log(p3.x, p3.y);

// `this` escapes before the constructor finishes: another function adds a key in between.
function add(o) { o.q = "q"; }
function E() { this.a = 1; add(this); this.b = 2; this.a = 3; }
console.log(show(new E()), show(new E()));
// ... or deletes one, or makes it non-extensible (strict: a TypeError).
function del(o) { delete o.a; }
function F() { this.a = 1; del(this); this.b = 2; this.a = 3; }
console.log(show(new F()));
function G() { this.a = 1; Object.preventExtensions(this); this.a = 2; this.b = 2; }
try { new G(); } catch (e) { console.log(e.constructor.name, e.message); }
function H() { this.a = 1; Object.freeze(this); this.a = 2; }
try { new H(); } catch (e) { console.log(e.constructor.name, e.message); }

// The prototype has a setter or a read-only key for a layout key.
function S(v) { this.s = v; this.t = v; }
console.log(show(new S(1)));
Object.defineProperty(S.prototype, "s", { set(v) { console.log("setter", v); }, configurable: true });
console.log(show(new S(2)));
delete S.prototype.s;
Object.defineProperty(S.prototype, "t", { value: "ro", writable: false, configurable: true });
try { new S(3); } catch (e) { console.log(e.constructor.name, e.message); }
delete S.prototype.t;
console.log(show(new S(4)));
// A new prototype object for the function.
S.prototype = { get s() { return "proto getter"; } };
try { new S(5); } catch (e) { console.log(e.constructor.name, e.message); }
S.prototype = {};
console.log(show(new S(6)));

// Called without `new` on other objects, with another new.target, through Reflect.construct.
const plain = { y: "first" };
P.call(plain, "a", "b");
console.log(show(plain));
class Base { constructor() { this.k1 = 1; this.k2 = 2; } }
class Derived extends Base { constructor() { super(); this.k3 = 3; this.k1 = 9; } }
console.log(show(new Derived()), show(Reflect.construct(Base, [], Derived)), show(Reflect.construct(P, [1, 2], Q)));
console.log(new Derived() instanceof Derived, Object.getPrototypeOf(Reflect.construct(P, [1, 2], Q)) === Q.prototype);

// More keys than a layout holds; `length` / `__proto__` keys; methods storing on `this`.
function Many() { for (const k of "abcdefghij") this[k] = k; this.a1 = 1; this.a2 = 2; this.a3 = 3; this.a4 = 4; this.a5 = 5; this.a6 = 6; this.a7 = 7; this.a8 = 8; this.a9 = 9; }
const m = new Many();
console.log(Object.keys(m).length, m.a9, m.e, "a5" in m, show(new Many()).length);
function L() { this.length = 3; this.size = 4; }
console.log(show(new L()), new L().length);
const o = { setX(v) { this.x = v; this.y = v; return this; } };
console.log(show(o.setX(1)), show(Object.create(o).setX(2)));

// Keys stored under other strings (a computed key with the same content), for-in, spread, assign.
function K() { this["x"] = 1; this.x = 2; this.y = 3; }
const k = new K();
const forIn = [];
for (const key in k) forIn.push(key);
console.log(show(k), forIn.join(), show({ ...k }), show(Object.assign({}, k)), k.hasOwnProperty("y"));

// Object literals with a layout; ones without (computed keys, spread, accessors, duplicates, `__proto__`).
const lits = [];
for (let i = 0; i < 3; i++) lits.push({ a: i, b: [i], "c": String(i), m() { return this.a; } });
lits[1].d = 1;
delete lits[2].a;
lits[2].a = "again";
console.log(lits.map(show).join(" | "), lits[0].m(), lits[1].m());
const dup = { a: 1, b: 2, a: 3 };
const proto = { __proto__: { inherited: 1 }, own: 2 };
const comp = { ["x" + 1]: 1, y: 2 };
const acc = { get g() { return 1; }, h: 2 };
const many = { k1: 1, k2: 2, k3: 3, k4: 4, k5: 5, k6: 6, k7: 7, k8: 8, k9: 9 };
console.log(show(dup), show(proto), proto.inherited, show(comp), show(acc), show(many), Object.keys({ 0: "z", n: 1, 1: "y" }).join());
const same1 = { x: 1, y: 2 }, same2 = new P(1, 2);
same1.z = 3; same2.z = 3;
console.log(show(same1), show(same2), Object.isFrozen(Object.freeze({ f: 1 })));
