// Array.prototype methods, Array(), Array.from/of (T21 slice 3).
const a = [3, 1, 4, 1, 5, 9, 2, 6];

console.log(a.map((x) => x * 2));
console.log(a.map((x, i, arr) => x + i + arr.length));
console.log(a.filter((x) => x % 2 === 1));
console.log(a.reduce((s, x) => s + x));
console.log(a.reduce((s, x) => s + x, 100));
console.log(a.reduceRight((s, x) => s + "," + x, ""));
console.log(["a", "b", "c"].reduceRight((s, x) => s + x));
let sum = 0;
a.forEach((x, i) => {
  sum += x * i;
});
console.log(sum);
console.log(a.indexOf(1), a.indexOf(1, 2), a.indexOf(7), a.indexOf(9, -3), a.lastIndexOf(1), a.lastIndexOf(1, 2));
console.log(a.lastIndexOf(3, -9), a.indexOf(3, -100));
console.log(a.includes(9), a.includes(10), [NaN].includes(NaN), [NaN].indexOf(NaN), [0].includes(-0));
console.log(a.slice(2, 5), a.slice(-2), a.slice(), a.slice(5, 2), a.slice(1, -1));
console.log(a.find((x) => x > 3), a.findIndex((x) => x > 3), a.findLast((x) => x < 3), a.findLastIndex((x) => x < 3));
console.log(a.find((x) => x > 100), a.findIndex((x) => x > 100), a.findLast((x) => x > 100), a.findLastIndex((x) => x > 100));
console.log(a.some((x) => x > 8), a.some((x) => x > 9), a.every((x) => x > 0), a.every((x) => x > 1));
console.log([].some((x) => true), [].every((x) => false));
console.log(a.at(0), a.at(-1), a.at(8), a.at(-9));
console.log([1, 2].concat([3, 4], 5, [[6]]), [].concat(), [1].concat("ab"));

// In place.
const b = [1, 2, 3, 4, 5];
console.log(b.reverse(), b);
console.log(b.fill(0, 1, 3), [1, 2, 3].fill(7), [1, 2, 3].fill(8, -1));
const c = [1, 2, 3, 4, 5];
console.log(c.splice(1, 2), c);
console.log(c.splice(1, 0, "x", "y"), c);
console.log(c.splice(-2), c);
console.log(c.splice(0, 1, "p", "q", "r"), c);
console.log(c.splice(1), c);
console.log(c.splice(), c);
const d = [1, 2, 3];
console.log(d.shift(), d, d.unshift(8, 9), d, [].shift());
console.log([1, 2, 3, 4, 5].copyWithin(0, 3), [1, 2, 3, 4, 5].copyWithin(1, 0, 3), [1, 2, 3, 4, 5].copyWithin(-2, 0));

// Sorting.
console.log([3, 20, 100, 1].sort(), [3, 20, 100, 1].sort((x, y) => x - y));
console.log(["b", "a", "c", undefined, "B"].sort());
const people = [
  { n: "a", age: 30 },
  { n: "b", age: 20 },
  { n: "c", age: 30 },
  { n: "d", age: 20 },
  { n: "e", age: 25 },
];
console.log(people.sort((x, y) => x.age - y.age).map((p) => p.n).join(""));
const big = [];
for (let i = 0; i < 200; i++) {
  big.push((i * 7919) % 211);
}
const sorted = big.slice().sort((x, y) => y - x);
console.log(sorted.slice(0, 10), sorted.length, big.toSorted().slice(0, 5));
console.log([5, 1, 4].toSorted(), [1, 2, 3].toReversed(), [1, 2, 3].with(1, 9), [1, 2, 3].with(-1, 0));
console.log([1, 2, 3, 4].toSpliced(1, 2, "a"), [1, 2, 3].toSpliced(1));

// Flattening.
console.log([1, [2, [3, [4]]]].flat(), [1, [2, [3, [4]]]].flat(Infinity), [1, [2]].flat(0));
console.log([1, 2, 3].flatMap((x) => [x, x * 10]), ["a b", "c"].flatMap((s) => s.split(" ")));

// Holes.
const h = [1, , 3];
console.log(h.map((x) => x * 2), h.filter(() => true), h.indexOf(undefined), h.includes(undefined));
let visits = 0;
h.forEach(() => visits++);
console.log(visits, h.findIndex((x) => x === undefined), [, 1].sort(), [3, , 1].sort());

// Callback `this` and arguments.
const obj = { k: 10 };
console.log([1, 2].map(function (x) { return x + this.k; }, obj));
console.log([1, 2, 3].filter(function (x) { return x > this.k; }, { k: 1 }));

// Constructors.
console.log(Array(3), Array(3).length, new Array(2).fill(0), Array(1, 2), Array("3"), new Array(), Array());
console.log(Array.of(7), Array.of(1, 2, 3), Array.of());
console.log(Array.from("héllo"), Array.from([1, 2, 3], (x) => x * x), Array.from({ length: 3 }, (_, i) => i));
console.log(Array.from({ length: 2, 0: "a", 1: "b" }));
function* gen() {
  yield 1;
  yield 2;
}
console.log(Array.from(gen()), Array.from(gen(), (x) => x + 1));

// Array-likes as `this`.
const like = { length: 3, 0: "x", 1: "y", 2: "z" };
console.log(Array.prototype.map.call(like, (s) => s.toUpperCase()));
console.log(Array.prototype.indexOf.call(like, "y"), Array.prototype.slice.call(like, 1));
console.log(Array.prototype.map.call("abc", (s) => s + s));

// Lengths.
console.log([].map.length, [].reduce.length, [].splice.length, [].slice.length, [].flat.length, Array.from.length, Array.of.length);

// Errors.
for (const f of [
  () => [1].map(3),
  () => [1].forEach(undefined),
  () => [1].filter({}),
  () => [1].some([]),
  () => [1].sort(1),
  () => [].reduce((x, y) => x),
  () => Array(-1),
  () => new Array(1.5),
  () => [1].flatMap(1),
  () => Array.from(null),
  () => Array.prototype.map.call(null, (x) => x),
  () => [1, 2].with(5, 0),
]) {
  try {
    f();
  } catch (e) {
    console.log(e.name, e.message);
  }
}

// node 24's messages for callbacks and comparators of every type.
for (const v of [3, "x", true, null, undefined, {}, [1], Symbol("s"), -0, NaN]) {
  for (const f of [() => [1].forEach(v), () => Array.from([1], v), () => [2, 1].sort(v), () => [1].flatMap(v), () => [1].find(v)]) {
    try {
      f();
      console.log("ok");
    } catch (e) {
      console.log(e.name, e.message);
    }
  }
}
