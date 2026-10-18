// T132: the type analysis gives `o.get(k)` one target, but the program changes the method in ways the analysis
// doesn't follow: the guarded direct call must notice each one.
function P(x) { this.x = x; }
P.prototype.get = function (k) { return this.x + k; };
function run(o, k) { return o.get(k); }
const name = ["g", "e", "t"].join("");
const p = new P(1);
const out = [];
for (let i = 0; i < 3; i++) out.push(run(p, i));
Object.defineProperty(P.prototype, name, { value: function (k) { return "defined" + k; }, configurable: true, writable: true });
out.push(run(p, 3));
Object.assign(p, { get: function (k) { return "assigned" + k; } });
out.push(run(p, 4));
delete p[name];
out.push(run(p, 5));
Object.setPrototypeOf(p, { get: function (k) { return "proto" + k; } });
out.push(run(p, 6));
Object.setPrototypeOf(p, P.prototype);
delete P.prototype[name];
try { run(p, 7); } catch (e) { out.push(e.constructor.name); }
Object.defineProperty(P.prototype, name, { value: function (k) { return "back" + k; }, configurable: true, writable: true });
out.push(run(p, 8));
const p2 = new P(2);
p2.y = 1;                                     // another keys array (shape) at the same site
out.push(run(p2, 9));
console.log(out.join(" "));
