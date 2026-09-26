// Math.random: values in [0, 1), not all equal, roughly uniform (T21).
let min = 1;
let max = 0;
let sum = 0;
const buckets = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const n = 20000;
for (let i = 0; i < n; i++) {
  const r = Math.random();
  if (r < min) min = r;
  if (r > max) max = r;
  sum += r;
  buckets[Math.floor(r * 10)]++;
}
console.log(min >= 0, max < 1, min < 0.01, max > 0.99);
console.log(Math.abs(sum / n - 0.5) < 0.02);
let even = true;
for (const b of buckets) {
  even = even && b > n / 10 * 0.9 && b < n / 10 * 1.1;
}
console.log(even);
console.log(Math.random() !== Math.random(), typeof Math.random());
