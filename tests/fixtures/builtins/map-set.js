// Map and Set (T25 slice 2): SameValueZero keys, insertion order, iteration while mutating,
// forEach, constructors, errors and console.log.

function show(label, f) {
  try {
    console.log(label, f());
  } catch (e) {
    console.log(label, e.name + ": " + e.message);
  }
}

// Keys: SameValueZero.
const m = new Map();
const o1 = {};
const o2 = {};
const sym = Symbol("s");
m.set(NaN, "nan").set(0, "zero").set("1", "string one").set(1, "number one");
m.set(o1, "o1").set(o2, "o2").set(sym, "sym").set(null, "null").set(undefined, "undef");
m.set(true, "true").set("", "empty");
console.log(m.get(NaN), m.get(0 / 0), m.get(-0), m.get(0), m.get("1"), m.get(1), m.get(1.0));
console.log(m.get(o1), m.get(o2), m.get({}), m.get(sym), m.get(Symbol("s")));
console.log(m.get(null), m.get(undefined), m.get(true), m.get(false), m.get(""), m.get("x"));
console.log(m.get("ab" + "c"), m.set("abc", 3).get("a" + "bc"), m.size);
m.set(-0, "minus zero");
console.log(m.get(0), [...m.keys()].map((k) => (k === 0 ? String(1 / k) : String(k))).join());
console.log(m.get(2 ** 31), m.set(2 ** 31, "big").get(2147483648), m.get(0.1 + 0.2), m.set(0.3, "x").get(0.30000000000000004));

// Insertion order: updating keeps the place, delete + add moves to the end.
const order = new Map([["a", 1], ["b", 2], ["c", 3]]);
order.set("a", 10);
order.delete("b");
order.set("b", 20);
console.log([...order].join(" | "));
console.log(order.delete("zz"), order.delete("a"), order.has("a"), order.size);
console.log(order.clear(), order.size, [...order].length);

// Set.
const s = new Set([3, 1, 3, "3", NaN, NaN, -0]);
console.log(s.size, s.has(0), s.has(-0), s.has(NaN), s.has("1"), [...s].join());
console.log(s.add(1) === s, s.add(4).size, s.delete(3), s.delete(3), [...s.values()].join());
console.log([...s.entries()].map((e) => e.join(":")).join(), [...s.keys()].join());
console.log(new Set("hello").size, [...new Set("hello")].join(""));

// Constructors.
const copy = new Map(order.set("x", 1).set("y", 2));
copy.set("z", 3);
console.log(order.size, copy.size, [...new Map(copy.entries())].join(" "));
function* pairs() {
  yield ["g1", 1];
  yield ["g2", 2];
}
console.log([...new Map(pairs())].join(" "), new Map(null).size, new Set(undefined).size);
console.log(new Map([[1]]).get(1), new Map([{ 0: "k", 1: "v" }]).get("k"));
console.log(new Set(new Map([[1, 2]])).size, [...new Set([[1, 2]])][0].join());

// Iteration while changing the map.
const live = new Map([[1, "a"], [2, "b"], [3, "c"]]);
const seen = [];
for (const [k] of live) {
  seen.push(k);
  if (k === 1) {
    live.delete(2);
    live.set(4, "d");
  }
  if (k === 3) {
    live.set(5, "e");
  }
}
console.log(seen.join());
// Many additions during iteration (the table is rebuilt several times).
const grow = new Set([0]);
const visited = [];
for (const v of grow) {
  visited.push(v);
  if (v < 40) {
    grow.add(v + 1);
    grow.delete(v - 1);
  }
}
console.log(visited.length, visited[40], grow.size, [...grow].join());
// clear() during iteration, then additions: the iterator continues with them.
const cl = new Map([[1, 1], [2, 2], [3, 3]]);
const clSeen = [];
for (const [k] of cl) {
  clSeen.push(k);
  if (k === 1) {
    cl.clear();
    cl.set(7, 7);
    cl.set(8, 8);
  }
}
console.log(clSeen.join(), cl.size);
// An iterator made before changes, used after.
const im = new Map([["a", 1], ["b", 2], ["c", 3], ["d", 4]]);
const it = im.keys();
console.log(it.next().value);
im.delete("b");
im.delete("c");
for (let i = 0; i < 20; i++) {
  im.set("n" + i, i);
}
console.log([...it].join());
console.log(it.next().done, it.next().value, it[Symbol.iterator]() === it);
// Deleting everything, then adding, while an iterator waits.
const w = new Set([1, 2, 3]);
const wi = w.values();
wi.next();
w.delete(1);
w.delete(2);
w.delete(3);
w.add(9);
console.log([...wi].join());

