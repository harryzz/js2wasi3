// T75 micro-benchmark: conversions, operators and number boxing on jsvals (values read from objects/arrays):
// === against constants, + / < / - on small integers, o[i] on plain objects, numbers to strings, JSON.stringify
// of numbers. Prints checksums (stdout, compared with node) and times (stderr), one word per test name.
function time(name, f) {
  const t = performance.now();
  const r = f();
  console.error(`${name} ${(performance.now() - t).toFixed(0)} ms`);
  console.log(name, r);
}

const kinds = ["numeric", "binary", "enum", "text", "composite", "list"];
const items = [];
for (let i = 0; i < 1000; i++) {
  const o = { type: kinds[i % 6], a: i, b: i % 7, v: undefined };
  if (i % 3 === 0) o.v = null;
  if (i % 3 === 1) o.v = i;
  if (i % 5 === 0) o.v = "x";
  items.push(o);
}

time("eqconst", () => {
  let n = 0;
  for (let r = 0; r < 300; r++)
    for (let i = 0; i < items.length; i++) {
      const o = items[i];
      if (o.v === undefined) n++;
      if (o.v === null) n += 2;
      if (o.v !== null && o.v !== undefined) n += 3;
      if (o.type === "numeric") n += 4;
      if (o.type !== "enum") n += 5;
      if (o.v === "x") n += 6;
    }
  return n;
});

time("intops", () => {
  let n = 0;
  for (let r = 0; r < 300; r++)
    for (let i = 0; i < items.length; i++) {
      const o = items[i];
      o.c = o.a + o.b;
      o.d = o.a - o.b;
      if (o.b < o.a) n++;
      if (o.c <= o.d) n++;
      if (o.a > 500) n++;
      o.e = o.c + 1;
    }
  for (let i = 0; i < items.length; i++) n += items[i].c + items[i].d + items[i].e;
  return n;
});

time("objindex", () => {
  const m = {};
  for (let i = 0; i < 200; i++) m[i] = i * 2;
  let n = 0;
  for (let r = 0; r < 2000; r++)
    for (let i = 0; i < 200; i++) {
      n += m[i];
      m[i] = m[i] + 1;
    }
  return n;
});

time("numstr", () => {
  let n = 0;
  for (let r = 0; r < 200; r++)
    for (let i = 0; i < items.length; i++) {
      const s = "" + items[i].a;
      const t = `${items[i].b}:${items[i].a}`;
      n += s.length + t.length + String(items[i].a).length;
    }
  return n;
});

time("stringify", () => {
  const rows = [];
  for (let i = 0; i < 100; i++) rows.push({ id: i, min: -40, max: 125, step: 0.5, n: [i, i + 1, i + 2] });
  let n = 0;
  for (let r = 0; r < 300; r++) n += JSON.stringify(rows).length;
  return n;
});

time("truthy", () => {
  let n = 0;
  for (let r = 0; r < 300; r++)
    for (let i = 0; i < items.length; i++) {
      const o = items[i];
      if (o.v) n++;
      if (!o.b) n++;
      if (o.type && o.a) n++;
      n += typeof o.v === "number" ? 1 : 0;
      n += typeof o.type === "string" ? 1 : 0;
    }
  return n;
});
