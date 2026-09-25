// Object literals: key kinds, duplicates, shorthand, spread, __proto__, nesting.
function show(o) {
  let s = "{";
  for (const k in o) s += " " + k + ":" + o[k];
  return s + " }";
}

const k = "computed";
const n = 3;
const o = {
  a: 1,
  "b-c": "two",
  3: "three",
  0.5: "half",
  1e21: "big",
  [k + "Key"]: true,
  [n * 2]: null,
  n,
  a2: undefined,
};
console.log(show(o));
console.log(o.a, o["b-c"], o[3], o["3"], o[0.5], o["1e+21"], o.computedKey, o[6], o.n, o.a2, o.missing);

// A later duplicate keeps the first position.
console.log(show({ x: 1, y: 2, x: 3 }));

// Spread copies own enumerable properties; strings spread their characters.
const base = { p: 1, q: 2 };
console.log(show({ ...base, q: 20, r: 3 }));
console.log(show({ z: 0, ..."hi", ...null, ...undefined, ...7, ...true }));

// `__proto__: v` sets the prototype; a computed or shorthand one is a plain property.
const proto = { inherited: "yes" };
const child = { __proto__: proto, own: 1 };
console.log(child.inherited, Object.getPrototypeOf(child) === proto, show(child));
const __proto__ = "short";
const plain = { __proto__, ["__proto__"]: "computed" };
console.log(Object.hasOwn(plain, "__proto__"), plain.__proto__ === undefined ? "?" : "own");

// Nesting.
const deep = { level1: { level2: { value: 42 } } };
console.log(deep.level1.level2.value, typeof deep.level1, show(deep.level1));
