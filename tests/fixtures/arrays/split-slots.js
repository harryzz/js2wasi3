// T134: split number slots: variables and parameters the type analysis sees only numbers stored in keep them
// unboxed, and still hold anything else that reaches them (a hole, a missing argument, a value from outside).

function sum(a, n) {
  let s = 0;
  for (let i = 0; i < n; i++) {
    let t = a[i]; // past the end: undefined
    s = s + t;
  }
  return s;
}
console.log(sum([1, 2, 3], 3), sum([1, 2, 3], 4), sum([1.5, 2.25], 2));

// A parameter that is sometimes missing.
function scale(x, k) {
  return x * k;
}
console.log(scale(2, 3), scale(2));

// Swaps through a local, keys from locals, `!=`/`==`/`===` against numbers.
function flips(p) {
  let f = 0;
  let k = p[0];
  while (k != 0) {
    let i = 0;
    while (2 * i < k) {
      let t = p[i];
      p[i] = p[k - i];
      p[k - i] = t;
      i += 1;
    }
    k = p[0];
    f += 1;
  }
  return [f, k == 0, k === 0, k !== 1];
}
console.log(flips([3, 1, 0, 2]), flips([0]));

// Values from outside the analysis's view (JSON, a function called through a value).
function addAll(a) {
  let acc = 0;
  for (let j = 0; j < a.length; j++) {
    let v = a[j];
    acc = acc + v;
  }
  return acc;
}
console.log(addAll(JSON.parse("[1, 2, 3]")), addAll(JSON.parse('[1, "2", 3]')), addAll(JSON.parse("[1, null, {}]")));
const call = [addAll][0];
console.log(call(["a", 1]), call([0.1, 0.2]));

// Element keys and values from split slots, in number-mode and generic arrays and objects.
function copy(src, dst, n) {
  for (let i = 0; i < n; i++) {
    let j = n - 1 - i;
    let v = src[j];
    dst[i] = v;
    dst[j + n] = v;
  }
  return dst;
}
console.log(copy([1, 2, 3], [], 3), copy([1.5, "x", null], [], 3), copy({ 0: "a", 1: "b" }, {}, 2));

// Counting down, `--n`/`i++` on parameters, 28-bit digits as in crypto's `am3`.
function am(xs, i, x, w, j, c, n) {
  const xl = x & 0x3fff, xh = x >> 14;
  while (--n >= 0) {
    let l = xs[i] & 0x3fff;
    const h = xs[i++] >> 14;
    const m = xh * l + h * xl;
    l = xl * l + ((m & 0x3fff) << 14) + w[j] + c;
    c = (l >> 28) + (m >> 14) + xh * h;
    w[j++] = l & 0xfffffff;
  }
  return c;
}
const w = [5, 6, 7, 8];
console.log(am([123456789, 98765432, 13579], 0, 24680, w, 0, 0, 3), w);
console.log(am([1, 2], 0, 3, ["7", 8], 0, "1", 2));

// `-0`, NaN and large numbers stay what they are.
function keep(x) {
  let y = x;
  return [Object.is(y, -0), y !== y, y + 1];
}
console.log(keep(-0), keep(NaN), keep(2 ** 40), keep(0.5));

// Calls the analysis may not follow: `apply`, `call`, a callback.
function twice(x) {
  let y = x;
  y = y + y;
  return y;
}
console.log(twice(2), twice(0.25), twice.apply(null, ["ab"]), twice.call(null, [1]), ["c", 3].map(twice));

// A parameter given an element past the end (the analysis says number; it's undefined).
function half(v, d) {
  let r = v;
  while (d-- > 0) r = r / 2;
  return [r, v, v == undefined, typeof v];
}
const nums = [8, 6];
console.log(half(nums[0], 2), half(nums[5], 1), half(nums[1] + 1, 0));
