// T145: `== null`, `!= null`, `== undefined`, `!= undefined` (inline), in every operand order and value kind.
const values = [undefined, null, 0, -0, 1, NaN, "", "0", "null", "undefined", false, true, [], [null], {}, { valueOf() { return null; } },
  function () {}, Symbol("s"), 10n, 0n, new Date(0), /x/, new Map(), Object.create(null), new Number(0), new String(""), Infinity];
const rows = [];
for (const v of values) {
  rows.push([v == null, v != null, v == undefined, v != undefined, null == v, undefined != v, v === null, v === undefined].map(Number).join(""));
}
console.log(rows.join(" "));

// In conditions, loops, logical operators and as values.
function f(x) {
  let r = "";
  if (x == null) r += "a"; else r += "b";
  if (x != null && x != undefined) r += "c";
  while (x == null) { r += "d"; x = 1; }
  r += (x == null ? "e" : "f");
  r += String(x == null || x == undefined);
  return r;
}
console.log(values.slice(0, 4).map(f));

// Object fields and array elements, optional-chain-like patterns, default arguments.
const o = { a: null, b: undefined, c: 0, d: "" };
console.log(o.a == null, o.b == null, o.c == null, o.d == null, o.zz == null, [1, , null][1] == null, [1, , null][2] == null);
function g(a, b = null) { return [a == null, b == null, a?.x, b ?? "dflt"]; }
console.log(g(), g(null, undefined), g(0, 0), g({ x: 1 }, false));

// Evaluation order and side effects of the other operand.
const log = [];
function side(v) { log.push(v); return v; }
console.log(side(null) == null, null == side(undefined), side(0) != null, undefined != side(1), log.join());
let n = 0;
console.log((n++, null) == null, n);
console.log(typeof document == "undefined", typeof Nonexistent == "undefined");
