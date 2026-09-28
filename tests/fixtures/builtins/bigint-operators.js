// BigInt operators (T40): arithmetic, bitwise, shifts, comparisons, ++/--, compound assignment and
// V8's errors.
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
const x = 10n;
const y = -3n;
console.log(x + y, x - y, x * y, x / y, x % y, y / x, y % x, -x / 4n, -x % 4n, x ** 3n, y ** 3n);
console.log((2n ** 127n) / 3n, (2n ** 127n) % 1000000007n, 0n ** 0n, (-1n) ** 1001n, 1n ** (2n ** 40n));
console.log(-x, ~x, ~y, ~0n, ~-1n, -(-x));
console.log(x & y, x | y, x ^ y, y & -8n, -12n | 5n, -12n ^ -5n, (2n ** 80n - 1n) & -(2n ** 40n));
console.log(x << 3n, x >> 1n, y >> 1n, y >> 10n, -1n >> 100n, x << -1n, y << -2n, 1n << 100n);
console.log(0n << 4000000000n, 5n >> 4000000000n, -5n >> 4000000000n);

// Comparisons: with BigInts, numbers (by value), strings (parsed).
console.log(x < 11n, x > y, x <= 10n, x >= 11n, y < 0n);
console.log(x < 10.5, x > 9.99, x <= 10, x < NaN, x >= NaN, x < Infinity, x > -Infinity);
console.log(2n ** 64n > 2 ** 64, 2n ** 64n >= 2 ** 64, 2n ** 64n + 1n > 2 ** 64, -(2n ** 70n) < -1e21);
console.log(x < "11", x > "9", x < "abc", x >= "abc", "10" <= x, "0x10" > x, x > " 9 ");
console.log(1 < 2n, 2.5 > 2n, NaN < 1n, "1" < 2n);

// ++, --, compound assignment on variables and properties.
let v = 5n;
v++;
++v;
v--;
console.log(v, v++, v, ++v, v--, --v);
v += 10n;
v -= 1n;
v *= 3n;
v /= 2n;
v %= 7n;
v **= 5n;
v <<= 2n;
v >>= 1n;
v &= 255n;
v |= 256n;
v ^= 1n;
console.log(v);
const o = { n: 1n, s: "a" };
o.n++;
++o.n;
o.n *= 10n;
o.s += 1n;
console.log(o, o.n--, o.n, --o.n);
const arr = [1n, 2n];
arr[0] += arr[1];
arr[1] <<= 3n;
console.log(arr);

// Mixed with other types.
console.log(1n + "", "" + 2n, `${3n}`, 1n + "2", [1n] + "");
t(() => 1n + 1);
t(() => 1 * 1n);
t(() => 1n - undefined);
t(() => 1n / 0n);
t(() => 1n % 0n);
t(() => 2n ** -1n);
t(() => +1n);
t(() => 1n >>> 1n);
t(() => 1n >>> 1);
t(() => 1n << (2n ** 40n));
t(() => 2n ** (2n ** 40n));
t(() => Math.abs(1n));
t(() => 1n < Symbol());
t(() => {
  let w = 1n;
  w += 1;
  return w;
});
t(() => {
  let w = 1;
  w *= 1n;
  return w;
});
t(() => {
  const p = { v: 1 };
  p.v -= 1n;
  return p.v;
});
t(() => {
  throw 5n;
});
