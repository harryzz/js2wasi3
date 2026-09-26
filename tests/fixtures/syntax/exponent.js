// `**` and `**=`: special values and exact results. (Inexact results can differ from node in
// the last bit: js2wasi3 uses fdlibm's pow, V8 a variant of it; see the Follow-ups.)
const log = (...a) => console.log(a.join(" "));
const isNegZero = (v) => v === 0 && 1 / v < 0;

log(2 ** 10, 2 ** -2, 2 ** 0.5, 4 ** 0.5, 9 ** 0.5, (-8) ** 3, (-2) ** 4, 10 ** 15, 10 ** -3);
log(2 ** 53, 2 ** 1023, 2 ** 1024, 2 ** -1074, 2 ** -1075, 2 ** -1080, 0.5 ** 1074);
log(3 ** 3 ** 2, (3 ** 3) ** 2, -(2 ** 2), (-2) ** 2, 2 ** -1, 1.5 ** 2, 0.1 ** 2);

// Special values (ECMAScript Number::exponentiate).
const vals = [0, -0, 1, -1, 2, -2, 0.5, -0.5, 3, -3, Infinity, -Infinity, NaN];
for (const x of vals) {
  const row = [];
  for (const y of vals) {
    const r = x ** y;
    row.push(isNegZero(r) ? "-0" : `${r}`);
  }
  log(`${isNegZero(x) ? "-0" : x}:`, row.join(" "));
}
log(1 ** Infinity, (-1) ** -Infinity, NaN ** 0, 1 ** NaN, (-8) ** (1 / 3), (-0.5) ** 0.5);
log(10 ** 400, 10 ** -400, (-10) ** 401, (-10) ** -401, 1.0000001 ** 1e12, 0.9999999 ** 1e12);

// `**=`, precedence with unary operators, variables of every kind.
let a = 3;
a **= 2;
let b = 2;
b **= b **= 2;
const o = { p: 5 };
o.p **= 2;
const arr = [1, 2, 3];
arr[2] **= 3;
log(a, b, o.p, arr[2], 2 ** -a, (-a) ** 2);
let count = 0;
for (let i = 0; i < 20; i++) count += 2 ** i;
log("sum", count, 2 ** 20 - 1);
