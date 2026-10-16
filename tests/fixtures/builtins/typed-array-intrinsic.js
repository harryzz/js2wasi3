// T121: %TypedArray% (the typed-array constructors' [[Prototype]]), typed [[Set]] through Reflect.set with another
// receiver, Object.freeze/seal of typed arrays. (Writes through a typed array on another object's prototype chain
// follow the spec, where node 24 differs: test262 checks those.)
"use strict";

const TA = Object.getPrototypeOf(Int8Array);
const show = (k, f) => { try { console.log(k, f()); } catch (e) { console.log(k, "!!", e.constructor.name + ": " + e.message); } };
show("typeof, name, length", () => [typeof TA, TA.name, TA.length]);
show("not Function.prototype", () => TA !== Function.prototype && Object.getPrototypeOf(TA) === Function.prototype);
show("every constructor", () => [Int8Array, Uint8Array, Uint8ClampedArray, Int16Array, Uint16Array, Int32Array,
  Uint32Array, Float32Array, Float64Array, BigInt64Array, BigUint64Array].every(C => Object.getPrototypeOf(C) === TA));
show("prototype links", () => [TA.prototype === Object.getPrototypeOf(Float64Array.prototype),
  TA.prototype.constructor === TA, Float64Array.prototype.constructor === Float64Array]);
show("prototype descriptor", () => JSON.stringify(Object.getOwnPropertyDescriptor(TA, "prototype"), (k, v) => k === "value" ? typeof v : v));
show("call", () => TA());
show("new", () => new TA());
show("new with args", () => new TA(4));
show("statics", () => [typeof TA.from, TA.from.length, typeof TA.of, TA.of.length]);
show("own names", () => Object.getOwnPropertyNames(TA).sort().join());
show("Int8Array own names", () => Object.getOwnPropertyNames(Int8Array).sort().join());
show("species", () => [TA[Symbol.species] === TA, Uint8Array[Symbol.species] === Uint8Array,
  Object.getOwnPropertyDescriptor(TA, Symbol.species).get.name]);
show("toStringTag", () => [TA.prototype[Symbol.toStringTag], new Int16Array(1)[Symbol.toStringTag]]);
show("instanceof", () => [new Uint8Array(1) instanceof TA, [] instanceof TA, TA.prototype.isPrototypeOf(new Float32Array(1))]);

// from/of through %TypedArray% take the kind from `this`.
const from = TA.from, of = TA.of;
show("from.call", () => [from.call(Uint8Array, [1, 2, 300]), from.call(Float32Array, new Set([0.5]), x => x * 2)]);
show("of.call", () => [of.call(Int16Array, 1, -2, 70000), of.call(Uint8ClampedArray, 300, -5)]);
show("dynamic C.from", () => { const C = [Int32Array][0]; return C.from([7]); });
show("from on a non-constructor", () => from.call({}, []));
show("of on undefined", () => of.call(undefined, 1));
show("BigInt from", () => from.call(BigInt64Array, [1n, 2n]));

// Methods defined on %TypedArray%.prototype reach every typed array.
show("define on TA.prototype", () => {
  Object.defineProperty(TA.prototype, "sum", { value() { let s = 0; for (const x of this) s += x; return s; }, configurable: true });
  const r = [new Uint8Array([1, 2, 3]).sum(), new Float64Array([0.5, 0.25]).sum()];
  delete TA.prototype.sum;
  return r;
});

// Reflect.set with a receiver (typed [[Set]], ECMA-262 10.4.5.5).
const t = new Uint8Array(2);
show("Reflect.set own", () => [Reflect.set(t, 0, 300), t[0], Reflect.set(t, "1.5", 1), Reflect.set(t, 5, 1), t.length]);
show("Reflect.set receiver, valid index", () => { const o = {}; return [Reflect.set(t, 1, 9, o), t[1], o[1]]; });
show("Reflect.set receiver, out of range", () => { const o = {}; return [Reflect.set(t, 7, 9, o), t[7], o[7], 7 in o]; });
show("Reflect.set receiver, -0", () => { const o = {}; return [Reflect.set(t, "-0", 9, o), "-0" in o]; });
show("Reflect.set symbol", () => { const s = Symbol("1"); return [Reflect.set(t, s, "x"), t[s], t[1]]; });
show("Reflect.get receiver", () => { const o = {}; return [Reflect.get(t, 0, o), Reflect.get(t, "1.5", o), Reflect.get(t, 9, o)]; });

// Integrity levels.
show("freeze with elements", () => Object.freeze(new Uint8Array(1)));
show("seal with elements", () => Object.seal(new Uint8Array(1)));
show("not extensible after", () => { const u = new Uint8Array(1); try { Object.freeze(u); } catch {} return Object.isExtensible(u); });
show("freeze empty", () => { const u = Object.freeze(new Uint8Array(0)); return [Object.isFrozen(u), Object.isSealed(u)]; });
show("isFrozen with elements", () => Object.isFrozen(Object.preventExtensions(new Uint8Array(2))));
const ne = Object.preventExtensions(new Uint8Array(2));
show("non-extensible", () => [Reflect.defineProperty(ne, "foo", { value: 1 }), ne.foo, Reflect.defineProperty(ne, 0, { value: 5 }), ne[0]]);
show("non-extensible add", () => { ne.bar = 1; });
