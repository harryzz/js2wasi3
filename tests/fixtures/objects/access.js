// Property get/set, compound assignment, ++/--, computed keys, optional chaining.
const o = { count: 0, name: "obj" };
o.count = o.count + 1;
o.count += 10;
o["count"] *= 2;
o.count++;
++o.count;
o.count--;
console.log(o.count, o.count++, o.count, --o.count);
o.name += "!";
o.label = `${o.name}#${o.count}`;
console.log(o.name, o.label);

// Assignment is an expression; keys are converted to strings.
let v = (o.x = 5);
console.log(v, o.x, (o[1] = "one"), o["1"], o[1.0], (o[true] = "t"), o.true);
const keyObj = {};
o[keyObj] = "by object";
console.log(o["[object Object]"]);
o[null] = "n";
o[undefined] = "u";
console.log(o.null, o.undefined);

// Chains of objects.
const a = { b: { c: { d: "deep" } } };
a.b.c.e = "added";
a.b["c"]["d"] += "er";
console.log(a.b.c.d, a.b.c.e);

// Optional chaining short-circuits on null/undefined.
const maybe = { inner: null, list: { first: 1 } };
console.log(maybe?.list?.first, maybe.inner?.x, maybe.inner?.x.y.z, maybe.missing?.["k"]);
let nothing = null;
console.log(nothing?.a, nothing?.a.b, (nothing?.a) === undefined);

// Strings: length and indices; other properties come from String.prototype.
const s = "héllo";
console.log(s.length, s[0], s[1], s["4"], s[5], s[-1], s["01"], s.foo);
let i = 2;
console.log(s[i], s[i + 1]);

// Numbers and booleans have no own properties.
const num = 5;
console.log(num.x, true.y, (1.5).z);
