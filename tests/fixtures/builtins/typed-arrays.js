// Typed arrays and ArrayBuffer: constructors, conversions, printing, errors (T20 slice 1c).
const u = new Uint8Array([1, 2, 300, -1, 1.7]);
console.log(u, u.length, u.byteLength, u.byteOffset, u[2], u[10], u["1"]);
u[0] = 256 + 7; u[9] = 5; console.log(u[0], u[9], Object.keys(u), JSON.stringify(u));
console.log(new Uint8ClampedArray([300, -5, 1.5, 2.5, NaN]));
console.log(new Int16Array([40000, -40000]), new Float32Array([0.1, -0]), new Float64Array(3));
console.log(new Int8Array(new ArrayBuffer(4), 1, 2), new Uint32Array([-1]));
const b = new ArrayBuffer(8); const v32 = new Uint32Array(b); v32[0] = 0x01020304; console.log(new Uint8Array(b), b);
console.log(new Uint8Array(200));
console.log(new ArrayBuffer(60));
console.log(Object.prototype.toString.call(u), u instanceof Uint8Array, u.constructor.name, Uint8Array.name, Uint8Array.length, Uint8Array.BYTES_PER_ELEMENT, u.BYTES_PER_ELEMENT);
console.log([...u], Array.from(u), u.map((x) => x * 2), u.subarray(1, 3), u.slice(1, 3), u.join("-"), String(u));
try { Uint8Array(2) } catch (e) { console.log(e.message) }
try { new Uint8Array(-1) } catch (e) { console.log(e.constructor.name, e.message) }
try { new Int32Array(new ArrayBuffer(5)) } catch (e) { console.log(e.constructor.name, e.message) }
try { new Int32Array(new ArrayBuffer(8), 2) } catch (e) { console.log(e.constructor.name, e.message) }
try { new Int32Array(new ArrayBuffer(8), 4, 2) } catch (e) { console.log(e.constructor.name, e.message) }
console.log(new Uint8Array({ length: 2, 0: 5, 1: 6 }), Uint8Array.from([1, 2], (x) => x + 1), Uint8Array.of(9, 8));
console.log(new Float64Array([1, NaN, -0, Infinity]).includes(NaN), new Float64Array([3, 1, 2, 10]).sort(), new Uint8Array([3,1,2]).reverse());
const s = new Uint8Array(6); s.set([1, 2], 1); s.set(new Uint8Array([9]), 5); console.log(s, s.fill(4, 4), s.indexOf(4), s.at(-1));
console.log(ArrayBuffer.isView(u), ArrayBuffer.isView(b), b.slice(1, 3).byteLength, u.buffer.byteLength);
console.log(new Uint8Array(new Uint16Array([258, 3])), u.toString(), Object.getPrototypeOf(Uint8Array.prototype) === Object.getPrototypeOf(Int8Array.prototype));
for (const x of new Int8Array([5, -5])) console.log(x);
