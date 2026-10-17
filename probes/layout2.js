// T000 probe 2: probes/layout.js plus what the language needs around the fast path: a property added to some
// objects (b.tag), generic code reading them (Object.keys), and a constructor argument of unknown type (cfg.half).
// probes/layout2.wat compiles it by hand. Run: node probes/layout2.js [n]
function Vec(x, y, z) { this.x = x; this.y = y; this.z = z; }
Vec.prototype.add = function (o) { return new Vec(this.x + o.x, this.y + o.y, this.z + o.z); };
Vec.prototype.dot = function (o) { return this.x * o.x + this.y * o.y + this.z * o.z; };
const cfg = { half: 0.5 };
function run(n) {
  const half = cfg.half;
  let a = new Vec(0, 0, 0), s = 0;
  for (let i = 0; i < n; i++) {
    const b = new Vec(i, half, 1);
    if (i % 1000 === 0) { b.tag = i; s += Object.keys(b).length; }
    a = a.add(b);
    s += a.dot(b);
  }
  return s;
}
const n = +(process.argv[2] ?? 20000000);
const t = Date.now();
const s = run(n);
console.log(s, Date.now() - t, 'ms');
