// T83 micro-benchmark: property access patterns shapes and shape-keyed inline caches should speed up. Prints
// checksums (stdout, compared with node) and times (stderr). Usage: tasks/T83-files/bench.sh <compiler>...
function time(name, f) {
  const t = performance.now();
  const r = f();
  console.error(`${name} ${(performance.now() - t).toFixed(0)} ms`);
  console.log(name, r);
}

// T78's probe (F334): `b.x += b.vx * dt` over 2 objects.
time("nbody", () => {
  const bodies = [{ x: 0, y: 0, vx: 1.5, vy: -0.5 }, { x: 10, y: 3, vx: -0.25, vy: 0.75 }];
  const dt = 0.001;
  for (let i = 0; i < 3000000; i++) {
    for (let j = 0; j < 2; j++) {
      const b = bodies[j];
      b.x += b.vx * dt;
      b.y += b.vy * dt;
    }
  }
  return Math.round(bodies[0].x * 1000) + Math.round(bodies[1].y * 1000);
});

// Objects from the same literal, built and read in a loop.
time("literals", () => {
  let sum = 0;
  for (let i = 0; i < 300000; i++) {
    const p = { id: i, kind: 1, a: i & 7, b: 2, c: 3, d: i & 3 };
    sum += p.id + p.kind + p.a + p.b + p.c + p.d;
  }
  return sum;
});

// Method calls through a class prototype (two levels) and instance fields.
class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }
  norm1() {
    return Math.abs(this.x) + Math.abs(this.y);
  }
}
class Point3 extends Point {
  constructor(x, y, z) {
    super(x, y);
    this.z = z;
  }
  sum() {
    return this.norm1() + this.z;
  }
}
time("methods", () => {
  const ps = [];
  for (let i = 0; i < 100; i++) ps.push(new Point3(i, -i, i & 3));
  let sum = 0;
  for (let k = 0; k < 5000; k++) for (let i = 0; i < 100; i++) sum += ps[i].sum();
  return sum;
});

// JSON.parse results read by key.
time("json", () => {
  const text = JSON.stringify(
    Array.from({ length: 200 }, (_, i) => ({ id: i, name: "n" + i, value: i * 2, ok: i % 2 === 0 })),
  );
  let sum = 0;
  for (let k = 0; k < 20; k++) {
    const xs = JSON.parse(text);
    for (let r = 0; r < 50; r++)
      for (let i = 0; i < xs.length; i++) {
        const x = xs[i];
        sum += x.id + x.value + (x.ok ? 1 : 0) + x.name.length;
      }
  }
  return sum;
});

// A polymorphic site: 4 shapes with `v` at different positions.
time("polymorphic", () => {
  const os = [{ v: 1 }, { a: 0, v: 2 }, { a: 0, b: 0, v: 3 }, { a: 0, b: 0, c: 0, v: 4 }];
  let sum = 0;
  for (let i = 0; i < 2000000; i++) sum += os[i & 3].v;
  return sum;
});

// An object that goes to dictionary mode (a delete, then many keys).
time("dictionary", () => {
  const o = { a: 1, b: 2, gone: 3 };
  delete o.gone;
  for (let i = 0; i < 40; i++) o["k" + i] = i;
  let sum = 0;
  for (let i = 0; i < 1000000; i++) {
    sum += o.a + o.k5 + o.k39;
    o.b = i;
  }
  return sum + o.b;
});
