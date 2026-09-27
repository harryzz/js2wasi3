// Object.defineProperty on array elements (T32): accessor elements, element attributes, and
// everything that reads elements (native methods, iteration, printing, keys, delete, JSON).
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.constructor.name + ": " + e.message);
  }
};

// Accessor elements.
const a = [1, 2, 3];
let reads = 0;
Object.defineProperty(a, "1", {
  get() {
    reads++;
    return "g" + this.length;
  },
  configurable: true,
});
t(() => a[1]);
t(() => a["1"]);
t(() => {
  a[1] = 5;
});
t(() => a);
t(() => Object.keys(a));
t(() => a.join("-"));
t(() => String(a));
t(() => [...a]);
t(() => {
  const out = [];
  for (const x of a) out.push(x);
  return out;
});
t(() => JSON.stringify(a));
t(() => ({ ...a }));
t(() => Object.values(a));
t(() => Object.entries(a));
t(() => a.map((x) => x));
t(() => a.indexOf("g3"));
t(() => a.includes("g3"));
t(() => a.slice(0, 2));
t(() => 1 in a);
t(() => a.hasOwnProperty(1));
t(() => Object.getOwnPropertyDescriptor(a, "1"));
console.log("reads", reads);

// A setter element.
const s = [0, 1];
Object.defineProperty(s, "0", {
  set(v) {
    console.log("set", v, this === s);
  },
  configurable: true,
});
t(() => s);
t(() => {
  s[0] = 3;
  return s[0];
});
const both = [];
Object.defineProperty(both, "0", { get: () => 1, set: (v) => {}, enumerable: true });
t(() => both);
t(() => both.length);

// Redefining: accessor back to data, data attributes.
Object.defineProperty(a, "1", { value: "data", writable: true });
t(() => a);
t(() => Object.getOwnPropertyDescriptor(a, "1"));

// Read-only, non-enumerable, non-configurable elements.
const r = [1, 2, 3, 4];
Object.defineProperty(r, "0", { value: 9, writable: false });
t(() => {
  r[0] = 1;
});
t(() => r[0]);
t(() => delete r[0]);
t(() => r);
Object.defineProperty(r, "1", { enumerable: false });
t(() => r);
t(() => Object.keys(r));
t(() => Object.getOwnPropertyNames(r));
t(() => {
  const ks = [];
  for (const k in r) ks.push(k);
  return ks;
});
t(() => JSON.stringify(r));
t(() => r.propertyIsEnumerable(1));
Object.defineProperty(r, "3", { value: 4, configurable: false });
t(() => delete r[3]);
t(() => r.pop());
t(() => r);
t(() => Object.defineProperty(r, "3", { value: 5 }));
t(() => Object.defineProperty(r, "3", { get() {} }));
t(() => Object.defineProperty(r, "3", { value: 4, writable: false }));
t(() => Object.getOwnPropertyDescriptor(r, "3"));
t(() => Object.defineProperty(r, "3", { value: 5 }));

// New elements through defineProperty: absent attributes are false; holes fill in.
const n = [];
Object.defineProperty(n, "2", { value: "x" });
t(() => n);
t(() => n.length);
t(() => Object.getOwnPropertyDescriptor(n, "2"));
t(() => Object.keys(n));
t(() => {
  n[2] = "y";
});

// Getters that change the array while it is read.
const m = [1, 2, 3, 4];
Object.defineProperty(m, "1", {
  get() {
    m.length = 2;
    return "cut";
  },
});
t(() => m.join());
t(() => m);

// Frozen and sealed arrays with accessor elements.
const f = [1];
Object.defineProperty(f, "0", { get: () => "fg", set: (v) => console.log("fset", v), configurable: true });
Object.freeze(f);
t(() => {
  f[0] = 2;
  return f[0];
});
t(() => Object.getOwnPropertyDescriptor(f, "0"));
t(() => Object.isFrozen(f));
