// T148: a function (or an object with its own toString) used as a property key: ToPropertyKey runs ToPrimitive.
function f(x) { return x; }
const t = {};
console.log(t[f]);
t[f] = 1;
console.log(Object.keys(t).length, t[f], f in t);
const g = () => 1;
t[g] = 2;
console.log(Object.keys(t).length, t[g], t[f]);
class C { m() {} }
t[C] = 3;
console.log(Object.keys(t).length, t[C]);
console.log(`${(function () {})}`.indexOf("function") === 0, typeof (f + ""));
const key = { toString() { return "k"; } }, u = { k: "found" };
console.log(u[key], key in u);
const v = { valueOf() { return "v"; }, toString() { return "s"; } };
console.log(({ s: 1, v: 2 })[v]);
const sym = { [Symbol.toPrimitive](hint) { return hint === "string" ? "p" : 9; } };
console.log(({ p: "prim", 9: "nine" })[sym]);
try { ({})[{ toString() { return {}; }, valueOf() { return {}; } }]; } catch (e) { console.log(e.name); }
