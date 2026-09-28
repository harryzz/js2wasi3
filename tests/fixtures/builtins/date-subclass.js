// Date subclasses (`extends Date`, `Reflect.construct` with a new target): prototype, printing.
class X extends Date { y() { return this.getUTCFullYear(); } }
const x = new X(0);
console.log(x.y(), x instanceof X, x instanceof Date, x, Object.prototype.toString.call(x), x.getTime());
class Y extends Date { constructor(a) { super(a * 1000); this.k = 1; } }
const y = new Y(5);
console.log(y.getTime(), y.k, y instanceof Y, JSON.stringify(y), String(y));
function F() {}
F.prototype = Object.create(Date.prototype);
const r = Reflect.construct(Date, [1], F);
console.log(r instanceof F, r.getTime(), Object.getPrototypeOf(r) === F.prototype);
const q = Reflect.construct(Date, [2], function () {}.bind());
console.log(q.getTime());
console.log(y, [x]);
