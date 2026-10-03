// A live set of N objects, then short-lived garbage: GC cost vs heap size.
const N = Number(process.env.N ?? 200000);
const live = [];
for (let i = 0; i < N; i++) live.push({ id: i, name: "n" + i, tags: [i, i + 1] });
const t0 = Date.now();
let sum = 0;
for (let r = 0; r < 2000000; r++) {
  const tmp = { a: r, b: [r, r * 2], s: "x" + (r & 1023) };
  sum += tmp.b[1] + tmp.s.length;
}
console.log(live.length, sum, "garbage loop ms", Date.now() - t0);
