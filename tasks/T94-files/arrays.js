"use strict";
// T95 check: array element reads with a computed index (`a[i]`) and string-keyed reads on arrays.
const n = Number(process.argv[2] || 3000000);
const a = []; for (let i = 0; i < 1024; i++) a.push(i);
let t = Date.now(), s = 0;
for (let i = 0; i < n; i++) s += a[i & 1023];
console.log("index", s, Date.now() - t, "ms");
const m = []; for (let i = 0; i < 64; i++) m.push([i, i + 1]);
t = Date.now(); s = 0;
for (let i = 0; i < n; i++) { const p = m[i & 63]; s += p[0] + p[1]; }
console.log("nested", s, Date.now() - t, "ms");
