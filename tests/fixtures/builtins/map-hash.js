// T91: Map/Set with stored hashes: string keys built at run time, numbers (integers, doubles, -0, NaN),
// "1" vs 1, deleted and re-added keys, iteration across deletes and rehashes.
const m = new Map();
const base = "key";
for (let i = 0; i < 2000; i++) m.set(base + i, i);
let sum = 0;
for (let i = 0; i < 2000; i++) sum += m.get(("k" + "ey") + String(i));
console.log(m.size, sum, m.has("key2000"), m.get("key1999"), m.get("key".substring(0, 3) + "7"));

// Strings and numbers never meet.
const mix = new Map([[1, "num"], ["1", "str"], [1.5, "dbl"], ["1.5", "dstr"]]);
console.log(mix.get(1), mix.get("1"), mix.get(3 / 2), mix.get("1." + "5"), mix.size);

// -0, NaN, integers as doubles.
const z = new Map();
z.set(-0, "zero");
z.set(NaN, "nan");
console.log(z.get(0), z.get(-0), Object.is([...z.keys()][0], -0), z.get(0 / 0), z.get(Number("x")));
z.set(2 ** 31, "big").set(2 ** 31 + 0.5, "bigh").set(-1, "neg").set(0.1 + 0.2, "f");
console.log(z.get(2147483648), z.get(2147483648.5), z.get(-1), z.get(0.30000000000000004), z.size);

// Many integer and double keys (the f64 hash must spread them).
const nums = new Map();
for (let i = 0; i < 5000; i++) nums.set(i, i * 2);
for (let i = 0; i < 5000; i++) nums.set(i + 0.25, -i);
let ok = true;
for (let i = 0; i < 5000; i++) if (nums.get(i) !== i * 2 || nums.get(i + 0.25) !== -i) ok = false;
console.log(nums.size, ok, nums.has(5000), nums.has(0.5));

// Delete and re-add: the old slot keeps the hash, the key moves to the end.
const d = new Map([["a", 1], ["b", 2], ["c", 3]]);
d.delete("b");
d.set("b" + "", 4);
d.delete("a");
console.log([...d], d.get("b"), d.has("a"));

// Iteration while deleting and adding (rehashes happen during it).
const it = new Map();
for (let i = 0; i < 8; i++) it.set("s" + i, i);
const seen = [];
for (const [k, v] of it) {
  seen.push(k);
  if (v < 40) it.set("s" + (v + 8), v + 8);
  it.delete("s" + (v + 1));
}
console.log(seen.length, seen.slice(0, 6).join(","), it.size);

// Set with run-time strings, objects, symbols, BigInts.
const s = new Set();
const o = {};
const sym = Symbol("x");
for (const w of "the quick brown fox jumps over the lazy dog the end".split(" ")) s.add(w);
s.add(o).add(sym).add(10n).add(10);
console.log(s.size, s.has("th" + "e"), s.has(o), s.has({}), s.has(sym), s.has(10n), s.has(BigInt(10)), [...s].join == null);
s.delete("the");
console.log(s.has("the"), [...s].slice(0, 4));
s.clear();
s.add("again");
console.log(s.size, [...s]);
