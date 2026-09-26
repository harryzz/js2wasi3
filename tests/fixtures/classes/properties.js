const t = (f) => {
  try {
    f();
    console.log("ok");
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
let log = [];
const o = {
  _v: 1,
  get v() {
    log.push("get");
    return this._v;
  },
  set v(x) {
    log.push("set " + x);
    this._v = x;
  },
  get only() {
    return "ro";
  },
};
o.v = 5;
console.log(o.v, o._v, log.join(","));
console.log(Object.keys(o), { ...o }.v);
t(() => {
  o.only = 1;
});
const child = Object.create(o);
child.v = 9;
console.log(child.v, Object.hasOwn(child, "v"), Object.hasOwn(child, "_v"), o._v);

const d = {};
Object.defineProperty(d, "fixed", { value: 42 });
Object.defineProperty(d, "visible", { value: 1, enumerable: true, writable: true, configurable: true });
console.log(d.fixed, Object.keys(d));
t(() => {
  d.fixed = 1;
});
t(() => {
  delete d.fixed;
});
t(() => Object.defineProperty(d, "fixed", { value: 2 }));
t(() => Object.defineProperty(d, "fixed", { value: 42 }));
t(() => Object.defineProperty(d, "x", 5));
t(() => Object.defineProperty(d, "x", { get() {}, value: 1 }));
t(() => Object.defineProperty(1, "x", {}));
let hits = 0;
Object.defineProperty(d, "count", {
  get() {
    return ++hits;
  },
  configurable: true,
});
console.log(d.count, d.count, hits);
const desc = Object.getOwnPropertyDescriptor(d, "fixed");
console.log(desc.value, desc.writable, desc.enumerable, desc.configurable);
const acc = Object.getOwnPropertyDescriptor(d, "count");
console.log(typeof acc.get, acc.set, acc.enumerable, acc.configurable);
console.log(Object.getOwnPropertyDescriptor(d, "missing"));
Object.defineProperty(d, "count", { value: "now data" });
console.log(d.count, Object.getOwnPropertyDescriptor(d, "count").writable);

const f = Object.freeze({ a: 1, nested: { b: 2 } });
t(() => {
  f.a = 2;
});
t(() => {
  f.c = 3;
});
t(() => {
  delete f.a;
});
f.nested.b = 3;
console.log(f.a, f.nested.b, Object.isFrozen(f), Object.isSealed(f), Object.isExtensible(f));
const s = Object.seal({ a: 1 });
s.a = 2;
t(() => {
  s.b = 1;
});
t(() => {
  delete s.a;
});
console.log(s.a, Object.isFrozen(s), Object.isSealed(s));
const pe = Object.preventExtensions({ a: 1 });
delete pe.a;
t(() => {
  pe.z = 1;
});
console.log(Object.keys(pe), Object.isExtensible(pe), Object.isSealed(pe), Object.isFrozen({}));
const arr = Object.freeze([1, 2]);
t(() => {
  arr[0] = 5;
});
t(() => {
  arr.push(3);
});
t(() => {
  arr.pop();
});
t(() => {
  arr.length = 0;
});
console.log(arr.length, arr[0], Object.isFrozen(arr));

class Money {
  constructor(n) {
    this.n = n;
  }
  valueOf() {
    return this.n;
  }
  toString() {
    return "$" + this.n;
  }
}
const m = new Money(5);
console.log(m + 1, `${m}`, m * 2, m > 4, m == 5, [m, m] + "");
const noPrim = {
  toString() {
    return {};
  },
  valueOf() {
    return {};
  },
};
t(() => "" + noPrim);
const onlyValue = { valueOf: () => 7 };
console.log(onlyValue + 1, `${onlyValue}`);
class Temp {
  static get name() {
    return "renamed";
  }
}
console.log(Temp.name);
