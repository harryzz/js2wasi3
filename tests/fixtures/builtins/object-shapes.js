// T83: what shapes and shape-keyed inline caches can get wrong. Each helper below is one property access
// site, called with objects of different shapes and states, so its cache sees every case.
const getX = (o) => o.x;
const setX = (o, v) => {
  o.x = v;
};
const getM = (o) => o.m;
// (Module code is strict: failed writes throw.)
const attempt = (f) => {
  try {
    f();
  } catch (e) {
    console.log(e.constructor.name + ": " + e.message);
  }
};
const lit = (a, b) => ({ a, b, c: a + b });

// Insertion order after delete and re-add; integer-like keys first.
const o1 = { b: 1, a: 2, c: 3 };
delete o1.a;
o1.a = 4;
o1[2] = "two";
o1[0] = "zero";
o1.z = 5;
console.log(Object.keys(o1), JSON.stringify(o1));
for (const k in o1) console.log("for-in", k, o1[k]);

// Objects from the same literal share a shape; a later add to one doesn't show in the others.
const l1 = lit(1, 2);
const l2 = lit(3, 4);
l2.d = 9;
const l3 = lit(5, 6);
console.log(l1, l2, l3, Object.keys(l3), "d" in l3, l3.d);

// One read site, many shapes: own, at other slots, absent, on the prototype, then a getter.
const shapes = [{ x: 1 }, { a: 0, x: 2 }, { a: 0, b: 0, x: 3 }, { y: 1 }, Object.create({ x: 4 })];
for (let r = 0; r < 3; r++) console.log(shapes.map(getX).join());
shapes.push({
  get x() {
    return "getter " + this.y;
  },
  y: 7,
});
console.log(shapes.map(getX).join());

// Attribute changes: writable, enumerable, configurable; accessor <-> data.
const o2 = { x: 1, y: 2 };
for (let i = 0; i < 3; i++) setX(o2, i);
Object.defineProperty(o2, "x", { writable: false });
try {
  (() => {
    "use strict";
    setX(o2, 10);
  })();
} catch (e) {
  console.log(e.constructor.name, e.message);
}
console.log(o2.x, Object.getOwnPropertyDescriptor(o2, "x"));
Object.defineProperty(o2, "y", { enumerable: false });
console.log(Object.keys(o2), JSON.stringify(o2), o2.y);
let store = 0;
const o3 = { x: 1 };
for (let i = 0; i < 3; i++) console.log("data", getX(o3));
Object.defineProperty(o3, "x", {
  get() {
    return "acc" + store;
  },
  set(v) {
    store = v;
  },
  configurable: true,
});
setX(o3, 42);
console.log("accessor", getX(o3), store);
Object.defineProperty(o3, "x", { value: "data again", writable: true });
console.log(getX(o3), Object.getOwnPropertyDescriptor(o3, "x"));

// freeze / seal / preventExtensions after the caches filled.
const twins = [{ x: 1 }, { x: 1 }, { x: 1 }];
for (const t of twins) setX(t, 2);
Object.freeze(twins[0]);
Object.seal(twins[1]);
Object.preventExtensions(twins[2]);
for (const t of twins) {
  attempt(() => setX(t, 3));
  attempt(() => {
    t.added = true;
  });
  console.log(t.x, t.added, Object.isFrozen(t), Object.isSealed(t), Object.isExtensible(t));
}
const fresh = [{}, {}];
setX(fresh[0], 1);
Object.preventExtensions(fresh[1]);
attempt(() => setX(fresh[1], 1));
console.log(fresh, Object.keys(fresh[1]));

// A prototype method replaced or shadowed after the cache filled.
class A {
  m() {
    return "A.m";
  }
}
class B extends A {}
const bs = [new B(), new B()];
const callM = (o) => o.m();
for (let i = 0; i < 3; i++) console.log(bs.map(callM).join());
A.prototype.m = function () {
  return "A.m replaced";
};
console.log(bs.map(callM).join());
B.prototype.m = function () {
  return "B.m";
};
console.log(bs.map(callM).join());
bs[1].m = () => "own m";
console.log(bs.map(callM).join());
delete B.prototype.m;
console.log(bs.map(callM).join());
Object.defineProperty(A.prototype, "m", {
  get() {
    return () => "A.m getter";
  },
});
console.log(bs.map(callM).join());

// setPrototypeOf after caching; absent then added on the prototype.
const p1 = { m: "p1" };
const p2 = { m: "p2" };
const child = Object.create(p1);
for (let i = 0; i < 3; i++) console.log(getM(child));
Object.setPrototypeOf(child, p2);
console.log(getM(child));
const p3 = {};
const c3 = Object.create(p3);
for (let i = 0; i < 3; i++) console.log(getM(c3));
p3.m = "added to p3";
console.log(getM(c3));
Object.prototype.m = "on Object.prototype";
console.log(getM({}), getM(Object.create(null)));
delete Object.prototype.m;
console.log(getM({}));

