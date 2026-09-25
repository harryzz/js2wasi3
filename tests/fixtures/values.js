// T08: primitive JS values (numbers, booleans, undefined, null) through f64 exports.
// Every export returns a number; values.out holds node's results.

const N = 14;

// The values under test, by index. Falls off the end (undefined) past the last one.
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
  if (i === 10) return -1073741824; // smallest i31
  if (i === 11) return 1073741824; // one past the largest i31: boxed
  if (i === 12) return 2;
}

function typeofCode(v) {
  if (typeof v === "undefined") return 1;
  if (typeof v === "object") return 2;
  if (typeof v == "boolean") return 3;
  if ("number" === typeof v) return 4;
  if (typeof v === "string") return 5;
  if (typeof v !== "function") return 6;
  return 0;
}

// Base-8 digits, one per value.
export function typeofTable() {
  let r = 0;
  for (let i = 0; i < N; i++) r = r * 8 + typeofCode(val(i));
  return r;
}

// For every pair: ===, ==, !==, != as bits, folded into a hash.
export function equalityTable() {
  let h = 0;
  for (let i = 0; i < N; i++) {
    for (let j = 0; j < N; j++) {
      const a = val(i);
      const b = val(j);
      let code = 0;
      if (a === b) code += 1;
      if (a == b) code += 2;
      if (a !== b) code += 4;
      if (a != b) code += 8;
      h = (h * 31 + code) % 1000000007;
    }
  }
  return h;
}

export function strictEqualCount() {
  let n = 0;
  for (let i = 0; i < N; i++) for (let j = 0; j < N; j++) if (val(i) === val(j)) n++;
  return n;
}

export function looseEqualCount() {
  let n = 0;
  for (let i = 0; i < N; i++) for (let j = 0; j < N; j++) if (val(i) == val(j)) n++;
  return n;
}

// ToBoolean in several forms, as bits.
export function truthTable() {
  let r = 0;
  for (let i = 0; i < N; i++) {
    const v = val(i);
    const t = v ? 1 : 0;
    const n = !v;
    const both = (v && 2) || 4;
    r = r * 16 + t + (n ? 0 : 2) + (both === 2 ? 4 : 0) + (!!v === true ? 8 : 0);
  }
  return r;
}

// ToNumber: unary +, arithmetic and comparisons on every value.
export function toNumberTable() {
  let sum = 0;
  for (let i = 0; i < N; i++) {
    const n = +val(i);
    let code = n;
    if (n !== n) code = -1000;
    else if (n === Infinity) code = -2000;
    sum = sum + code * (i + 1);
    if (val(i) < 1) sum += 1000000;
    if (val(i) - 1 >= 0) sum += 10000000;
    if (val(i) * 2 === 2) sum += 100000000;
  }
  return sum;
}

let counter;
let flag = false;

function nothing() {}

function second(a, b) {
  return b;
}

// Implicit undefined (F10), >>> (F9), ??, void, comma, postfix values, jsval globals.
export function misc(x) {
  let r = 0;
  let u;
  r = r * 2 + (u === undefined ? 1 : 0);
  r = r * 2 + (nothing() === undefined ? 1 : 0);
  r = r * 2 + (second(1) === undefined ? 1 : 0);
  r = r * 2 + ((null ?? 7) === 7 ? 1 : 0);
  r = r * 2 + ((0 ?? 7) === 0 ? 1 : 0);
  r = r * 2 + (void x === undefined ? 1 : 0);
  let y = x;
  const old = y++;
  r = r * 2 + (old === x && y === x + 1 ? 1 : 0);
  r = r * 2 + ((1, 2, 3) === 3 ? 1 : 0);
  r = r * 2 + (counter === undefined ? 1 : 0);
  counter = 5;
  r = r * 2 + (counter === 5 ? 1 : 0);
  flag = !flag;
  r = r * 2 + (flag === true ? 1 : 0);
  r = r * 2 + (-1 >>> 0 === 4294967295 ? 1 : 0);
  r = r * 2 + (-x >>> 28 === 15 ? 1 : 0);
  r = r * 2 + ((x > 0) + (x > 0) === 2 ? 1 : 0);
  return r;
}
