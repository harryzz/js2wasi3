// T118: element writes on receivers of unknown type: overwrites inline, everything else through `set`/`set_num`.
"use strict";

const write = (o, k, v) => { o[k] = v; };
const writeNum = (o, k, x) => { o[k] = x * 1; };
const writeAt = (o, i, v) => { for (let j = 0; j < 1; j++) o[i + j] = v; }; // an f64 key

// Generic arrays: overwrite, append, hole, negative, fractional, -0, string keys, past length.
const a = [1, "two", { three: 3 }, null];
write(a, 0, "x"); write(a, 3, 4); write(a, 4, 5); write(a, 10, 11);
write(a, -1, "neg"); write(a, 1.5, "frac"); write(a, "2", "str"); write(a, -0, "zero");
writeAt(a, 1, "f64"); writeAt(a, 2.5, "f64frac"); writeAt(a, -0, "f64zero");
console.log(a, a.length, a[-1], a[1.5], a[2.5], Object.keys(a));

// Number values: number mode, hinted (all integers), generic; NaN, -0, big, fractional.
const nums = [0.5, 1.5, 2.5];
const ints = [1, 2, 3];
const mixed = [1, "s", {}];
for (const arr of [nums, ints, mixed]) {
  writeNum(arr, 0, 7); writeNum(arr, 1, 0.25); writeNum(arr, 2, NaN); writeNum(arr, 0, -0);
  writeNum(arr, 3, 2 ** 40); writeNum(arr, 1, 2 ** 31); writeNum(arr, 5, 9);
  console.log(arr, arr.length, Object.is(arr[0], -0), 3 in arr, 4 in arr);
}
const big = [1, 2, 3];
writeNum(big, 1, 1073741823); writeNum(big, 2, 1073741824); writeNum(big, 0, -1073741824); writeNum(big, 0, -1073741825);
console.log(big);

// Holes with an index setter on the prototype chain: the setter runs.
const holey = [1, , 3];
const hnums = [0.5, , 1.5];
Object.defineProperty(Array.prototype, 1, { set(v) { console.log("proto setter", v); }, configurable: true });
write(holey, 1, "h"); writeNum(hnums, 1, 9);
const own = [holey.hasOwnProperty(1), hnums.hasOwnProperty(1)];
delete Array.prototype[1]; // (node's console.log uses arrays too)
console.log(holey, hnums, own);

// Frozen, sealed, non-extensible, read-only elements, accessor elements, read-only length.
const fz = Object.freeze([1, 2]), sl = Object.seal([1, 2]), ne = Object.preventExtensions([1.5, 2.5]);
for (const [name, arr] of [["frozen", fz], ["sealed", sl], ["nonext", ne]]) {
  try { write(arr, 0, 9); writeNum(arr, 1, 8.5); write(arr, 2, 7); } catch (e) { console.log(name, e.constructor.name); }
  console.log(name, arr, arr.length);
}
const ro = [1, 2, 3];
Object.defineProperty(ro, 1, { writable: false });
try { write(ro, 1, 9); } catch (e) { console.log("ro", e.constructor.name); }
try { writeNum(ro, 1, 9); } catch (e) { console.log("ro num", e.constructor.name); }
const acc = [1, 2, 3];
let seen = 0;
Object.defineProperty(acc, 0, { set(v) { seen = v; }, get() { return seen; } });
write(acc, 0, "via setter"); writeNum(acc, 0, 4.5);
console.log(acc, seen);
const rl = [1.5, 2.5];
Object.defineProperty(rl, "length", { writable: false });
try { writeNum(rl, 0, 3); writeNum(rl, 2, 3); } catch (e) { console.log("rl", e.constructor.name, rl); }

// Other receivers: plain objects, Maps, typed arrays, strings, functions, null.
const plain = { 0: "a" }, fn = function () {}, map = new Map(), ta = new Float64Array(2), u8 = new Uint8Array(2);
for (const o of [plain, fn, map, ta, u8]) { write(o, 0, 3.75); writeNum(o, 1, 300.5); writeAt(o, 0, 2); }
console.log(plain, fn[0], fn[1], map[0], map[1], ta, u8);
try { write("str", 0, "x"); } catch (e) { console.log("string", e.constructor.name); }
try { write(null, 0, 1); } catch (e) { console.log("null", e.message); }
try { writeNum(undefined, 0, 1); } catch (e) { console.log("undefined", e.message); }

// Arguments objects.
function args() { write(arguments, 0, "arg"); writeNum(arguments, 1, 1.25); return [...arguments]; }
console.log(args(1, 2, 3));

// A hot loop: the inline path, then the values read back.
const grid = new Array(64).fill(0), fgrid = new Array(64).fill(0.5);
for (let r = 0; r < 100; r++) for (let i = 0; i < 64; i++) { write(grid, i, grid[i] + 1); writeNum(fgrid, i, fgrid[i] + 0.5); }
console.log(grid.reduce((x, y) => x + y), fgrid.reduce((x, y) => x + y), grid.length, fgrid.length);

// Typed arrays with number values: every kind, out of range, negative keys, NaN, clamping. (Keys "1.5" and -0: F429.)
const kinds = [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array, Uint32Array, Float32Array, Float64Array];
for (const K of kinds) {
  const t = new K(3);
  writeNum(t, 0, 300.7); writeNum(t, 1, -1.5); writeNum(t, 2, NaN); writeNum(t, 3, 5); writeNum(t, -1, 6);
  writeAt(t, 1, 2.25); writeAt(t, 3, 9);
  write(t, 2, "42"); write(t, 1, { valueOf: () => 17 });
  console.log(K.name, t, t[-1], t.length);
}
const bi = new BigInt64Array(2);
try { writeNum(bi, 0, 1); } catch (e) { console.log("BigInt64Array", e.constructor.name); }
write(bi, 1, 5n);
console.log(bi);
const view = new Uint8Array(new ArrayBuffer(8), 2, 3);
for (let i = 0; i < 5; i++) writeNum(view, i, i * 100);
console.log(view, new Uint8Array(view.buffer));
const rab = new ArrayBuffer(4, { maxByteLength: 8 });
const tr = new Uint8Array(rab);
writeNum(tr, 3, 1); rab.resize(2); writeNum(tr, 3, 2); writeNum(tr, 1, 3); rab.resize(8); writeNum(tr, 6, 4);
console.log(tr, tr.length);
