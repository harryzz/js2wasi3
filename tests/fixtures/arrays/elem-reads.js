// T108: `+` chains with element reads under a numeric consumer (`*`, `-`, `/`, comparisons) = node.

const log = [];

// Number mode (non-integers), generic arrays of i31s, mixed.
const x = [0.5, 1.25, 2.75, 3.5];
const ints = [1, 2, 3, 4];
let lastX = x[0];
let c = 0, l = 0, n = 1;
for (let i = 0; i < 3; i++) {
  lastX = (x[c] + 0.5 * (lastX + x[++c] + x[++l] + x[++n % 4])) * 0.25;
  log.push(lastX);
}
log.push((ints[0] + ints[1]) * 2, (x[1] + ints[2]) * 3, (ints[3] + x[0] + 1) / 2);

// Strings: concatenation, then ToNumber.
const s = ["1", "2", 3.5, "x"];
log.push((s[0] + s[1]) * 1, (s[2] + s[0]) * 1, (s[3] + x[0]) * 1, (x[0] + s[1]) - 0);

// Holes behind a prototype element; out of range; undefined.
const h = [1.5, , 3.5];
Array.prototype[1] = 10.25;
log.push((h[0] + h[1]) * 2);
delete Array.prototype[1];
log.push((h[0] + h[1]) * 2, (h[0] + h[7]) * 1, (h[2] + h[-1]) * 1);

// NaN, -0, Infinity.
const z = [-0, -0, NaN, Infinity, -Infinity, 0.5];
log.push(Object.is((z[0] + z[1]) * 1, -0), (z[0] + z[5]) * 1, (z[2] + z[5]) * 1, (z[3] + z[4]) * 1);
log.push((z[3] + z[5]) < 1, (z[0] + z[1]) <= 0, (x[0] + x[1]) > 1.7, 2 >= (x[0] + ints[0]));

// Getters on elements count reads; valueOf on an object element runs once, after both operands.
let reads = 0;
const g = [0.5, 1.5];
Object.defineProperty(g, 0, { get() { reads++; log.push("get0"); return 2.5; } });
log.push((g[0] + g[1]) * 2, reads);
const o = [{ valueOf() { log.push("valueOf"); return 4.5; } }, 0.25];
log.push((o[0] + o[1]) * 2);

// Order of evaluation: a right operand that changes the array; the left operand's value is the one read.
const m = [1.5, 2.5, 3.5];
let k = 0;
function bump() { m[0] = 100.5; m.push("s"); return 1; }
log.push((m[k] + m[bump()]) * 2, m.length, (m[0] + m[3]) * 1);
// valueOf runs after both operands are evaluated (left object, right element changed by a call).
const p = [{ valueOf() { log.push("p.valueOf, p[1] = " + p[1]); return 1; } }, 0.5];
function setP() { p[1] = 7.5; return 1; }
log.push((p[0] + p[setP()]) * 1);
// A chain: the first `+` converts (valueOf) before the next operand is evaluated.
const q = [{ valueOf() { log.push("q0"); return 1; } }, 2.5, 3.5];
function third() { log.push("third"); return 2; }
log.push((q[0] + q[1] + q[third()]) * 1);

// Typed arrays and plain objects with numeric keys.
const f = new Float64Array([1.5, 2.25, 3.125]);
const u = new Uint8Array([200, 100]);
const pl = { 0: 1.5, 1: "2", 2: 4.25 };
log.push((f[0] + f[1]) * 2, (u[0] + u[1]) * 1, (pl[0] + pl[2]) * 2, (pl[0] + pl[1]) * 1);

// Non-array receivers and keys: strings, undefined index, a null array throws a TypeError.
const str = "123";
log.push((str[0] + str[1]) * 1, (x["1"] + x[1]) * 1);
try {
  const nul = null;
  log.push((x[0] + nul[0]) * 1);
} catch (e) {
  log.push(e.constructor.name);
}

// Comparisons with a chain on one side.
let cnt = 0;
for (let i = 0; i + 1 < x.length; i++) if (x[i] + x[i + 1] < 5) cnt++;
log.push(cnt);

console.log(log.join("\n"));
