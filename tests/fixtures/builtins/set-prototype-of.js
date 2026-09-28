// Object.setPrototypeOf (T37).
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.constructor.name + ": " + e.message);
  }
};
t(() => Object.setPrototypeOf(undefined, {}));
t(() => Object.setPrototypeOf(null, null));
t(() => Object.setPrototypeOf(1, {}));
t(() => Object.setPrototypeOf("s", null));
t(() => Object.setPrototypeOf({}, 1));
t(() => Object.setPrototypeOf(1, 1));
t(() => Object.setPrototypeOf(Object.preventExtensions({}), {}));
t(() => Object.setPrototypeOf(Object.preventExtensions([]), {}));
t(() => Object.setPrototypeOf(Object.freeze({ a: 1 }), Object.prototype).a);
t(() => {
  const a = {};
  const b = Object.create(a);
  return Object.setPrototypeOf(a, b);
});
t(() => Object.setPrototypeOf(Object.prototype, {}));
t(() => Object.setPrototypeOf(Object.prototype, null) === Object.prototype);
class K {
  hi() {
    return "hi " + this.n;
  }
}
t(() => Object.setPrototypeOf(Object.preventExtensions(new K()), {}));
t(() => Object.setPrototypeOf.length + Object.setPrototypeOf.name);
const o = Object.setPrototypeOf({ a: 1 }, Array.prototype);
console.log(o, o instanceof Array, Array.isArray(o));
const k = Object.setPrototypeOf({ n: 2 }, K.prototype);
console.log(k, k.hi(), k instanceof K);
const n = Object.setPrototypeOf({ x: 1 }, null);
console.log(n, Object.getPrototypeOf(n), "x" in n, "toString" in n);
const f = Object.setPrototypeOf(() => 3, { extra: 4 });
console.log(f(), f.extra, typeof f);
const arr = Object.setPrototypeOf([1, 2], { get 5() { return "from proto"; } });
console.log(arr[5], arr.length);
const sp = Object.setPrototypeOf;
console.log(Object.getPrototypeOf(sp({}, K.prototype)) === K.prototype);
// Object.prototype methods inherited by the namespaces (F167).
console.log(Reflect.hasOwnProperty("get"), Reflect.hasOwnProperty("enumerate"), Math.hasOwnProperty("PI"));
console.log(JSON.propertyIsEnumerable("parse"), Object.prototype.isPrototypeOf(Math), Reflect.isPrototypeOf({}));
console.log(Reflect.toString(), Math.valueOf() === Math, typeof JSON.toString, Math.constructor === Object);
