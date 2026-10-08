"use strict";
// T92 methods-only check (12 alternating runs per build).
class P { constructor(x, y) { this.x = x; this.y = y; } len() { return this.x + this.y; } }
const ps = []; for (let i = 0; i < 64; i++) ps.push(new P(i, i + 1));
let s = 0; const t = Date.now();
for (let i = 0; i < 4000000; i++) s += ps[i & 63].len();
console.log(s, Date.now() - t);
