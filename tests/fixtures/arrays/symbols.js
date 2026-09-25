// Symbols as keys, Function.prototype.apply, console.log layouts.
const tag = Symbol("tag");
const anon = Symbol();
const o = { [tag]: 1, plain: 2 };
o[anon] = 3;
console.log(typeof tag, tag, anon, Symbol.iterator, o[tag], o[anon], tag in o, Object.keys(o));
console.log(tag === tag, tag === Symbol("tag"), [tag], Object.hasOwn(o, tag));
for (const k in o) console.log("key", k);
try {
  console.log("" + tag);
} catch (e) {
  console.log(e.name, e.message);
}
try {
  console.log(tag * 2);
} catch (e) {
  console.log(e.name, e.message);
}
console.log(tag ? "truthy" : "falsy", Symbol("") ? "yes" : "no");

function describe(greeting, punctuation) {
  return greeting + ", " + this.name + punctuation;
}
console.log(describe.apply({ name: "Ann" }, ["Hi", "!"]), describe.apply({ name: "Bo" }, { length: 2, 0: "Yo", 1: "?" }));
console.log(describe.call({ name: "Cy" }, "Hey", "."), Math2(describe));
function Math2(f) {
  return f.apply({ name: "Di" }, ["Hello"]);
}
try {
  describe.apply(null, 5);
} catch (e) {
  console.log(e.message);
}

const many = [];
for (let i = 0; i < 26; i++) many.push(i * 7);
console.log(many);
console.log(["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa"]);
console.log([1.5, -2, 300, "x"], [true, false, true, false, true, false, true]);
const long = ["a fairly long string number one", "another fairly long string", "and a third one here"];
console.log(long);
console.log({ name: "widget", tags: ["small", "blue"], size: { w: 10, h: 20 }, active: true, count: 12 });
console.log([[1, 2, [3, 4, [5, [6]]]], { a: { b: { c: { d: 1 } } } }]);
console.log([undefined, , null], [[]], [{}], { e: [] });