// An add that a setter or a read-only property on the chain must stop, after the add was cached.
const plain = [{}, {}];
setX(plain[0], "own");
const withSetter = Object.create({
  set x(v) {
    console.log("setter got", v);
  },
});
setX(withSetter, "via setter");
console.log(Object.keys(withSetter));
const proto4 = {};
const c4 = [Object.create(proto4), Object.create(proto4)];
setX(c4[0], 1);
Object.defineProperty(proto4, "x", { value: "read-only", writable: false });
attempt(() => setX(c4[1], 2));
console.log(c4[0].x, c4[1].x, Object.keys(c4[1]));
Object.defineProperty(proto4, "y", {
  set(v) {
    console.log("late setter", v);
  },
});
const setY = (o, v) => {
  o.y = v;
};
const c5 = [Object.create({}), Object.create(proto4)];
setY(c5[0], 1);
setY(c5[1], 2);
console.log(Object.keys(c5[0]), Object.keys(c5[1]));

// __proto__ in literals.
const base = { hello: "from base" };
const withProto = { __proto__: base, own: 1 };
const withProtoNull = { __proto__: null, own: 2 };
console.log(withProto.hello, Object.getPrototypeOf(withProto) === base, withProtoNull.hello, Object.keys(withProtoNull));

// Many keys: past the shape limit (dictionary mode), with deletes and re-adds.
const big = {};
for (let i = 0; i < 100; i++) big["k" + i] = i;
delete big.k3;
big.k3 = "again";
let sum = 0;
for (const k in big) sum += typeof big[k] === "number" ? big[k] : 0;
console.log(Object.keys(big).length, Object.keys(big).slice(0, 5), Object.keys(big).slice(-2), sum, big.k99, big.k3);
const many = [];
for (let j = 0; j < 3; j++) {
  const o = { x: j };
  for (let i = 0; i < 70; i++) o["p" + i] = i;
  many.push(o);
}
console.log(many.map(getX).join(), many[2].p69, Object.keys(many[1]).length);

// JSON.parse objects (run-time keys) and Object.create(null).
const parsed = JSON.parse('[{"id":1,"name":"a","x":10},{"id":2,"name":"b","x":20},{"name":"c","id":3,"x":30}]');
console.log(parsed.map(getX).join(), parsed.map((p) => p.id + p.name).join(), Object.keys(parsed[2]));
const lit2 = { id: 1, name: "a", x: 11 };
console.log([lit2, parsed[0]].map(getX).join());
const dict = Object.create(null);
dict.x = "null proto";
console.log(getX(dict), getM(dict), Object.keys(dict), "toString" in dict);

// Object.assign, spread, rest and Reflect on shaped objects.
const src = { a: 1, x: 2 };
const copy = { ...src, b: 3 };
const { a: ra, ...rest } = copy;
const assigned = Object.assign({ x: 0 }, src);
console.log(copy, ra, rest, assigned, getX(copy), getX(rest), getX(assigned));
Reflect.deleteProperty(copy, "x");
Reflect.set(copy, "x", "reflected");
console.log(Object.keys(copy), Reflect.get(copy, "x"), Reflect.ownKeys(copy));

// Same shape, different prototypes: an own read stays right, a prototype read follows each object's.
const q1 = Object.create({ m: "q1 proto" });
const q2 = Object.create({ m: "q2 proto" });
q1.x = 1;
q2.x = 2;
for (let i = 0; i < 2; i++) console.log(getX(q1), getX(q2), getM(q1), getM(q2));

// Class instances: fields, private fields, accessors on the prototype.
class C {
  #secret = 1;
  pub = 2;
  get both() {
    return this.#secret + this.pub;
  }
  bump() {
    this.#secret++;
    this.pub++;
    return this.both;
  }
}
const cs = [new C(), new C()];
for (let i = 0; i < 3; i++) console.log(cs.map((c) => c.bump()).join());
console.log(Object.keys(cs[0]), JSON.stringify(cs[1]));

// T85: many transitions from one shape (hashed past 8, more than T83's 64): every object keeps its own keys,
// order and values; the same key with other attributes is another transition.
const wide = [];
for (let i = 0; i < 300; i++) wide.push(JSON.parse(`{"w${i}": ${i}, "tail": ${i * 2}}`));
for (let i = 0; i < 300; i++) wide.push({ [`w${i}`]: -i, tail: 0 });
console.log(wide.length, Object.keys(wide[299]), Object.keys(wide[599]), wide[150].w150, wide[450].w150);
let wideSum = 0;
for (const o of wide) wideSum += o.tail + (o[Object.keys(o)[0]] ?? 0);
console.log(wideSum);
const attrA = {};
Object.defineProperty(attrA, "w7", { value: 1, enumerable: false, writable: true, configurable: true });
const attrB = { w7: 2 };
console.log(Object.keys(attrA), Object.keys(attrB), attrA.w7, attrB.w7, JSON.stringify(attrA), JSON.stringify(attrB));
for (let i = 0; i < 20; i++) {
  const m = { topic: `t${i}`, payload: i, qos: i & 1 };
  if (i % 7 === 0) console.log(JSON.stringify(m), Object.keys(m).join());
}
