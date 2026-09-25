// for-of, the iterator protocol, closing iterators, spread.
const seen = [];
for (const x of [1, 2, 3]) seen.push(x * 10);
console.log(seen);

let s = "";
for (const ch of "héllo 😀!") s += "[" + ch + "]";
console.log(s);

// A live array: elements pushed during the loop are visited.
const grow = [1, 2];
for (const x of grow) {
  if (x < 4) grow.push(x + 2);
}
console.log(grow);

// Holes read as undefined.
for (const x of [, "b"]) console.log(x);

function range(from, to) {
  const log = [];
  return {
    log,
    [Symbol.iterator]() {
      let i = from;
      return {
        next() {
          return i < to ? { value: i++, done: false } : { value: undefined, done: true };
        },
        return() {
          log.push("closed at " + i);
          return {};
        },
      };
    },
  };
}

const r = range(0, 5);
for (const v of r) console.log("v", v);
console.log(r.log);

const r2 = range(0, 5);
for (const v of r2) {
  if (v === 2) break;
}
console.log(r2.log);

const r3 = range(10, 20);
function firstOver(it, n) {
  for (const v of it) {
    if (v > n) return v;
  }
  return -1;
}
console.log(firstOver(r3, 12), r3.log);

const r4 = range(0, 3);
try {
  for (const v of r4) {
    if (v === 1) throw new Error("stop");
  }
} catch (e) {
  console.log(e.message, r4.log);
}

const r5 = range(0, 3);
outer: for (const a of [1, 2]) {
  for (const b of r5) {
    if (b === 1) continue outer;
  }
}
console.log(r5.log);

console.log([...range(3, 6)], [..."ab", ...[1, 2], 3], [...[]]);
function sum3(a, b, c) {
  return a + b + c;
}
console.log(sum3(...[1, 2, 3]), sum3(1, ...[2], ...range(3, 4)));

try {
  for (const x of {}) console.log(x);
} catch (e) {
  console.log(e.message);
}
try {
  const n = 5;
  console.log([...n]);
} catch (e) {
  console.log(e.message);
}
try {
  sum3(...{});
} catch (e) {
  console.log(e.message);
}
const bad = { [Symbol.iterator]() { return { next() { return 1; } }; } };
try {
  for (const x of bad) console.log(x);
} catch (e) {
  console.log(e.message);
}

// Delegating to the built-in iterators.
class_like: {
  const bag = {
    items: ["p", "q"],
    [Symbol.iterator]() {
      return this.items[Symbol.iterator]();
    },
  };
  console.log([...bag], Symbol.iterator in [], typeof [][Symbol.iterator], [].values === [].values);
  const it = [7, 8].values();
  console.log(it.next(), it.next(), it.next(), it.next());
  const sit = "a😀"[Symbol.iterator]();
  console.log(sit.next().value, sit.next().value, sit.next().done);
  for (const v of ["x", "y"].values()) console.log(v);
  const again = [1, 2][Symbol.iterator]();
  console.log(again[Symbol.iterator]() === again, [...again]);
}