// forEach.
const fe = new Map([["a", 1], ["b", 2]]);
fe.forEach(function (v, k, map) {
  console.log(k, v, map === fe, this.tag);
  if (k === "a") {
    map.set("c", 3);
  }
}, { tag: "T" });
new Set(["x", "y"]).forEach((v, k, set) => console.log(v, k, set.size));

// Iterators and the prototype.
console.log(Map.prototype.entries === Map.prototype[Symbol.iterator], Set.prototype.keys === Set.prototype.values);
console.log(Set.prototype.values === Set.prototype[Symbol.iterator], new Map()[Symbol.iterator] === new Map().entries);
console.log(Map.prototype.get.name, Map.prototype.get.length, Map.prototype.set.length, Map.prototype.forEach.length);
console.log(Set.prototype.add.length, Set.prototype.forEach.length, Set.prototype.keys.name);
const mi = new Map([[1, 2]]).entries();
console.log(JSON.stringify(mi.next()), JSON.stringify(mi.next()), JSON.stringify(mi.next()));
const [first, second] = new Set(["p", "q", "r"]);
console.log(first, second, Array.from(new Map([[1, 2], [3, 4]]).values()).join());
console.log(new Map() instanceof Map, new Set() instanceof Set, new Map() instanceof Set, {} instanceof Map);
console.log(typeof new Map(), Object.keys(new Map([[1, 2]])).length, JSON.stringify(new Map([[1, 2]])));
console.log(Object.getPrototypeOf(new Map()) === Map.prototype, Object.getPrototypeOf(new Set()) === Set.prototype);

// Errors.
show("no new", () => Map());
show("no new set", () => Set([1]));
show("not iterable", () => new Map(5));
show("not iterable set", () => new Set(5));
show("not iterable bool", () => new Map(true));
show("not iterable obj", () => new Map({}));
show("entry", () => new Map([1]));
show("entry null", () => new Map([null]));
show("entry string", () => new Map(["ab"]));
show("entry later", () => new Map([[1, 2], 3]));
show("receiver", () => Map.prototype.get.call({}, 1));
show("receiver set", () => Map.prototype.get.call(new Set(), 1));
show("receiver map", () => Set.prototype.add.call(new Map(), 1));
show("receiver array", () => Map.prototype.set.call([], 1));
show("receiver prim", () => Map.prototype.has.call(1, 1));
show("forEach num", () => new Map().forEach(5));
show("forEach none", () => new Set().forEach());
show("forEach str", () => new Map().forEach("x"));
show("forEach null", () => new Map().forEach(null));
show("forEach obj", () => new Map().forEach({}));
show("forEach bool", () => new Set().forEach(true));
show("next", () => new Map().keys().next.call({}));
show("next set", () => new Set().values().next.call(new Map().keys()));

// Bigger: rehashing, identity hashes of many objects.
const big = new Map();
const objs = [];
for (let i = 0; i < 5000; i++) {
  const o = { i };
  objs.push(o);
  big.set(o, i);
  big.set("k" + i, i);
  big.set(i * 1.5, i);
}
let sum = 0;
for (let i = 0; i < 5000; i += 7) {
  sum += big.get(objs[i]) + big.get("k" + i) + big.get(i * 1.5);
}
for (let i = 0; i < 5000; i += 2) {
  big.delete(objs[i]);
  big.delete("k" + i);
}
console.log(big.size, sum, big.get(objs[1]), big.get(objs[2]), big.has("k3"), big.has("k4"));
let count = 0;
for (const [k, v] of big) {
  if (typeof k === "object" && k.i !== v) {
    count++;
  }
}
console.log(count, [...big.keys()].slice(0, 4).map((k) => (typeof k === "object" ? "o" + k.i : k)).join());

// console.log.
console.log(new Map(), new Set());
console.log(new Map([["a", 1], [{ x: 1 }, [1, 2]], [NaN, "s"]]));
console.log(new Set([1, "a", [2], { b: 3 }, new Set([4])]));
console.log({ a: { b: { c: new Map([[1, 2]]) } } }, { a: { b: { c: new Set() } } });
console.log([[[new Map([[1, { a: { b: 1 } }]])]]]);
console.log(new Map([[1, new Map([[2, new Map([[3, new Map([[4, 5]])]])]])]]));
console.log(new Map(Array.from({ length: 12 }, (_, i) => [i, "v" + i])));
console.log(new Set(Array.from({ length: 30 }, (_, i) => i)));
console.log([new Map([["k", "v"]])], { m: new Set(["x"]) });
console.log(new Map([[1, "a".repeat(30)], [2, "b".repeat(40)]]));
show("not iterable sym", () => new Map(Symbol("q")));
show("not iterable fn", () => new Set(function f() {}));
show("not iterable nan", () => new Set(NaN));
