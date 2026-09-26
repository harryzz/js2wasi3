// JSON.stringify (T21 slice 4).
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.name + ": " + JSON.stringify(e.message));
  }
};

// Primitives.
console.log(JSON.stringify(1), JSON.stringify(-0), JSON.stringify(1.5e300), JSON.stringify(NaN), JSON.stringify(-Infinity));
console.log(JSON.stringify("hi"), JSON.stringify(true), JSON.stringify(false), JSON.stringify(null));
console.log(JSON.stringify(undefined), JSON.stringify(() => 1), JSON.stringify(Symbol("s")));
console.log(JSON.stringify(""), JSON.stringify("\u2028\ud800\x7f\x1f\"\\é😀\udc00\b\t\n\f\r\0"));
console.log(JSON.stringify("a\ud83d"), JSON.stringify("\ude00b"), JSON.stringify("\ud83d\ud83d\ude00"));

// Arrays and objects.
console.log(JSON.stringify([]), JSON.stringify({}), JSON.stringify([[]]), JSON.stringify({ a: {} }));
console.log(JSON.stringify({ a: 1, b: [1, "x", null], c: { d: true } }));
console.log(JSON.stringify([undefined, () => 1, Symbol("s"), NaN, , 2]));
console.log(JSON.stringify({ u: undefined, f() {}, s: Symbol("s"), n: null, i: Infinity }));
console.log(JSON.stringify({ 2: "b", 1: "a", z: 0, 10: "c", y: 1 }));
console.log(JSON.stringify({ "quo\"te": 1, "new\nline": 2 }));
const proto = { inherited: 1 };
const child = Object.create(proto);
child.own = 2;
Object.defineProperty(child, "hidden", { value: 3, enumerable: false });
console.log(JSON.stringify(child));
class P {
  constructor() {
    this.x = 1;
    this.y = 2;
  }
  get z() {
    return 3;
  }
}
console.log(JSON.stringify(new P()), JSON.stringify([new P()]));
const withGetter = {
  get g() {
    return "got";
  },
};
console.log(JSON.stringify(withGetter));

// toJSON.
console.log(JSON.stringify({ toJSON() { return { replaced: true }; } }));
console.log(JSON.stringify({ a: { toJSON(k) { return "key:" + k; } }, b: [{ toJSON(k) { return k + typeof k; } }] }));
console.log(JSON.stringify({ toJSON() { return undefined; } }), JSON.stringify({ a: { toJSON: () => undefined } }));
console.log(JSON.stringify({ toJSON: 5, v: 1 }));

// Replacer function.
console.log(JSON.stringify({ a: 1, b: "x", c: [1, 2] }, (k, v) => (typeof v === "number" ? v * 10 : v)));
console.log(JSON.stringify({ a: 1, b: 2 }, (k, v) => (k === "a" ? undefined : v)));
const seen = [];
JSON.stringify({ a: [1, { b: 2 }] }, function (k, v) {
  seen.push(JSON.stringify(k) + ":" + typeof v + ":" + Array.isArray(this));
  return v;
});
console.log(seen.join(" "));
console.log(JSON.stringify(5, (k, v) => (k === "" ? { wrapped: v, key: k } : v)));
console.log(JSON.stringify([1, 2], (k, v) => (k === "0" ? () => 1 : v)));

// Replacer list.
console.log(JSON.stringify({ a: 1, b: 2, c: 3, 1: 4 }, ["c", "a", "zz", 1, "a"]));
console.log(JSON.stringify({ a: { a: 1, b: 2 }, b: 3 }, ["a"]));
console.log(JSON.stringify([{ a: 1, b: 2 }], ["b", {}, null, true]));

// Indentation.
console.log(JSON.stringify({ a: [1, { b: 2 }, []], c: {} }, null, 2));
console.log(JSON.stringify([1, [2, [3]]], null, "--"));
console.log(JSON.stringify({ a: 1 }, null, 20));
console.log(JSON.stringify({ a: 1 }, null, "abcdefghijklmnop"));
console.log(JSON.stringify({ a: 1 }, null, 0), JSON.stringify({ a: 1 }, null, -3), JSON.stringify({ a: 1 }, null, ""));
console.log(JSON.stringify({ a: 1 }, null, 2.9), JSON.stringify([], null, 2), JSON.stringify({}, null, 2));
console.log(JSON.stringify({ a: [undefined], b: undefined }, null, 1));

// Errors.
const a = {};
a.self = a;
t(() => JSON.stringify(a));
const b = { x: { y: [1, { z: null }] } };
b.x.y[1].z = b;
t(() => JSON.stringify(b));
const c = { p: { q: { r: { s: { t: {} } } } } };
c.p.q.r.s.t.u = c.p;
t(() => JSON.stringify(c));
class K {
  constructor() {
    this.me = this;
  }
}
t(() => JSON.stringify(new K()));
const d = Object.create(null);
d.d = d;
t(() => JSON.stringify(d));
const e = [];
e.push(e);
t(() => JSON.stringify(e));
const f = {};
f.g = { h: f };
t(() => JSON.stringify([f]));
const g = [{}];
g[0].back = [g];
t(() => JSON.stringify(g));
t(() => JSON.stringify({ toJSON() { throw new RangeError("from toJSON"); } }));
// Not circular: the same object twice.
const shared = { s: 1 };
console.log(JSON.stringify({ a: shared, b: shared, c: [shared, shared] }));
