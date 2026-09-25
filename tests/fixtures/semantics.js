// T11: the T05–T09 subset as a command, checked against node by scripts/diff.sh.
// Prints values instead of returning them, so node is the only oracle needed.

function val(i) {
  if (i === 0) return undefined;
  if (i === 1) return null;
  if (i === 2) return true;
  if (i === 3) return false;
  if (i === 4) return 0;
  if (i === 5) return -0;
  if (i === 6) return 1;
  if (i === 7) return 1.5;
  if (i === 8) return NaN;
  if (i === 9) return Infinity;
  if (i === 10) return -1073741824;
  if (i === 11) return 1073741824;
  if (i === 12) return "";
  if (i === 13) return "0";
  if (i === 14) return "abc";
}
const N = 15;

console.log("-- values, typeof, truthiness");
for (let i = 0; i < N; i++) {
  let v = val(i);
  console.log(i, v, typeof v, v ? "truthy" : "falsy", !v, `${v}`, "" + v);
}

console.log("-- === and == (row: i, then one digit per j)");
for (let i = 0; i < N; i++) {
  let strict = "";
  let loose = "";
  for (let j = 0; j < N; j++) {
    // string == number would need ToNumber of a string (F20): skip those pairs.
    let a = val(i);
    let b = val(j);
    strict = strict + (a === b ? "1" : "0");
    if ((typeof a === "string" && typeof b !== "string") || (typeof b === "string" && typeof a !== "string")) {
      loose = loose + "-";
    } else {
      loose = loose + (a == b ? "1" : "0");
    }
  }
  console.log(i, strict, loose);
}

console.log("-- numbers");
console.log(0.1 + 0.2, 1 / 3, 1e21, 1e-7, 123456789012345680000);
console.log(5e-324, 1.7976931348623157e308, -1e-7, 100, 0.000001);
console.log(7 % 3, -7 % 3, 7 % -3, 5.5 % 2, -0 % 1, 1e300 % 7);
console.log(1 / 0, -1 / 0, 0 / 0, -0 * 1);
console.log(~5, 5 | 0, -5 >> 1, -5 >>> 1, 1 << 31, 2147483648 | 0, 4294967297 | 0);
console.log(3 > 2, 3 >= 3, NaN < 1, NaN >= NaN, null + 1, true + true, undefined + 1);

console.log("-- strings");
let s = "héllo 😀";
console.log(s, s.length, "a" + 1 + 2, 1 + 2 + "a", `x${1.5}y${null}z${true}`);
console.log("" + -0, `${-0}`);
let acc = "";
for (let i = 0; i < 5; i++) acc += i;
console.log(acc, acc.length);

console.log("-- control flow");
let total = 0;
let i = 0;
while (true) {
  i++;
  if (i % 2 === 0) continue;
  if (i > 9) break;
  total += i;
}
console.log(total, i);
let n = 0;
do n++; while (n < 3);
console.log(n, n > 2 && "yes", n < 2 || "no", null ?? "dflt", 0 ?? "zero");

function fib(k) { return k < 2 ? k : fib(k - 1) + fib(k - 2); }
console.log(fib(20));
