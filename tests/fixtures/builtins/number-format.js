// T25 slice 4: Number.prototype.toExponential / toPrecision (F88): exact decimal expansion, ties up.

const xs = [0, -0, 1, -1, 0.5, 1.5, 2.5, 123.456, 0.000001234, 1e-7, 1e21, 123456789012345680000, 5e-324, 1.7976931348623157e308, 0.1, 0.3, 1 / 3, 25, 1.005, 999.9999, -0.00000123];
for (const x of xs) {
  const out = [x.toExponential(), x.toPrecision()];
  for (const d of [0, 1, 2, 5, 20]) out.push(x.toExponential(d));
  for (const p of [1, 2, 3, 7, 21, 100]) out.push(x.toPrecision(p));
  console.log(out.join(" "));
}
console.log(NaN.toExponential(200), Infinity.toPrecision(0), (-0).toExponential(2), (-0).toPrecision(3), (0).toExponential());
const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.constructor.name, e.message); } };
t(() => (1).toExponential(101)); t(() => (1).toPrecision(0)); t(() => (1).toExponential(-1)); t(() => Number.prototype.toPrecision.call("x", 2));
// A deterministic sweep (LCG) over magnitudes 1e-30..1e30 (no `**`: pow differs, F77).
let seed = 12345;
const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const lines = [];
for (let i = 0; i < 300; i++) {
  let x = rnd() - 0.5;
  const k = Math.floor(rnd() * 60 - 30);
  for (let j = 0; j < k; j++) x *= 10;
  for (let j = 0; j > k; j--) x /= 10;
  const p = 1 + Math.floor(rnd() * 25), f = Math.floor(rnd() * 25);
  lines.push(x.toPrecision(p) + " " + x.toExponential(f));
}
console.log(lines.join("\n"));
