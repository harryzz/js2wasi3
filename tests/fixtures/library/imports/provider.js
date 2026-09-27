// Implements interface `acme:demo/host` (jco's shape: an object named after the interface).
function add(a, b) {
  return a + b;
}

export const host = {
  add,
  neg(x) {
    return -x;
  },
  wide(x) {
    return x * 2;
  },
  half(x) {
    return x / 2;
  },
  initial(s) {
    return s;
  },
  greet: function (name) {
    return `hello ${name}`;
  },
  isEven(n) {
    return n % 2 === 0;
  },
  shift(p, by) {
    return Array.from(by, (d) => ({ x: p.x + d, y: p.y - d }));
  },
  bytes(n) {
    const b = new Uint8Array(n);
    for (let i = 0; i < n; i++) b[i] = i * 100;
    return b;
  },
  maybe(n) {
    return n > 0 ? "x".repeat(n) : undefined;
  },
  pair(a, b) {
    return [b + 1, a.toUpperCase()];
  },
  paint(c, p) {
    return `${c}:${p.read}:${p.writeAll}`;
  },
  area(s) {
    return s.tag === "circle" ? 3 * s.val * s.val : 0;
  },
  parse(s) {
    const n = Number(s);
    if (Number.isNaN(n)) throw `not a number: ${s}`;
    return n;
  },
  check(ok) {
    if (!ok) throw new Error("no");
  },
  countMany(a, b) {
    return a.reduce((x, y) => x + y, 0) + b.length;
  },
  log(msg) {},
};
