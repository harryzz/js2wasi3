// T142: element keys that are locals only holding integers (stepped by constants, copies of each other).
function counters() {
  const a = [10, 20, 30, 40], out = [];
  let i = 0;
  while (i < 6) { out.push(a[i]); i += 1; }           // past the end
  i = 3;
  while (i > -3) { out.push(a[i]); i -= 1; }          // negative
  let j = i;                                          // copy of an integer local
  j += 2;
  out.push(a[j], a[j - 1], a[j + 100]);
  a[j] = "j"; a[i + 100] = "far";
  return [out.join(), a.join(), a.length];
}
console.log(counters());

// Large steps and values past 2^31 saturate instead of wrapping (index 4294967297 is not index 1).
function big() {
  const a = [1, 2, 3], out = [];
  let k = 0;
  k += 2147483647; out.push(a[k]);
  k += 2;          out.push(a[k]);
  k += 2147483648; out.push(a[k]);
  k += 4294967296; out.push(a[k]);
  k = 4294967297;  out.push(a[k], a[k - 4294967296]);
  a[k] = "set";
  out.push(a.length, a[1]);
  let m = -2147483648; m -= 1; out.push(a[m]);
  return out;
}
console.log(big());

// Not integer locals: fractions, NaN, -0, strings.
function others(step) {
  const a = [5, 6, 7], out = [];
  let x = 0;
  for (let n = 0; n < 4; n++) { out.push(a[x]); x += step; }
  let y = 1; y /= 2; out.push(a[y]);                  // 0.5
  let z = 0; z = -z; out.push(a[z]);                  // -0
  let w = 1; w = w * 0 / 0; out.push(a[w]);           // NaN
  let u = 0; u = u + "1"; out.push(a[u]);             // a string
  return out;
}
console.log(others(1), others(0.5), others(-1));

// A counter reset to a non-integer inside a loop.
function mixedWrites(flag) {
  const a = [1, 2, 3, 4];
  let i = 0, s = 0;
  for (let n = 0; n < 6; n++) {
    s += a[i] === undefined ? 100 : a[i];
    i = flag && n === 2 ? 1.5 : i + 1;
  }
  return s;
}
console.log(mixedWrites(false), mixedWrites(true));

// Writes through integer keys, in nested loops, with the array replaced meanwhile.
function nested() {
  const m = [[0, 0, 0], [0, 0, 0]];
  for (let r = 0; r < 2; r++) for (let c = 0; c < 3; c++) m[r][c] = r * 10 + c;
  const flat = [];
  for (let r = 0; r < 3; r++) for (let c = 0; c < 4; c++) flat[r * 4 + c] = r + c;
  return [JSON.stringify(m), flat.join(), flat.length];
}
console.log(nested());

// Typed arrays and strings as receivers with integer keys.
function receivers() {
  const t = new Int16Array(4), s = "hello", out = [];
  for (let i = 0; i < 6; i++) { t[i] = i * 1000; out.push(t[i], s[i]); }
  return out;
}
console.log(receivers());
