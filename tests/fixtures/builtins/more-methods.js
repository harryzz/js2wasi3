// T25 slice 4: Object.groupBy and the ES2025 Set methods (F101), __defineGetter__ & co. (F37),
// Array.prototype.keys/entries/toLocaleString (F91), Number.prototype.toLocaleString (en-US, F88).

const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.constructor.name + ": " + e.message); } };
t(() => Object.groupBy(null, (x) => x)); t(() => Object.groupBy([1], 3)); t(() => Object.groupBy(5, (x) => x));
t(() => Object.groupBy([1, 2, 3, 4], (x) => (x % 2 ? "odd" : "even")));
t(() => new Set([1]).union(3)); t(() => new Set([1]).union({})); t(() => new Set([1]).union({ size: -1 }));
t(() => new Set([1]).union({ size: 1 })); t(() => new Set([1]).union({ size: 1, has: () => 1 }));
t(() => new Set([1]).union({ size: "x", has() {}, keys() {} }));
t(() => Set.prototype.union.call({}, new Set()));
t(() => ({}).__defineGetter__("x", 1)); t(() => ({}).__lookupGetter__("x"));
t(() => [1, 2].keys()); t(() => [...["a", "b"].entries()]); t(() => [1, 1234.5, "x", null, undefined, [2, 3]].toLocaleString());
t(() => Array.prototype.keys.call({ length: 2 }).next());
const a = new Set([1, 2, 3]), b = new Set([3, 4]);
console.log(a.union(b), a.intersection(b), a.difference(b), a.symmetricDifference(b));
console.log(a.isSubsetOf(b), new Set([3]).isSubsetOf(b), a.isSupersetOf(new Set([1])), a.isDisjointFrom(new Set([9])));
const m = new Map([[3, "x"], [9, "y"]]);
console.log(a.intersection(m), a.union(m), a.isDisjointFrom(m));
const o = {}; o.__defineGetter__("g", () => 7); o.__defineSetter__("g", (v) => console.log("set", v)); o.g = 2;
console.log(o.g, typeof o.__lookupGetter__("g"), typeof o.__lookupSetter__("g"), o.__lookupGetter__("toString"));
for (const [i, v] of ["p", "q"].entries()) console.log(i, v);
console.log([...[5, 6].keys()], Array.from({ length: 2, 0: "z" }.constructor === Object ? [1] : []));
console.log((1234567.891).toLocaleString(), (-0).toLocaleString(), (0.0005).toLocaleString(), (-1e6).toLocaleString(), NaN.toLocaleString(), (12).toLocaleString());
console.log(Object.groupBy("abca", (c) => c), Object.groupBy([1.5, 2.5], Math.floor));
