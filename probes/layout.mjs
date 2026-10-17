// T000 probe: runs probes/layout.wat's exports on node's V8. Usage: node probes/layout.mjs out/layout.wasm [n]
import { readFileSync } from 'node:fs';
const [, , file, arg] = process.argv;
const n = +(arg ?? 20000000);
const { instance } = await WebAssembly.instantiate(readFileSync(file));
for (const name of Object.keys(instance.exports).filter(k => k.startsWith('run_'))) {
  const t = performance.now();
  const s = instance.exports[name](n);
  console.log(name, s, Math.round(performance.now() - t), 'ms');
}
