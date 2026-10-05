// T80 micro-benchmark (a sanity check, not a claim): node:buffer's UTF-8 paths on MQTT-sized strings.
// Run: node utf8-bench.js; js2wasi3 build utf8-bench.js -o b.wasm && wasmtime run -Sp3 b.wasm
const { Buffer } = require('node:buffer');

const ascii = JSON.stringify({
  linkquality: 120, state: 'ON', brightness: 254, color_temp: 370,
  update: { state: 'idle', installed_version: 587765297, latest_version: 587765297 },
  last_seen: '2026-10-05T10:11:12.345Z', device: { friendlyName: 'living_room_lamp', model: 'LED1545G12' },
});
const mixed = JSON.stringify({
  name: 'Wohnzimmer Lämpchen – große Küche', note: 'température 21,5 °C, Влажность 45 %',
  emoji: 'ok 👍 🏠', last_seen: '2026-10-05T10:11:12.345Z',
});
const N = 20000;

function time(label, f) {
  const t0 = Date.now();
  let sink = 0;
  for (let i = 0; i < N; i++) sink += f(i);
  console.log(`${label}: ${Date.now() - t0} ms (${sink})`);
}

for (const [name, s] of [['ascii', ascii], ['mixed', mixed]]) {
  const buf = Buffer.alloc(1024);
  const enc = Buffer.from(s);
  console.log(`${name}: ${s.length} units, ${enc.length} bytes`);
  time(`${name} byteLength`, () => Buffer.byteLength(s));
  time(`${name} Buffer.from`, () => Buffer.from(s).length);
  time(`${name} write`, () => buf.write(s, 7));
  time(`${name} toString`, () => enc.toString().length);
}
