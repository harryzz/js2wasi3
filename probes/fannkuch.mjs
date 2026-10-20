// T140 probe on node's V8: node probes/fannkuch.mjs [n] [runs]; needs probes/out/fannkuch-m{0,1,2}.wasm (probes/fannkuch-run.sh).
import { readFileSync } from 'node:fs';
const n = +(process.argv[2] ?? 10), runs = +(process.argv[3] ?? 5);
for (const m of [0, 1, 2, 3, 4, 5, 6]) {
  const { instance } = await WebAssembly.instantiate(readFileSync(`probes/out/fannkuch-m${m}.wasm`));
  let best = 1e9, res;
  for (let r = 0; r < runs; r++) { const t = performance.now(); res = instance.exports.run(n); best = Math.min(best, performance.now() - t); }
  console.log(`M${m}: ${Math.round(best)} ms  result: ${res}`);
}
