// T69: every kind of value through the operations whose type tests avoid wasmtime's `is_subtype`
// libcall (`runtime::JsTest`): truthiness, `===`/`==`, `+`, `typeof`, String/Number conversion,
// property reads and writes, `in`, `instanceof`, keys.
const sym = Symbol("s");
const fn = function named() {};
class C { m() { return 1; } }
const kinds = [
  ["0", 0], ["1.5", 1.5], ["NaN", NaN], ["-0", -0], ["big int", 2 ** 40],
  ["true", true], ["false", false], ["null", null], ["undefined", undefined],
  ["empty string", ""], ["string", "abc"], ["symbol", sym],
  ["object", { a: 1 }], ["array", [1, 2]], ["function", fn], ["class", C], ["instance", new C()],
  ["String wrapper", new String("w")], ["Number wrapper", new Number(3)], ["Map", new Map([[1, 2]])],
  ["Date", new Date(0)], ["RegExp", /x/g], ["bigint", 10n], ["Promise", Promise.resolve(1)],
];
for (const [name, v] of kinds) {
  const out = [name, typeof v, v ? "truthy" : "falsy", v === v, v == null, v === undefined];
  try { out.push(typeof v === "symbol" ? "(symbol)" : String(v).slice(0, 20)); } catch (e) { out.push(e.constructor.name); }
  try { out.push(typeof v === "symbol" || typeof v === "bigint" ? "(skip)" : Number(v)); } catch (e) { out.push(e.constructor.name); }
  try { out.push(typeof v === "symbol" ? "(skip)" : v + "|"); } catch (e) { out.push(e.constructor.name); }
  if (v !== null && v !== undefined) {
    // (`Symbol.prototype.constructor` traps in js2wasi3: F302.)
    out.push(typeof v.length, typeof v === "symbol" ? "(skip)" : v.constructor === undefined ? "no ctor" : typeof v.constructor);
    out.push(v.toString === undefined ? "no toString" : "toString");
  }
  if (v !== null && (typeof v === "object" || typeof v === "function")) {
    out.push("m" in v, v instanceof Object, v instanceof C, Object.keys(v).length);
    v.extra = name;
    out.push(v.extra, Object.prototype.hasOwnProperty.call(v, "extra"));
  }
  console.log(out.join(" "));
}
// Keys of every kind on one object.
const o = {};
o[1] = "one"; o["two"] = 2; o[sym] = "sym"; o[true] = "t"; o[null] = "n";
console.log(Object.keys(o), o[sym], o["1"], o.true, o.null, sym in o, "two" in o);
console.log([1, "1", true, null, undefined, sym, fn, [1]].map((x) => [1, "1", true, null, undefined, sym, fn, [1]].filter((y) => x === y).length));
console.log([0, "", false, null, undefined, NaN].map((x) => [0, "", false, null, undefined, NaN].filter((y) => x == y).length));
