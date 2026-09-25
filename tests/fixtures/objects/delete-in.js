// delete, in, Object.hasOwn, and what they see through the prototype chain.
const proto = { shared: 1 };
const o = Object.create(proto);
o.own = 2;
o.other = 3;
console.log("own" in o, "shared" in o, "nope" in o, "toString" in o, "constructor" in o);
console.log(Object.hasOwn(o, "own"), Object.hasOwn(o, "shared"), Object.hasOwn(o, "toString"));
console.log(delete o.own, "own" in o, o.own);
console.log(delete o.shared, "shared" in o, o.shared);
console.log(delete proto.shared, "shared" in o);
console.log(delete o.neverThere, delete o["other"], "other" in o);

// Keys are converted like property keys.
const m = { 1: "a", true: "b", null: "c" };
console.log(1 in m, "1" in m, true in m, null in m, undefined in m);
console.log(delete m[1], 1 in m);

// Re-adding a deleted key moves it to the end.
const order = { a: 1, b: 2, c: 3 };
delete order.a;
order.a = 4;
let s = "";
for (const k in order) s += k + order[k] + " ";
console.log(s);

// Strings: indices and length are own properties.
console.log(Object.hasOwn("abc", 1), Object.hasOwn("abc", "length"), Object.hasOwn("abc", 3));
console.log(delete (5).x, delete "abc".x);

// Deleting built-in prototype properties works too (they are configurable).
const bare = Object.create(null);
bare.x = 1;
console.log("x" in bare, "toString" in bare, Object.getPrototypeOf(bare));
