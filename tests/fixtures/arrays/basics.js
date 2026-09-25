// Array literals, indexing, length, push/pop, holes, console.log of arrays.
const a = [1, 2, 3];
console.log(a, a.length, a[0], a[2], a[3]);
a.push(4, 5);
console.log(a, a.length, a.pop(), a.length);
a[7] = "x";
console.log(a, a.length, a[5], 5 in a, 7 in a);
a.length = 2;
console.log(a, a.length);
a.length = 4;
console.log(a);

const empty = [];
console.log(empty, empty.length, empty.pop(), typeof empty, Array.isArray(empty), Array.isArray({}));
console.log([, 1, , 2, , , 3]);
console.log(["a", 'it\'s', 'say "hi"', true, null, undefined, -0, 1.5]);
console.log([[1, 2], [3, [4, [5, [6]]]]], [{ a: 1 }, { b: [2] }]);
console.log([1, 2, 3, 4, 5, 6, 7], [1, 20, 300, 4000, 5, 60, 700, 8000, 9, 10]);
console.log(["apple", "banana", "cherry", "date", "elderberry", "fig", "grape", "honeydew"]);

const nested = [0, [1, [2, [3]]]];
console.log(nested[1][1][0], nested.length);
console.log("" + [1, [2, 3], null, undefined], [] + "", [1, 2] + [3], `${[4, 5]}`);
console.log([1, 2].join("-"), [1, [2, 3]].join(), [null, undefined, 0].join(":"));
console.log(a instanceof Array, a instanceof Object, Object.getPrototypeOf(a) === Array.prototype);

let sum = 0;
const nums = [];
for (let i = 0; i < 10; i++) nums.push(i * i);
for (let i = 0; i < nums.length; i++) sum += nums[i];
console.log(sum, nums.length, nums[9]);

const keys = [];
for (const k in ["x", "y", , "z"]) keys.push(k);
console.log(keys, Object.keys([5, 6]), Object.values(["p", "q"]), Object.entries({ a: 1, b: [2] }));

const o = { list: [1, 2] };
o.list.push(3);
o.list[0] += 10;
console.log(o, delete o.list[1], o.list, o.list.length);
try {
  delete o.list.length;
} catch (e) {
  console.log(e.message);
}
try {
  o.list.length = -1;
} catch (e) {
  console.log(e.name, e.message);
}
