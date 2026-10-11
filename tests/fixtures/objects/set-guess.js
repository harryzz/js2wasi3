// T105: a write site's remembered entry must still be `k`, writable and a data property (modules are
// strict: a refused write throws).
function put(o, v) { try { o.x = v; } catch (err) { console.log(err.constructor.name + ': ' + err.message); } }
const a = { x: 1, y: 2 }, b = { y: 1, z: 2, x: 3 }, c = { x: 0 };
for (let i = 0; i < 3; i++) { put(a, i); put(b, i * 10); put(c, 'c' + i); }
console.log(a, b, c);
delete a.x; put(a, 'readded'); console.log(a, Object.keys(a));
delete b.y; put(b, 'after delete'); console.log(b);
Object.defineProperty(c, 'x', { writable: false }); put(c, 'no'); console.log(c.x);
const d = { x: 1 }; put(d, 2);
Object.defineProperty(d, 'x', { get() { return 'getter'; }, set(v) { console.log('setter', v); } });
put(d, 3); console.log(d.x);
const e = { x: 1 }; put(e, 2); Object.freeze(e); put(e, 3); console.log(e.x, Object.isFrozen(e));
const f = { x: 1 }; put(f, 2); Object.seal(f); put(f, 4); console.log(f.x);
const g = { x: 1, y: 1 }; put(g, 2); Object.defineProperty(g, 'x', { writable: true, enumerable: false }); put(g, 6); console.log(g.x, Object.keys(g));
const proto = { set x(v) { console.log('proto setter', v); } };
const h = Object.create(proto); put(h, 5); console.log(Object.keys(h));
const arr = [1, 2]; arr.x = 0; put(arr, 'arr'); console.log(arr.x, arr.length);
class P { constructor() { this.x = 0; this.n = 0; } bump() { this.x = this.x + 1; this.n = this.n + 2; } }
const ps = [new P(), new P(), { n: 0, x: 0 }];
for (let i = 0; i < 5; i++) for (const p of ps) P.prototype.bump.call(p);
console.log(ps);
const many = {}; for (let i = 0; i < 20; i++) many['k' + i] = i; many.x = 'big'; put(many, 'bigger'); console.log(many.x);
