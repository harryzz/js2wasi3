"use strict";
// T92 plain-object check: property reads/writes on plain objects, arrays, and method calls on a class instance.
const n = Number(process.argv[2] || 3000000);
class P { constructor(x, y) { this.x = x; this.y = y; } len() { return this.x + this.y; } }
function reads() {
  const os = [];
  for (let i = 0; i < 64; i++) os.push({ a: i, b: i * 2, c: "s" + i });
  let s = 0;
  for (let i = 0; i < n; i++) { const o = os[i & 63]; s += o.a + o.b; o.a = (o.a + 1) & 1023; }
  return s;
}
function methods() {
  const ps = [];
  for (let i = 0; i < 64; i++) ps.push(new P(i, i + 1));
  let s = 0;
  for (let i = 0; i < n; i++) s += ps[i & 63].len();
  return s;
}
function arrays() {
  const a = [1, 2, 3, 4, 5, 6, 7, 8];
  let s = 0;
  for (let i = 0; i < n; i++) s += a.length + a[i & 7];
  return s;
}
for (const [name, f] of Object.entries({ reads, methods, arrays })) {
  const t = Date.now(); const r = f(); console.log(name, r, Date.now() - t, "ms");
}
