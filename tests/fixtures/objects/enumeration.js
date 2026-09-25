// for-in: integer keys ascending, then strings in insertion order; inherited keys after own ones.
function keys(o) {
  let s = "";
  for (const k in o) s += k + " ";
  return s;
}

console.log(keys({ b: 1, 2: 1, a: 1, 1: 1, "01": 1, 4294967295: 1, 4294967294: 1, "-1": 1, 0: 1 }));

const base = { inherited: 1, shadowed: 1 };
const o = Object.create(base);
o.own = 1;
o.shadowed = 2;
console.log(keys(o));

// Non-enumerable built-ins are skipped; a key shadowed by a non-enumerable one is too.
const e = new Error("x");
e.extra = 1;
console.log(keys(e));

// Keys deleted before their turn are skipped; keys added during the loop are not visited.
const live = { a: 1, b: 2, c: 3 };
let seen = "";
for (const k in live) {
  seen += k;
  if (k === "a") {
    delete live.b;
    live.d = 4;
  }
}
console.log(seen, keys(live));

// for-in over strings, null, undefined and numbers.
console.log(keys("abc"), "[" + keys(null) + "]", "[" + keys(undefined) + "]", "[" + keys(42) + "]");

// break, continue, labels, an existing variable as the target.
let k;
let out = "";
outer: for (k in { x: 1, y: 2, z: 3 }) {
  for (const j in { p: 1, q: 2 }) {
    if (j === "q") continue outer;
    if (k === "z") break outer;
    out += k + j + " ";
  }
}
console.log(out, k);

// Many keys (the hash index), in insertion order.
const big = {};
for (let i = 0; i < 30; i++) big["k" + i] = i;
for (let i = 0; i < 30; i += 3) delete big["k" + i];
big.k0 = "back";
let count = 0;
let sum = 0;
for (const key in big) {
  count++;
  if (key !== "k0") sum += big[key];
}
console.log(count, sum);
