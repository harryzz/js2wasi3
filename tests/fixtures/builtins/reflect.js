// Reflect (T36).
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.constructor.name + ": " + e.message);
  }
};

// The namespace.
console.log(Reflect, typeof Reflect, Object.prototype.toString.call(Reflect));
console.log(Reflect[Symbol.toStringTag], Reflect.get.name, Reflect.get.length);
console.log(Object.getOwnPropertyDescriptor(Reflect, "has"));
const names = ["apply", "construct", "defineProperty", "deleteProperty", "get",
  "getOwnPropertyDescriptor", "getPrototypeOf", "has", "isExtensible", "ownKeys",
  "preventExtensions", "set", "setPrototypeOf"];
for (const k of names) {
  console.log(k, Reflect[k].length);
  t(() => Reflect[k](1, "x"));
}

// Readers.
class P {
  get g() {
    return this.tag;
  }
}
const o = Object.create(P.prototype);
o.tag = "o";
o.a = 1;
o[Symbol.iterator] = 2;
o[2] = 3;
console.log(Reflect.get(o, "a"), Reflect.get(o, "g"), Reflect.get(o, "g", { tag: "r" }));
console.log(Reflect.get(o, "missing", {}), Reflect.get([5, 6], 1), Reflect.get([5, 6], "length", {}));
console.log(Reflect.has(o, "g"), Reflect.has(o, "zz"), Reflect.has([1], 0));
console.log(Reflect.ownKeys(o), Reflect.ownKeys([1, 2]), Reflect.ownKeys(function f(a) {}).length);
console.log(Reflect.getOwnPropertyDescriptor(o, "a"), Reflect.getOwnPropertyDescriptor(o, "g"));
console.log(Reflect.getPrototypeOf(o) === P.prototype, Reflect.getPrototypeOf(Object.create(null)));
console.log(Reflect.isExtensible(o), Reflect.preventExtensions(o), Reflect.isExtensible(o));

// apply / construct.
function sum(...xs) {
  return xs.reduce((a, b) => a + b, this === undefined ? 0 : this.base);
}
console.log(Reflect.apply(sum, undefined, [1, 2, 3]), Reflect.apply(sum, { base: 10 }, { length: 2, 0: 1, 1: 2 }));
console.log(Reflect.apply(Math.max, null, [1, 7, 3]));
t(() => Reflect.apply(sum, null));
t(() => Reflect.apply({}, null, []));
t(() => Reflect.apply(undefined, null, []));
t(() => Reflect.apply(null, null, []));
class A {
  constructor(x) {
    this.x = x;
    this.nt = new.target;
  }
}
class B {}
const b = Reflect.construct(A, [1], B);
console.log(b, Object.getPrototypeOf(b) === B.prototype, b.nt === B, b instanceof A);
function F() {
  this.made = 1;
}
console.log(Reflect.construct(F, []) instanceof F, Reflect.getPrototypeOf(Reflect.construct(F, [], A)) === A.prototype);
console.log(Reflect.construct(Array, [3]).length, Reflect.construct(Map, [[[1, 2]]]).get(1));
t(() => Reflect.construct(function () {}, 5));
t(() => Reflect.construct(1, []));
t(() => Reflect.construct(F, [], {}));
t(() => Reflect.construct(F, [], [1]));
// test262's isConstructor.js.
function isConstructor(f) {
  try {
    Reflect.construct(function () {}, [], f);
  } catch (e) {
    return false;
  }
  return true;
}
console.log(isConstructor(F), isConstructor(A), isConstructor(() => 1), isConstructor(sum.bind(null)));
console.log(isConstructor({ m() {} }.m), isConstructor(Array), isConstructor(Math.max));

// Writers return booleans.
const w = {};
console.log(Reflect.defineProperty(w, "k", { value: 1 }), Reflect.defineProperty(w, "k", { value: 2 }), w.k);
console.log(Reflect.defineProperty(Object.freeze({}), "x", { value: 1 }));
t(() => Reflect.defineProperty({}, "x", "s"));
t(() => Reflect.defineProperty({}, "x", { get: {} }));
t(() => Reflect.defineProperty({}, "x", { set: "q" }));
t(() => Reflect.defineProperty({}, "x", { get() {}, value: 1 }));
t(() => Reflect.defineProperty([], "length", { value: -1 }));
const arr = [1, 2, 3];
console.log(Reflect.defineProperty(arr, "length", { value: 1 }), arr);
console.log(Reflect.deleteProperty(w, "k"), Reflect.deleteProperty(w, "nope"));
const fz = Object.freeze({ a: 1 });
console.log(Reflect.deleteProperty(fz, "a"), Reflect.set(fz, "a", 2), fz.a);
const s = { v: 0, set x(v) { this.v = v; }, get only() { return 1; } };
const recv = {};
console.log(Reflect.set(s, "x", 5), s.v, Reflect.set(s, "x", 6, recv), recv.v, s.v);
console.log(Reflect.set(s, "only", 1), Reflect.set(s, "n", 1), s.n);
console.log(Reflect.set(s, "n", 2, recv), recv.n, Reflect.set(s, "n", 3, 1));
const ro = Object.defineProperty({}, "r", { value: 1, writable: false });
console.log(Reflect.set({}, "r", 1, ro), Reflect.set(ro, "r", 2), ro.r);
console.log(Reflect.set(Object.preventExtensions({}), "q", 1));
const arr2 = [1, 2, 3];
console.log(Reflect.set(arr2, "length", 1), arr2, Reflect.set(arr2, 5, "x"), arr2.length);
t(() => Reflect.set([], "length", -1));
const thrower = { set x(v) { throw new TypeError("from setter"); } };
t(() => Reflect.set(thrower, "x", 1));
const proto = {};
const child = Object.create(proto);
console.log(Reflect.setPrototypeOf(child, null), Reflect.getPrototypeOf(child));
console.log(Reflect.setPrototypeOf(proto, child), Reflect.setPrototypeOf(child, proto));
console.log(Reflect.setPrototypeOf(proto, child), Reflect.setPrototypeOf(Object.freeze({}), {}));
console.log(Reflect.setPrototypeOf(Object.prototype, {}), Reflect.setPrototypeOf(Object.prototype, null));
const sp = {};
console.log(Reflect.setPrototypeOf(sp, Array.prototype), Array.isArray(sp), sp instanceof Array);
t(() => Reflect.setPrototypeOf({}, 1));
t(() => Reflect.setPrototypeOf({}, undefined));
const key = { toString() { console.log("key"); return "kk"; } };
console.log(Reflect.defineProperty(w, key, { value: 3, enumerable: true }), Reflect.get(w, key), Reflect.has(w, key));
const R = Reflect;
console.log(R.ownKeys({ z: 1 }), [["a", 1]].map(([k, v]) => R.has({ a: v }, k)));
const ta = new Uint8Array(2);
console.log(Reflect.set(ta, 0, 300), ta[0], Reflect.set(ta, 5, 1), Reflect.get(ta, 1, {}), Reflect.ownKeys(ta));
const sw = new String("ab");
console.log(Reflect.get(sw, 0, {}), Reflect.set(sw, 0, "z"), Reflect.ownKeys(sw), Reflect.has(sw, "length"));
const gt = {};
Reflect.defineProperty(gt, "g", { get() { return this; } });
console.log(Reflect.get(gt, "g", 5), Reflect.set({}, "x", 1, undefined), Reflect.get({ x: 1 }, "x", undefined));
