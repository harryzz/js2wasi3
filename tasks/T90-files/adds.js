"use strict";
// T90 micro-benchmark: adding properties with constant keys from the same sites.
// Usage: node adds.js [case] [n]; case: ctor | literal | setter | frozen | all (default all).
const which = process.argv[2] || "all";
const n = Number(process.argv[3] || 1000000);

class Base {
  hello() {
    return 1;
  }
}
class Car extends Base {
  constructor(make, model, year) {
    super();
    this.make = make;
    this.model = model;
    this.year = year;
    this.miles = 0;
  }
}

function ctor() {
  let s = 0;
  for (let i = 0; i < n; i++) s += new Car("a", "b", i).year;
  return s;
}

function literal() {
  let s = 0;
  for (let i = 0; i < n; i++) {
    const o = {};
    o.a = i;
    o.b = i + 1;
    o.c = i + 2;
    o.d = i + 3;
    s += o.d;
  }
  return s;
}

// A setter added to the prototype halfway: the second half calls it.
function setter() {
  const proto = {};
  let calls = 0;
  let s = 0;
  for (let i = 0; i < n; i++) {
    if (i === n / 2) {
      Object.defineProperty(proto, "v", {
        set(x) {
          calls++;
        },
      });
    }
    const o = Object.create(proto);
    o.v = i;
    s += Object.keys(o).length;
  }
  return s + calls;
}

// A frozen receiver: every add throws.
function frozen() {
  const f = Object.freeze({});
  let s = 0;
  for (let i = 0; i < n / 10; i++) {
    try {
      f.v = i;
    } catch (e) {
      s++;
    }
  }
  return s;
}

const cases = { ctor, literal, setter, frozen };
for (const name of which === "all" ? Object.keys(cases) : [which]) {
  const t = Date.now();
  const r = cases[name]();
  console.log(name, r, Date.now() - t, "ms");
}
