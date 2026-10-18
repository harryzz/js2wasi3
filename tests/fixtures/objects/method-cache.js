// T132: guarded direct method calls: every way the remembered method can stop being the right one.
function P(x) { this.x = x; }
P.prototype.get = function (k) { return this.x + k; };
P.prototype.args = function () { return arguments.length + ":" + this.x; };
function Q(x) { this.x = x; }
Q.prototype = Object.create(P.prototype);
Q.prototype.get = function (k) { return "Q" + (this.x * k); };

function run(o, k) { return o.get(k); }
function runArgs(o) { return o.args(1, 2, 3); }

const p = new P(1), q = new Q(2);
const out = [];
for (let i = 0; i < 3; i++) out.push(run(p, i));
out.push(run(q, 3));                       // another receiver shape at the same site
out.push(runArgs(p), runArgs(q));          // `arguments` through the direct call
P.prototype.get = function (k) { return "replaced" + k; };
out.push(run(p, 4));                       // the method on the prototype replaced
p.get = function (k) { return "own" + k; };
out.push(run(p, 5));                       // shadowed by an own property
delete p.get;
out.push(run(p, 6));                       // back to the prototype's
delete P.prototype.get;
Object.prototype.get = function (k) { return "object" + k; };
out.push(run(p, 7));                       // deleted, found further up
delete Object.prototype.get;
Object.defineProperty(P.prototype, "get", { get() { return function (k) { return "getter" + k; }; }, configurable: true });
out.push(run(p, 8));                       // an accessor now
Object.setPrototypeOf(p, Q.prototype);
out.push(run(p, 9));                       // another prototype
const r = { x: 10, get: P.prototype.args };
out.push(run(r, 0) === undefined ? "u" : run(r, 0));
try { run(null, 1); } catch (e) { out.push(e.constructor.name); }
try { run({}, 1); } catch (e) { out.push(e.constructor.name); }
try { run("abc", 1); } catch (e) { out.push(e.constructor.name); }  // a primitive receiver
console.log(out.join(" "));
