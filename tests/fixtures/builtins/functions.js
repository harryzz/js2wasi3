// Function.prototype.bind and `arguments` (T25 slice 3).

function show(label, f) {
  try {
    console.log(label, f());
  } catch (e) {
    console.log(label, e.name + ": " + e.message);
  }
}

// bind: this, partial arguments, name, length, no prototype.
function add(a, b, c) {
  return [this === undefined ? "undef" : this.tag, a, b, c].join();
}
const b1 = add.bind({ tag: "T" }, 1);
console.log(b1(2, 3), b1.name, b1.length, typeof b1, "prototype" in b1);
const b2 = b1.bind({ tag: "ignored" }, 2);
console.log(b2(3), b2.name, b2.length, add.bind(null, 1, 2, 3, 4).length, add.bind().length);
console.log(add.bind(undefined)(1), b1, b2);
const anon = function () {}.bind(null);
console.log(anon.name, (() => 1).bind(null).name, Object.hasOwn(anon, "name"), Object.hasOwn(anon, "length"));

// new through a bound class/function: the target's prototype, instanceof sees through.
class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }
}
const AtTen = Point.bind(null, 10);
const p = new AtTen(20);
console.log(p.x, p.y, p instanceof Point, p instanceof AtTen, Object.getPrototypeOf(p) === Point.prototype);
function Old(v) {
  this.v = v;
}
const BoundOld = Old.bind({ ignored: true });
const o = new BoundOld(7);
console.log(o.v, o instanceof Old, o instanceof BoundOld, o.ignored);
show("new arrow", () => new ((() => 1).bind(null))());
show("new method", () => new ({ m() {} }.m.bind(null))());
show("bind non-function", () => Function.prototype.bind.call(5));
show("bind object", () => Function.prototype.bind.call({}));

// Bound functions as callbacks.
const counter = { n: 5, get(k) { return this.n * k; } };
console.log([1, 2, 3].map(counter.get.bind(counter)).join(), Math.max.bind(null, 3)(1, 7));
console.log([3, 1, 2].sort(function (a, b) { return this.dir * (a - b); }.bind({ dir: -1 })).join());

// arguments (unmapped: modules are strict).
function f(a, b = 5) {
  return [a, b, arguments.length, arguments[0], arguments[2], typeof arguments].join();
}
console.log(f(), f(1), f(1, undefined), f(1, 2, 3));
function sum() {
  let s = 0;
  for (let i = 0; i < arguments.length; i++) {
    s += arguments[i];
  }
  return s;
}
console.log(sum(1, 2, 3, 4), sum(), sum.apply(null, [5, 6]), sum.call(null, 1, 1), sum(...[2, 3]));
console.log([1, 2].map(function () {
  return arguments.length;
}).join());
function iter() {
  const out = [];
  for (const a of arguments) {
    out.push(a);
  }
  return out.join("-") + " " + [...arguments].length + " " + Array.from(arguments).join() + " " + Array.prototype.slice.call(arguments, 1).join();
}
console.log(iter("a", "b", "c"));
function meta() {
  return Object.keys(arguments).join() + " " + JSON.stringify(arguments) + " " + Array.isArray(arguments) + " " + arguments.length;
}
console.log(meta(7, 8), meta());
function unmapped(a) {
  a = 9;
  arguments[1] = "set";
  return [arguments[0], arguments[1], arguments.length].join();
}
console.log(unmapped(1));
function destructured({ x }, [y] = [4]) {
  return x + y + arguments.length;
}
console.log(destructured({ x: 1 }), destructured({ x: 1 }, [10], 99));
const obj = {
  m(x) {
    return x + arguments.length;
  },
};
console.log(obj.m(10, 20, 30));
function* gen() {
  yield arguments.length;
  yield arguments[1];
}
console.log([...gen("p", "q")].join());
async function later() {
  await null;
  return arguments[0] + arguments.length;
}
later(40, 1).then((v) => console.log("async", v));
function outer() {
  function inner() {
    return arguments.length;
  }
  return inner(1, 2, 3) + arguments.length;
}
console.log(outer(1));
function print() {
  console.log(arguments);
}
print(1, "x", [2]);
print();
