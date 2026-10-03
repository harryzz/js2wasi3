// T74 micro-benchmark: the property access patterns of zigbee-herdsman-converters' exposes (builder chains on
// class instances, prototype methods, object literals, objects past 8 keys, JSON.stringify). Prints checksums
// (stdout, compared with node) and times (stderr).
class Base {
  constructor(name, label) {
    this.type = "numeric";
    this.name = name;
    this.label = label;
    this.access = 1;
    this.property = name;
  }
  withUnit(u) {
    this.unit = u;
    return this;
  }
  withDescription(d) {
    this.description = d;
    return this;
  }
  withValueMin(v) {
    this.value_min = v;
    return this;
  }
  withValueMax(v) {
    this.value_max = v;
    return this;
  }
  withCategory(c) {
    this.category = c;
    return this;
  }
}
class Numeric extends Base {
  withValueStep(v) {
    this.value_step = v;
    return this;
  }
}

function time(name, f) {
  const t = performance.now();
  const r = f();
  console.error(`${name} ${(performance.now() - t).toFixed(0)} ms`);
  console.log(name, r);
}

const N = 20000;
time("builders", () => {
  let sum = 0;
  for (let i = 0; i < N; i++) {
    const e = new Numeric("temperature", "Temperature")
      .withUnit("°C")
      .withDescription("Measured temperature value")
      .withValueMin(-40)
      .withValueMax(125)
      .withValueStep(0.1)
      .withCategory("diagnostic");
    sum += e.value_max - e.value_min + e.access + e.name.length + e.unit.length + e.category.length;
  }
  return sum;
});

time("literals", () => {
  let sum = 0;
  for (let i = 0; i < N * 5; i++) {
    const o = { a: i, b: 2, c: 3, d: { x: 1, y: 2 } };
    sum += o.a + o.b + o.c + o.d.x + o.d.y;
  }
  return sum;
});

time("big objects", () => {
  const o = {};
  for (let k = 0; k < 40; k++) o["key" + k] = k;
  let sum = 0;
  for (let i = 0; i < N * 5; i++) sum += o.key3 + o.key17 + o.key39 + o.key0;
  return sum;
});

time("prototype methods", () => {
  const xs = [];
  for (let i = 0; i < 100; i++) xs.push(new Numeric("n" + i, "N"));
  let sum = 0;
  for (let r = 0; r < N / 10; r++) for (const x of xs) sum += x.withValueMin(r).value_min;
  return sum;
});

time("stringify", () => {
  const list = [];
  for (let i = 0; i < 50; i++) {
    list.push(new Numeric("temperature" + i, "Temperature").withUnit("°C").withValueMin(-40).withValueMax(125));
  }
  let n = 0;
  for (let r = 0; r < 200; r++) n += JSON.stringify(list).length;
  return n;
});
