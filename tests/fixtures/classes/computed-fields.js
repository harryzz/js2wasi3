// Computed class field keys (T31): evaluated once, in order with the other computed keys,
// converted with ToPropertyKey; instance fields defined per instance with that key.
const log = [];
function key(k) {
  log.push(k);
  return k;
}
let n = 0;
const counted = {
  toString() {
    n++;
    return "counted";
  },
};
const sym = Symbol("s");

class A {
  [key("a")] = 1;
  static [key("s1")] = "static";
  [key("m")]() {
    return "method";
  }
  [counted] = n;
  [sym] = "symbol";
  [key(1 + 1)] = this.a + 1;
  ["f" + "n"] = function () {};
  [key("arrow")] = () => this;
  [key("late")];
}
console.log(log.join(","));
const a = new A();
const b = new A();
console.log(n, a.a, a.counted, a[sym], a[2], a.fn.name, a.arrow.name, a.arrow() === a);
console.log("late" in a, a.late, A.s1, a.m(), Object.keys(a).join(","));
console.log(a.fn !== b.fn, n);

// Subclass: fields defined after super().
class B extends A {
  [key("b")] = this.a * 10;
  constructor() {
    super();
    log.push("ctor");
  }
}
log.length = 0;
const bb = new B();
console.log(bb.b, log.join(","));

// Keys that throw stop the class evaluation.
try {
  class C {
    [key("ok")] = 1;
    [(() => {
      throw new Error("bad key");
    })()] = 2;
  }
  console.log(C);
} catch (e) {
  console.log(e.message);
}

// Keys in class expressions inside functions (captured per evaluation).
function make(k) {
  return class {
    [k] = k.toUpperCase();
  };
}
const X = make("x");
const Y = make("y");
console.log(new X().x, new Y().y, new X().y);
