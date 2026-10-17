// T128: functions without a build-time layout learn one from their first object at the 8th `new` (fields set by
// another function, later fields); objects that differ from it must still behave.
var Class = { create: function () { return function () { this.initialize.apply(this, arguments); }; } };
var Vec = Class.create();
Vec.prototype = { x: 0, y: 0, z: 0, initialize: function (x, y, z) { this.x = x; this.y = y; this.z = z; } };
var Info = Class.create();
Info.prototype = { hit: false, dist: null, initialize: function () { this.color = new Vec(0, 0, 0); } };
var vs = [], infos = [];
for (var i = 0; i < 20; i++) {
  vs.push(new Vec(i, i + 1, i + 2));
  var info = new Info();
  if (i % 3) { info.hit = true; info.dist = i; }
  infos.push(info);
}
console.log(vs[19], Object.keys(vs[3]), JSON.stringify(vs[12]));
console.log(infos.map(function (o) { return Object.keys(o).join("/"); }).join(" "));
// Objects that leave the learned order: other keys, fewer, more, deletes, accessors, frozen.
var odd = new Vec(1, 2, 3);
odd.w = 4;
delete odd.y;
odd.y = 9;
console.log(odd, Object.keys(odd));
var Pt = function (a, b) { if (a) this.a = a; if (b) this.b = b; };
for (var i = 0; i < 12; i++) new Pt(i, i);
var p1 = new Pt(0, 5), p2 = new Pt(5, 0), p3 = new Pt(1, 2);
p3.c = 3;
Object.defineProperty(p3, "d", { get: function () { return 4; }, enumerable: true });
console.log(p1, p2, p3, p3.d, Object.keys(p3));
var fz = new Pt(7, 8);
Object.freeze(fz);
try { "use strict"; fz.a = 1; } catch (e) { console.log(e.name); }
console.log(fz.a, Object.isFrozen(fz));
// Hand-made inheritance (deltablue's superConstructor.call).
function Base(s) { this.strength = s; this.out = null; }
function Mid(v, s) { Mid.superConstructor.call(this, s); this.v = v; this.ok = false; }
Mid.superConstructor = Base;
Mid.prototype = Object.create(Base.prototype);
var ms = [];
for (var i = 0; i < 15; i++) ms.push(new Mid(i, i * 2));
console.log(ms[0], ms[14], ms[14] instanceof Base);
// A constructor that returns another object; Reflect.construct with another new.target.
function Ret(x) { this.x = x; if (x > 10) return { other: x }; }
var rs = [];
for (var i = 0; i < 14; i++) rs.push(new Ret(i));
console.log(rs[3], rs[13]);
function Other() {}
Other.prototype.tag = "other";
var r = Reflect.construct(Ret, [1], Other);
console.log(r.x, r.tag, Object.keys(r));
// Many keys (no layout) and keys first set on the prototype.
function Big() { for (var i = 0; i < 12; i++) this["k" + i] = i; }
for (var i = 0; i < 10; i++) new Big();
console.log(Object.keys(new Big()).length);
function Late() {}
var late = [];
for (var i = 0; i < 10; i++) { var o = new Late(); o.first = i; late.push(o); }
Late.prototype.first = -1;
var l2 = new Late();
console.log(l2.first, (l2.first = 3, l2.first), Object.keys(l2), late[9].first);
for (var k in new Vec(1, 2, 3)) console.log(k);
