// T147: a method-call site whose receivers all come from one constructor tests that shape type first; other receivers,
// changed prototypes and own overrides must still reach the right function.
function P(x) { this.x = x; this.y = x + 1; }
P.prototype.sum = function () { return this.x + this.y; };
P.prototype.scale = function (k) { return this.sum() * k; };
function Q(a, b, c) { this.a = a; this.b = b; this.c = c; }
Q.prototype.sum = function () { return this.a + this.b + this.c; };

function run(objs, k) { let t = 0; for (const o of objs) t += o.scale ? o.scale(k) : o.sum(); return t; }
const ps = [new P(1), new P(2), new P(3)];
console.log(run(ps, 2), run(ps, 3));

// the same site with receivers of another constructor (the guard's exact test fails, the generic helper decides)
function callSum(o) { return o.sum(); }
console.log(callSum(ps[0]), callSum(new Q(1, 2, 3)), callSum(ps[1]), callSum({ sum() { return "lit"; } }));

// an own override on one instance, a replaced prototype method, a deleted one
const o1 = new P(10);
o1.sum = function () { return "own"; };
console.log(callSum(o1), callSum(ps[2]));
P.prototype.sum = function () { return "replaced"; };
console.log(callSum(ps[0]), callSum(o1), callSum(new P(5)));
P.prototype.sum = function () { return this.x * 100; };
console.log(callSum(ps[0]));
delete P.prototype.sum;
try { callSum(ps[0]); } catch (e) { console.log(e.constructor.name); }
P.prototype.sum = function () { return -1; };
console.log(callSum(ps[0]));

// receivers whose shape changed after construction (a property added or deleted), a different prototype
const p2 = new P(7);
p2.extra = 1; delete p2.y;
console.log(callSum(p2));
Object.setPrototypeOf(p2, Q.prototype);
p2.a = 1; p2.b = 2; p2.c = 3;
console.log(callSum(p2));
const sub = Object.create(P.prototype); sub.x = 4;
console.log(callSum(sub));

// subclasses and null / primitive receivers at the site
class A { constructor() { this.v = 1; } m() { return this.v; } }
class B extends A { constructor() { super(); this.w = 2; } m() { return super.m() + this.w; } }
function callM(o) { return o.m(); }
console.log(callM(new A()), callM(new B()), callM(new A()));
try { callM(null); } catch (e) { console.log(e.constructor.name); }
try { callM(undefined); } catch (e) { console.log(e.constructor.name); }
Number.prototype.m = function () { return typeof this; };
console.log(callM(5));
let total = 0;
for (let i = 0; i < 2000; i++) total += callSum(i % 3 === 0 ? ps[0] : i % 3 === 1 ? new Q(1, 1, 1) : ps[1]);
console.log(total);
