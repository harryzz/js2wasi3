// T133: objects made by constructors with a layout are structs with their fields inline; typed reads and writes
// check the struct type, the layout and that the entry is set, else take the generic path.
function Point(x, y) {
  this.early = this.peek();
  this.x = x;
  this.y = y;
}
Point.prototype.peek = function () { return this.y; };
Point.prototype.y = "proto-y";
Point.prototype.len = function () { return this.x * this.x + this.y * this.y; };

function show(p) { return [p.x, p.y, p.early, p.len()].join(" "); }

var ps = [];
for (var i = 0; i < 6; i++) ps.push(new Point(i, i + 1));
console.log(ps.map(show).join(" | "));

// an added key: the fields move out, reads and writes stay right
ps[1].z = 3;
ps[1].x = 10;
console.log(show(ps[1]), ps[1].z, JSON.stringify(ps[1]));

// delete: the next read finds the prototype's value
delete ps[2].y;
console.log(ps[2].y, show(ps[2]), Object.keys(ps[2]));
ps[2].y = 7;
console.log(show(ps[2]), Object.keys(ps[2]));

// freeze: writes are ignored (sloppy mode), reads unchanged
Object.freeze(ps[3]);
try { ps[3].x = 99; } catch (e) { console.log("threw", e instanceof TypeError); }
console.log(show(ps[3]), Object.isFrozen(ps[3]));

// an accessor replacing a field
Object.defineProperty(ps[4], "x", { get: function () { return 40; }, configurable: true });
console.log(show(ps[4]));
ps[4].y = 1;
console.log(show(ps[4]));

// a non-writable field
Object.defineProperty(ps[5], "y", { writable: false });
try { ps[5].y = 1; } catch (e) { console.log("threw", e instanceof TypeError); }
console.log(show(ps[5]));

// a setter on the prototype for a key the constructor sets
function Box(v) { this.v = v; this.w = v; }
var log = [];
Object.defineProperty(Box.prototype, "w", { set: function (x) { log.push(x); }, get: function () { return "W"; } });
var b = new Box(2);
b.v = 5;
console.log(b.v, b.w, log, Object.keys(b));

// another object at a typed site (the guard's miss)
function getX(p) { return p.x; }
console.log(getX(new Point(1, 2)), getX({ x: "lit" }), getX(Object.create(Point.prototype)));

// many writes and reads
function Acc() { this.n = 0; this.s = ""; }
var a = new Acc();
for (var j = 0; j < 1000; j++) { a.n = a.n + j; if (j % 100 == 0) a.s = a.s + j; }
console.log(a.n, a.s);

// number-only fields are f64 fields; a non-number written there moves the entries out
function Vec3(x, y, z) { this.x = x; this.y = y; this.z = z; }
Vec3.prototype.add = function (o) { return new Vec3(this.x + o.x, this.y + o.y, this.z + o.z); };
var v = new Vec3(0, 0, 0);
for (var k = 0; k < 100; k++) { v = v.add(new Vec3(k, 0.5, -0)); v.y *= 0.99; }
console.log(v.x, v.y, Object.is(v.z, -0), v);
var w = new Vec3(1, 2, 3);
w.x += 0.25; w.y = w.y / 3; w.z = -w.z;
console.log(w.x, w.y, w.z, JSON.stringify(w), Object.values(w));
function setAny(o, k, val) { o[k] = val; }
setAny(w, "y", "str");
console.log(w, w.y, w.x + 1, Object.keys(w));
w.x = 7; w.z = null;
console.log(w, 1 / new Vec3(-0, 0, 0).x);

// `+=` on a number field: unboxed; a string there afterwards concatenates
var u = new Vec3(1, 2, 3);
for (var q = 0; q < 10; q++) { u.x += 0.5; u.y += q; }
console.log(u.x, u.y);
u.z = "s"; u.z += 1; u.x += "t";
console.log(u.z, u.x, u);
