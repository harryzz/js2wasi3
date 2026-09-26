// Typed arrays: element conversions, views, iteration, methods, keys (T20 slice 1c).
console.log(new Uint8ClampedArray([0.5, 1.5, 2.5, 254.5, 255.5, -0.5, Infinity]));
console.log(new Int32Array([2 ** 31, -(2 ** 31) - 1, 2 ** 32 + 5, 1e20, -1.9, NaN, Infinity]));
console.log(new Uint16Array([65536 + 3, -1]), new Int8Array([127, 128, 255, -129]));
console.log(new Float32Array([1 / 3, 1e40, -1e-50, 16777217]));
const base = new Uint8Array([1, 2, 3, 4, 5, 6]);
const sub = base.subarray(2, 5); sub[0] = 99; console.log(base, sub, sub.byteOffset, sub.length, sub.buffer === base.buffer);
const [a, b, ...rest] = base; console.log(a, b, rest, Math.max(...base));
for (const i in new Uint8Array(3)) console.log("key", i);
console.log([...base.keys()], [...base.entries()].slice(0, 2), base.values().next());
console.log({ t: new Int16Array([1, 2]), nested: [new Uint8Array(2)] });
console.log(new Float64Array([0.1, 1e21, -0, NaN]), new Uint8Array(0), new Float64Array(8));
const c = new Uint8Array([1, 2, 3, 4, 5]); c.copyWithin(0, 3); console.log(c);
const d = new Uint8Array([1, 2, 3, 4, 5]); d.set(d.subarray(0, 3), 2); console.log(d);
const buf = new ArrayBuffer(4); const f = new Float32Array(buf); f[0] = 1.5; console.log(new Uint8Array(buf), new Uint8Array(buf.slice(2)), buf.byteLength);
console.log(new Uint8Array([5, 1, 4]).toSorted(), new Int8Array([1, 2, 3]).with(-1, 9), new Uint8Array([1, 2]).toReversed());
console.log(new Uint8Array([1, 2, 3]).filter((x) => x % 2), new Uint8Array([1, 2, 3]).reduce((s, x) => s + x), new Uint8Array([3, 4]).findLast((x) => x > 2));
try { Uint8Array.prototype.map.call([1], (x) => x); } catch (e) { console.log(e.message); }
try { new Uint8Array(4).set([1, 2, 3], 2); } catch (e) { console.log(e.constructor.name, e.message); }
try { new ArrayBuffer(-1); } catch (e) { console.log(e.constructor.name, e.message); }
const u = new Uint8Array(2); u.foo = "bar"; u[-1] = 5; u[5] = 5; u["1.5"] = 7; console.log(u.foo, u[-1], u[5], u.length);
console.log(new Uint8Array(new Set([7, 8])), new Uint16Array(new Uint8Array([200, 100])), typeof Uint8Array);
console.log(JSON.stringify({ x: new Uint8Array([1]) }), Object.entries(new Uint8Array([4, 5])));
