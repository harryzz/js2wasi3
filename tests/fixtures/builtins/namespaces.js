// T25 slice 4: `Math` and `JSON` as values (F81), `Number`'s constants on its constructor value.
// A static read with a computed key is found when its name appears in the code ("floor").

const M = Math, J = JSON;
console.log(M, J, [Math]);
console.log(M.floor(2.7), M.PI, M.max(1, 5), J.stringify({ a: [1] }), J.parse("[2]"));
const fns = ["floor", "ceil", "abs"];
console.log(fns.map((k) => Math[k](-1.5)));
console.log(Object.keys(Math), typeof Math, Math.SQRT2 === M.SQRT2);
const N = Number;
console.log(N.MAX_SAFE_INTEGER, N.EPSILON, N.isInteger(3), N("7"));
function apply(ns, k, v) { return ns[k](v); }
console.log(apply(Math, "sqrt", 16), apply(JSON, "stringify", "x"));
