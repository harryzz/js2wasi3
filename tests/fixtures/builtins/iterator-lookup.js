// T25 slice 5: iteration looks up Symbol.iterator (F51): a replaced Array.prototype / String.prototype
// [Symbol.iterator] or an own one is used; values and [Symbol.iterator] are the same function.

console.log(Array.prototype.values === Array.prototype[Symbol.iterator], String.prototype[Symbol.iterator] === Array.prototype.values);
const a = [1, 2, 3];
console.log([...a], [...a.values()]);
const orig = Array.prototype[Symbol.iterator];
Array.prototype[Symbol.iterator] = function* () { yield "patched"; };
console.log([...a], [...a.values()]);
for (const x of [9]) console.log(x);
Array.prototype[Symbol.iterator] = orig;
console.log([...a]);
const own = [1, 2]; own[Symbol.iterator] = function* () { yield* [7, 8, 9]; }; console.log([...own]);
String.prototype[Symbol.iterator] = function* () { yield "S"; }; console.log([..."abc"]);
try { for (const x of undefined) {} } catch (e) { console.log(e.message); }
