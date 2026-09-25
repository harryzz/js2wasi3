// Array and object destructuring: declarations, assignment, parameters, defaults, rest.
const [a, b = 2, , d, ...rest] = [1, undefined, 3, 4, 5, 6];
console.log(a, b, d, rest);

const { x, y: why = "default", z: { deep } = { deep: "d" }, ...others } = { x: 1, y: undefined, p: 3, q: 4 };
console.log(x, why, deep, others);

let p = 1,
  q = 2;
[p, q] = [q, p];
console.log(p, q);

const o = {};
({ a: o.first, b: o["second"] = "two" } = { a: "one" });
console.log(o);

const [[n1, n2], { k }] = [[10, 20], { k: "kay" }];
console.log(n1, n2, k);

const [c1, c2, c3] = "hi";
console.log(c1, c2, c3);

function point({ x = 0, y = 0 } = {}, ...tags) {
  return "(" + x + ", " + y + ")" + (tags.length ? " " + tags.join("|") : "");
}
console.log(point(), point({ x: 3 }), point({ y: 4 }, "a", "b"), point.length);

function defaults(a, b = a * 2, c = b + 1) {
  return [a, b, c];
}
console.log(defaults(1), defaults(1, 5), defaults(1, undefined, 0), defaults.length);

const sum = (...nums) => {
  let t = 0;
  for (const n of nums) t += n;
  return t;
};
console.log(sum(), sum(1, 2, 3), sum(...[4, 5]));

function pairs([first, second] = [], { label } = { label: "none" }) {
  return first + "/" + second + "/" + label;
}
console.log(pairs([1, 2], { label: "L" }), pairs(), pairs(["only"]));

for (const [key, value] of Object.entries({ one: 1, two: 2 })) {
  console.log(key, value);
}
for (const { id, tags: [firstTag] = ["-"] } of [{ id: 1, tags: ["t"] }, { id: 2 }]) {
  console.log(id, firstTag);
}

try {
  const { missing } = undefined;
} catch (e) {
  console.log(e.message);
}
try {
  const { a: { b } } = {};
} catch (e) {
  console.log(e.message);
}
try {
  point(null);
} catch (e) {
  console.log(e.message);
}
try {
  const [z] = {};
} catch (e) {
  console.log(e.message);
}
try {
  const {} = null;
} catch (e) {
  console.log(e.message);
}
try {
  const [e1] = [];
  console.log(e1);
} catch (e) {
  console.log(e.message);
}

// Closures capture destructured bindings per iteration.
const fs = [];
for (const [i, v] of [["a", 1], ["b", 2]]) fs.push(() => i + v);
console.log(fs[0](), fs[1]());

// Array destructuring closes the iterator it doesn't exhaust.
const log = [];
const iterable = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next: () => ({ value: i++, done: false }),
      return: () => {
        log.push("closed");
        return {};
      },
    };
  },
};
const [first, second] = iterable;
console.log(first, second, log);
