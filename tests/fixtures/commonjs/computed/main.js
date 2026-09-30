// Computed require (T54 slice 4): the files T41's import() patterns match are in the build and
// looked up when the call runs (node's LOAD_AS_FILE: .js/.json added, index.js).
const names = ['lamp', 'plug', 'sensor'];
const devices = names.map((name) => require('./devices/' + name));
console.log(devices.map((d) => d.model));
for (const lang of ['en', 'de']) console.log(require(`./locales/${lang}.json`).hello);
const DIR = './lib/';
const pick = (fast) => require(fast ? './lib/fast' : './lib/slow.js');
console.log(pick(true), pick(false), require(DIR + 'index.js') === require('./lib'));
console.log(require('./devices/' + 'lamp') === devices[0], require.resolve('./devices/' + 'plug').endsWith('/computed/devices/plug.js'));
try {
  require('./devices/' + 'missing');
} catch (e) {
  console.log(e.code, e.message.split('\n')[0]);
}
