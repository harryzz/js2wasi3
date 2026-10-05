// T81 micro-benchmark (a sanity check, not a claim): RegExp Symbol.match/replace/split/search on
// log-line-like strings. Run: node regex-bench.js; js2wasi3 build regex-bench.js -o b.wasm && wasmtime run -Sp3 b.wasm
const line = '[2026-10-05 10:11:12] info: \tz2m: MQTT publish: topic \'zigbee2mqtt/living_room_lamp\', ' +
  'payload \'{"brightness":254,"color_temp":370,"linkquality":120,"state":"ON"}\'';
const N = 20000;

function time(label, f) {
  const t0 = Date.now();
  let sink = 0;
  for (let i = 0; i < N; i++) sink += f(i);
  console.log(`${label}: ${Date.now() - t0} ms (${sink})`);
}

time('replace /g string', () => line.replace(/"/g, "'").length);
time('replace /g $&', () => line.replace(/[{}]/g, '<$&>').length);
time('replace /g fn', () => line.replace(/\d+/g, (m) => m.length).length);
time('replace once', () => line.replace(/^\[[^\]]*\] /, '').length);
time('match /g', () => line.match(/\w+/g).length);
time('match', () => line.match(/topic '([^']*)'/)[1].length);
time('split', () => line.split(/[,:]\s*/).length);
time('search', () => line.search(/payload/));
time('test', () => (/info/.test(line) ? 1 : 0));
