// await inside loops, conditions, try/catch/finally, expressions, methods and closures.
const log = (...a) => console.log(a.join(" "));
const delay = (v) => new Promise((r) => queueMicrotask(() => r(v)));
const fail = (m) => Promise.reject(new Error(m));

async function loops() {
  let sum = 0;
  for (let i = 0; i < 5; i++) sum += await delay(i);
  let j = 0;
  while ((await delay(j)) < 3) j++;
  const out = [];
  for (const x of [1, 2, 3]) out.push(await delay(x * 10));
  do { j--; } while (await delay(j > 0));
  label: for (let a = 0; a < 3; a++) {
    for (let b = 0; b < 3; b++) {
      if (await delay(b === 1)) continue label;
      if (a === 2) break label;
      out.push(`${a}${b}`);
    }
  }
  return `sum=${sum} j=${j} out=${out.join("/")}`;
}

async function errors() {
  const seen = [];
  try {
    await fail("first");
    seen.push("not reached");
  } catch (e) {
    seen.push("caught " + e.message);
    try {
      await fail("inner");
    } catch (e2) {
      seen.push("caught " + e2.message + " in catch");
    } finally {
      seen.push("inner finally " + (await delay("x")));
    }
  } finally {
    seen.push("finally");
  }
  for (let i = 0; i < 3; i++) {
    try {
      if (i === 1) continue;
      if (i === 2) break;
      await delay(i);
    } finally {
      seen.push("loop finally " + i);
    }
  }
  return seen.join("; ");
}

async function returnsInFinally() {
  try {
    return await delay("from try");
  } finally {
    log("finally runs before resolve");
  }
}

async function throwsAfterAwait() {
  await delay(0);
  throw new RangeError("late");
}

async function exprs(o) {
  const a = (await delay(1)) + (await delay(2)) * (await delay(3));
  const b = o.x ? await delay("yes") : await delay("no");
  const c = [await delay("p"), await delay("q")].join("");
  const d = { k: await delay("v") };
  const e = (await delay(false)) || (await delay("or"));
  const f = `${await delay("t")}${await delay("l")}`;
  if ((await delay(2)) === 1) return "wrong";
  return [a, b, c, d.k, e, f].join(",");
}

class Counter {
  #n = 0;
  constructor(name) { this.name = name; }
  async add(k) {
    this.#n += await delay(k);
    return this;
  }
  get n() { return this.#n; }
  static async make(name) { return new Counter(await delay(name)); }
}

const obj = {
  base: 100,
  async m(x) { return this.base + (await delay(x)); },
  arrow() { return async () => this.base * 2; },
};

async function main() {
  log(await loops());
  log(await errors());
  log(await returnsInFinally());
  try { await throwsAfterAwait(); } catch (e) { log(e.name, e.message); }
  log(await exprs({ x: 1 }));
  const c = await Counter.make("ctr");
  await c.add(2);
  await (await c.add(3)).add(4);
  log(c.name, c.n);
  log(await obj.m(5), await obj.arrow()());
  let captured = 0;
  const bump = async () => { captured += await delay(1); };
  await Promise.all([bump(), bump(), bump()]);
  log("captured", captured);
  const results = await Promise.allSettled([throwsAfterAwait(), delay("fine")]);
  log(results[0].status, results[0].reason.message, results[1].status, results[1].value);
  log(typeof main, main.name, main.length, (async (a, b) => {}).length);
  console.log(async function () {}, async () => {}, obj.m);
}
main().then(() => log("main done"));
