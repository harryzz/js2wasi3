// Math: constants and the basic functions (T21).
console.log(Math.PI, Math.E, Math.LN2, Math.LN10, Math.LOG2E, Math.LOG10E, Math.SQRT2, Math.SQRT1_2);

const xs = [0, -0, 0.5, -0.5, 1.5, -1.5, 2.5, -2.5, 0.49999999999999994, -0.49999999999999994,
  4503599627370495.5, -4503599627370495.5, 1e300, -1e-300, 3.7, -3.7, NaN, Infinity, -Infinity];
for (const x of xs) {
  console.log(x, Math.floor(x), Math.ceil(x), Math.trunc(x), Math.round(x), Math.sign(x),
    Math.abs(x), Math.sqrt(x), Math.fround(x), Math.clz32(x));
}

console.log(Math.max(), Math.min(), Math.max(1, 3, 2), Math.min(1, 3, 2), Math.max(1, NaN, 3));
console.log(Math.max(0, -0), Math.max(-0, 0), Math.min(0, -0), Math.min(-0, 0));
const arr = [5, 1, 9, -2];
console.log(Math.max(...arr), Math.min(...arr));
console.log(Math.hypot(), Math.hypot(3, 4), Math.hypot(3, 4, 12), Math.hypot(NaN, Infinity),
  Math.hypot(1e200, 1e200), Math.hypot(-0), Math.hypot(1, NaN), Math.hypot(0.1, 0.2, 0.3));
console.log(Math.imul(3, 4), Math.imul(0xffffffff, 5), Math.imul(0x7fffffff, 0x7fffffff));
console.log(Math.clz32(1), Math.clz32(0x80000000), Math.clz32(-1), Math.clz32(0));
console.log(Math.pow(2, 10), Math.pow(2, 0.5), Math.pow(-8, 1 / 3), Math.pow(NaN, 0));

// As values.
const f = Math.floor;
console.log(f(2.5), f.name, f.length, Math.max.name, typeof Math.abs);
console.log(Math.round.call(null, 2.5), Math.max.apply(null, [3, 8, 1]));
console.log(Math.max.length, Math.hypot.length, Math.atan2.length, Math.random.length, Math.sin.name);
