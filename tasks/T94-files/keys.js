"use strict";
// T94 micro-benchmark: computed-key property access (`o[k]`), the generic path.
// Usage: node keys.js [case] [n]; case: own | proto | miss | built | rw | all (default all).
const which = process.argv[2] || "all";
const n = Number(process.argv[3] || 2000000);
const names = [];
for (let i = 0; i < 32; i++) names.push("field" + i);

// Own hits on a big object (has a hash index), keys from an array (same string objects each time).
function own() {
  const o = {};
  for (const k of names) o[k] = k.length;
  let s = 0;
  for (let i = 0; i < n; i++) s += o[names[i & 31]];
  return s;
}
// Prototype hits two levels up (each indexed level hashed the key again before T94).
function proto() {
  const base = {};
  for (const k of names) base[k] = 1;
  const mid = Object.create(base);
  for (let i = 0; i < 20; i++) mid["m" + i] = i;
  const o = Object.create(mid);
  for (let i = 0; i < 20; i++) o["o" + i] = i;
  let s = 0;
  for (let i = 0; i < n; i++) s += o[names[i & 31]];
  return s;
}
// Misses (`in` style checks through the whole chain).
function miss() {
  const o = {};
  for (let i = 0; i < 20; i++) o["x" + i] = i;
  let s = 0;
  for (let i = 0; i < n; i++) if (o[names[i & 31]] === undefined) s++;
  return s;
}
// Keys built at run time (a fresh string each access: the memo can't help).
function built() {
  const o = {};
  for (const k of names) o[k] = 1;
  let s = 0;
  for (let i = 0; i < n; i++) s += o["field" + (i & 31)];
  return s;
}
// Read then write the same key (`o[k] = o[k] + 1`).
function rw() {
  const o = {};
  for (const k of names) o[k] = 0;
  for (let i = 0; i < n; i++) { const k = names[i & 31]; o[k] = o[k] + 1; }
  return o.field3;
}
const cases = { own, proto, miss, built, rw };
for (const name of which === "all" ? Object.keys(cases) : [which]) {
  const t = Date.now();
  const r = cases[name]();
  console.log(name, r, Date.now() - t, "ms");
}
