// T100 probe: node gen.mjs <n> globals|array > x.js. The same allocation-heavy loop with n extra strings, either
// as n string literals (n constant globals: GC roots) or built at run time into one array (one root).
const [n, mode] = [Number(process.argv[2]), process.argv[3]];
let extra = "";
if (mode === "globals" && n > 0) {
  const lits = [];
  for (let i = 0; i < n; i++) lits.push(JSON.stringify("k" + i + "_" + "x".repeat(i % 7)));
  extra = `function never() { return [${lits.join(",")}]; }\nif (process.argv.length > 99) console.log(never());\n`;
} else if (mode === "array" && n > 0) {
  extra = `const keep = [];\nfor (let i = 0; i < ${n}; i++) keep.push("k" + i + "_" + "x".repeat(i % 7));\n`;
}
console.log(`"use strict";
${extra}function loop() {
  let s = 0;
  for (let i = 0; i < 3000000; i++) {
    const o = { a: i, b: [i, i + 1] };
    s += o.b[1] - o.a;
  }
  return s;
}
const t = Date.now();
const r = loop();
console.log(r, Date.now() - t, "ms");`);
