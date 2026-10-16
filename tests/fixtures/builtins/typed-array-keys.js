// T120: typed arrays' integer-indexed exotic methods for every kind of key (ECMA-262 10.4.5): a valid index is an
// element, another canonical numeric key is never a property, any other key is ordinary.
"use strict";

const keys = [1.5, "1.5", -0, "-0", "0", 0, "1", 3, "3", 10, "10", -1, "-1", "1e21", 1e21, "Infinity", Infinity,
  "-Infinity", -Infinity, "NaN", NaN, "01", "1.50", "+1", " 1", "1 ", "0x1", "abc", "", "4294967295", 4294967296,
  2 ** 53, "9007199254740993", "1e-7", 1e-7, "0.0000001", "-1.5"];
const show = k => typeof k === "string" ? JSON.stringify(k) : Object.is(k, -0) ? "-0" : String(k);
for (const k of keys) {
  const t = new Uint8Array(4);
  const r = [];
  t[k] = 7;
  r.push(t[k], k in t, Object.keys(t).length, t[0], t.hasOwnProperty(k));
  try { r.push(delete t[k]); } catch (e) { r.push("delete " + e.constructor.name + ": " + e.message); }
  try {
    Object.defineProperty(t, k, { value: 9, writable: true, enumerable: true, configurable: true });
    r.push("defined", t[k]);
  } catch (e) { r.push("define " + e.constructor.name + ": " + e.message); }
  r.push(Reflect.set(t, k, 5), JSON.stringify(Object.getOwnPropertyDescriptor(t, k)), Reflect.has(t, k));
  console.log(typeof k, show(k), JSON.stringify(r));
}

// Constant keys (the compiler's named paths) that a typed array treats as numeric.
const c = new Float64Array(2);
c.NaN = 1; c.Infinity = 2; c["-Infinity"] = 3; c["1.5"] = 4; c["-0"] = 5; c["-1"] = 6; c["0"] = 7; c["1"] = 8;
console.log(c.NaN, c.Infinity, c["-Infinity"], c["1.5"], c["-0"], c["-1"], c[0], c[1], Object.keys(c));
c.name = "x"; c.length2 = 3;
console.log(c.name, c.length2, Object.keys(c));

// The same constant keys on ordinary objects, arrays and functions stay ordinary properties.
const o = {};
o.NaN = 1; o.Infinity = 2; o["1.5"] = 3; o["-0"] = 4; o["-1"] = 5; o["0x"] = 6; o["9a"] = 7;
console.log(JSON.stringify(o), o.NaN + o.Infinity + o["1.5"] + o["-0"] + o["-1"] + o["0x"] + o["9a"]);
o.NaN++; o["-1"] += 10;
console.log(o.NaN, o["-1"], "NaN" in o, delete o["1.5"], Object.keys(o));
const a = [1, 2];
a["-1"] = "neg"; a["1.5"] = "half"; a.NaN = "nan";
console.log(a.length, a["-1"], a["1.5"], a.NaN, Object.keys(a));
const lit = { "1.5": "a", "-1": "b", NaN: "c", Infinity: "d", 7: "e" };
console.log(JSON.stringify(lit), lit["1.5"], lit.NaN);

// Symbols are ordinary keys, also when their description looks like an index.
const t = new Uint8Array(3);
const s1 = Symbol("1"), s0 = Symbol("-0");
t[s1] = 5; t[s0] = 6;
Object.defineProperty(t, Symbol("2"), { value: 7, enumerable: false });
console.log(t[s1], t[s0], t[1], Reflect.ownKeys(t).length, s1 in t, delete t[s1], s1 in t,
  Object.getOwnPropertyDescriptor(t, s0).value);

// Conversions run once, even for an invalid index (TypedArraySetElement), and in order.
const log = [];
const v = { valueOf() { log.push("valueOf"); return 3; } };
const u = new Int16Array(2);
u["1.5"] = v; u[10] = v; u["-0"] = v; u["1"] = v;
console.log(log.length, u[1]);
try { u[0] = { valueOf() { throw new RangeError("boom"); } }; } catch (e) { console.log(e.constructor.name, e.message); }

// defineProperty on elements: data descriptors only, configurable/enumerable/writable not false.
const d = new Int8Array(2);
for (const desc of [{ value: 1 }, { value: 2, configurable: true, enumerable: true, writable: true }, { get() {} },
  { set(x) {} }, { value: 3, configurable: false }, { value: 4, enumerable: false }, { value: 5, writable: false },
  {}, { configurable: true }]) {
  try { Object.defineProperty(d, 0, desc); console.log("ok", d[0]); } catch (e) { console.log(e.constructor.name, e.message); }
}
console.log(Reflect.defineProperty(d, 5, { value: 1 }), Reflect.defineProperty(d, "1", { value: 6 }), d[1]);

// Element descriptors and deletes.
console.log(JSON.stringify(Object.getOwnPropertyDescriptor(d, "1")), Object.getOwnPropertyDescriptor(d, 2));
try { delete d[1]; } catch (e) { console.log(e.constructor.name, e.message); }
console.log(delete d[2], delete d["1.5"], delete d.foo);

// Out of range after a resizable buffer shrinks.
const rab = new ArrayBuffer(4, { maxByteLength: 8 });
const ra = new Uint8Array(rab);
ra["3"] = 9;
console.log(ra[3], "3" in ra);
rab.resize(2);
ra["3"] = 9;
console.log(ra[3], "3" in ra, ra.length, Object.keys(ra), Reflect.ownKeys(ra));

// A typed array on another object's prototype chain: its numeric keys aren't searched past.
const proto = new Uint8Array(2);
const child = Object.create(proto);
Object.prototype["1.5"] = "from Object.prototype";
console.log(child[0], child["1.5"], proto["1.5"], ({})["1.5"]);
delete Object.prototype["1.5"];

// BigInt arrays: ToBigInt for every numeric key, TypeError for a number value.
const b = new BigInt64Array(2);
b["1"] = 5n; b["1.5"] = 6n; b["-0"] = 7n;
console.log(b[1], b["1.5"], b["-0"], Object.keys(b));
try { b["0"] = 1; } catch (e) { console.log(e.constructor.name, e.message); }
try { b["1.5"] = 1; } catch (e) { console.log(e.constructor.name, e.message); }
try { Object.defineProperty(b, "0", { value: 3 }); } catch (e) { console.log(e.constructor.name, e.message); }
Object.defineProperty(b, "0", { value: 3n });
console.log(b[0]);
