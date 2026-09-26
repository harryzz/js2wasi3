// Object.assign / fromEntries, Object.prototype methods (T21 slice 4).
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};

// Object.assign.
const target = { a: 1 };
const r = Object.assign(target, { b: 2 }, null, undefined, { a: 3, c: 4 });
console.log(r === target, r);
console.log(Object.assign({}, "xyz"), Object.assign({}, 5, true));
console.log(Object.assign([1, 2, 3], [9]), Object.assign({}, [7, 8]));
const src = Object.create({ inherited: 1 });
src.own = 2;
Object.defineProperty(src, "hidden", { value: 3, enumerable: false });
console.log(Object.assign({}, src));
const log = [];
Object.assign(
  {
    set x(v) {
      log.push("set " + v);
    },
  },
  {
    get x() {
      log.push("get");
      return 42;
    },
  },
);
console.log(log);
console.log(Object.assign.length, Object.fromEntries.length);
const frozen = Object.freeze({ a: 1 });
t(() => Object.assign(frozen, { a: 2 }));
t(() => Object.assign(null, {}));
t(() => Object.assign(undefined));
function F() {}
console.log(Object.assign(F, { tag: "fn" }).tag);

// Object.fromEntries.
console.log(Object.fromEntries([["a", 1], ["b", 2], ["a", 3]]));
console.log(Object.fromEntries([[1, "one"], [true, "t"], [null, "n"]]));
console.log(Object.fromEntries(Object.entries({ x: 1, y: 2 }).map(([k, v]) => [k, v * 10])));
function* gen() {
  yield ["g", 1];
  yield ["h", 2];
}
console.log(Object.fromEntries(gen()));
const p = Object.fromEntries([["__proto__", 1]]);
console.log(Object.keys(p), Object.getPrototypeOf(p) === Object.prototype);
console.log(Object.fromEntries([]), Object.fromEntries([["k"]]));
t(() => Object.fromEntries(null));
t(() => Object.fromEntries(undefined));
t(() => Object.fromEntries([1]));
t(() => Object.fromEntries(["ab"]));
t(() => Object.fromEntries([null]));

// hasOwnProperty, propertyIsEnumerable, isPrototypeOf.
const o = { own: 1 };
console.log(o.hasOwnProperty("own"), o.hasOwnProperty("toString"), o.hasOwnProperty("nope"));
console.log(child().hasOwnProperty("base"), [1, 2].hasOwnProperty(1), [1, 2].hasOwnProperty("length"));
console.log("ab".hasOwnProperty("0"), "ab".hasOwnProperty("length"), "ab".hasOwnProperty("2"));
console.log(o.propertyIsEnumerable("own"), [1].propertyIsEnumerable("length"), [1].propertyIsEnumerable(0));
console.log("ab".propertyIsEnumerable("0"), "ab".propertyIsEnumerable("length"), o.propertyIsEnumerable("toString"));
function child() {
  return Object.create({ base: 1 });
}
class A {}
class B extends A {}
const inst = new B();
console.log(A.prototype.isPrototypeOf(inst), B.prototype.isPrototypeOf(inst), Object.prototype.isPrototypeOf(inst));
console.log(B.prototype.isPrototypeOf(new A()), Object.prototype.isPrototypeOf(Object.create(null)));
console.log(Object.prototype.isPrototypeOf(1), A.prototype.isPrototypeOf(A.prototype), Array.prototype.isPrototypeOf([]));
console.log(Function.prototype.isPrototypeOf(A), A.isPrototypeOf(B));
const hop = Object.prototype.hasOwnProperty;
console.log(hop.call({ k: 1 }, "k"), hop.call([], "length"));
t(() => hop.call(null, "a"));
t(() => Object.prototype.propertyIsEnumerable.call(undefined, "x"));
t(() => Object.prototype.isPrototypeOf.call(null, {}));
console.log(Object.prototype.isPrototypeOf.call(null, 1));
const nullProto = Object.create(null);
nullProto.k = 1;
console.log(Object.prototype.hasOwnProperty.call(nullProto, "k"));

// toString / toLocaleString / valueOf.
const ts = Object.prototype.toString;
console.log(ts.call(undefined), ts.call(null), ts.call(1), ts.call("s"), ts.call(true), ts.call(Symbol()));
console.log(ts.call({}), ts.call([]), ts.call(() => 1), ts.call(new Error("x")), ts.call(new TypeError("y")), ts.call(new A()));
console.log({}.toString(), String({}), `${{ a: 1 }}`, {} + "", [{}] + "");
console.log(o.toLocaleString(), Object.prototype.toLocaleString.call(5));
t(() => Object.prototype.toLocaleString.call(null));
console.log(o.valueOf() === o, [].valueOf().length, Object.prototype.valueOf.call(o) === o);
t(() => Object.prototype.valueOf.call(undefined));
const custom = {
  toString() {
    return "custom";
  },
};
console.log(`${custom}`, custom.toLocaleString(), ts.call(custom));
console.log(hop.length, ts.length, Object.prototype.isPrototypeOf.length, ts.name, hop.name);
