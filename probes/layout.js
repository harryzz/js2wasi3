// T000 probe: the kernel that probes/layout.wat compiles by hand (constructor, field reads, prototype methods,
// double arithmetic, two allocations per iteration). Run: node probes/layout.js [n]
function Vec(x, y, z) { this.x = x; this.y = y; this.z = z; }
Vec.prototype.add = function (o) { return new Vec(this.x + o.x, this.y + o.y, this.z + o.z); };
Vec.prototype.dot = function (o) { return this.x * o.x + this.y * o.y + this.z * o.z; };
function run(n) {
  let a = new Vec(0, 0, 0), s = 0;
  for (let i = 0; i < n; i++) {
    const b = new Vec(i, 0.5, 1);
    a = a.add(b);
    s += a.dot(b);
  }
  return s;
}
const n = +(process.argv[2] ?? 20000000);
const t = Date.now();
const s = run(n);
console.log(s, Date.now() - t, 'ms');
