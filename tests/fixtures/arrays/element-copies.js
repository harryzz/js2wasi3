// T136: `a[i] = b[j]` of numbers: number-mode and generic arrays, holes, past the end, typed arrays, objects,
// and the assignment's value.

function copy(src, dst, n) {
  for (let i = 0; i < n; i++) dst[i] = src[i];
  return dst;
}
console.log(copy([1.5, 2.25, -0], [], 3), copy([1, 2, 3], [0, 0, 0], 3), copy([1.5, , 3.5], [9, 9, 9], 3));
console.log(copy([0.5, 1.5], [], 4), copy(new Float64Array([0.1, 0.2]), [], 2), copy([7, 8], new Int8Array(2), 2));
console.log(copy({ 0: 1.5, 1: 2.5 }, [], 2), copy([1.5, 2.5], {}, 2));

// Reversal in place, as fannkuchredux's flips (small integers in a generic store).
function rev(p, k) {
  for (let i = 0; 2 * i < k; i++) {
    const t = p[i];
    p[i] = p[k - i];
    p[k - i] = t;
  }
  return p;
}
console.log(rev([0, 1, 2, 3, 4], 4), rev([0.5, 1.5, 2.5], 2));

// Doubles, as navier-stokes (number mode), and the copy's value used.
function shift(x, n) {
  let last = 0;
  for (let i = n - 1; i > 0; i--) last = x[i] = x[i - 1];
  return [x, last];
}
console.log(shift([0.1, 0.2, 0.3, 0.4], 4));

// An inherited element where the source has a hole, and NaN.
Array.prototype[1] = 42.5;
console.log(copy([1.5, , 3.5], [], 3), copy([NaN, 1.25], [], 2));
delete Array.prototype[1];

// Number reads the analysis types, given holes, past-the-end reads, an inherited element at run time.
function copyNums(src, dst, n) {
  for (let i = 0; i < n; i++) dst[i] = src[i];
  return dst;
}
const holes = [1.5, 2.5];
holes.length = 4;
console.log(copyNums([1.5, 2.5, 3.5], [], 3), copyNums([1, 2], [5, 6, 7], 3), copyNums(holes, [], 4));
const big = [0.5, 1.5, 2.5];
big.length = 3;
delete big[1];
Array.prototype[1] = 99.5;
console.log(copyNums(big, [], 3));
delete Array.prototype[1];
// A frozen destination: strict code throws (CommonJS is compiled as strict code, F232; node's sloppy file wouldn't).
function copyStrict(src, dst, n) {
  "use strict";
  for (let i = 0; i < n; i++) dst[i] = src[i];
  return dst;
}
try {
  copyStrict([3.5, 4.5], Object.freeze([0, 0]), 2);
} catch (e) {
  console.log(e.message);
}
console.log(copyStrict([3.5, 4.5], [0, 0], 2));
