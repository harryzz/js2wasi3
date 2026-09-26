// Yields in conditions and loop heads.
function* conds() {
  if (yield "c1") {
    console.log("then");
  } else {
    console.log("else");
  }
  let n = 0;
  while ((yield "w" + n) !== "stop") n++;
  const t = (yield "t") ? yield "yes" : yield "no";
  console.log("ternary", t);
  const s = (yield "and") && (yield "and2");
  console.log("and", s);
  const o = (yield "or") || (yield "or2");
  console.log("or", o);
}
const c = conds();
const sends = [undefined, true, 1, 2, "stop", false, "N", 0, "x", 1];
for (const v of sends) console.log(c.next(v));

// Typed locals (numbers, booleans, int32) survive suspension.
function* typed(k) {
  let total = 0.5;
  let flag = false;
  let bits = k | 0;
  for (let i = 0; i < 3; i++) {
    total += i;
    flag = !flag;
    bits = (bits << 1) ^ i;
    yield [total, flag, bits];
  }
  return total * 2;
}
console.log([...typed(3)], typed(1).next());

// Labels, break and continue across yields.
function* labels() {
  outer: for (let i = 0; i < 3; i++) {
    for (let j = 0; j < 3; j++) {
      if (j === 1) continue outer;
      if (i === 2) break outer;
      yield `${i},${j}`;
    }
  }
  let k = 0;
  do {
    yield "do" + k;
    k++;
  } while (k < 2);
  block: {
    yield "in block";
    if (k) break block;
    yield "skipped";
  }
}
console.log([...labels()]);

// Yield in catch and in finally, rethrow after a yield.
function* handlers() {
  try {
    throw new Error("first");
  } catch (e) {
    const v = yield "caught " + e.message;
    console.log("catch resumed with", v);
  }
  try {
    try {
      yield "inner try";
      throw new Error("second");
    } finally {
      yield "finally";
    }
  } catch (e) {
    yield "outer caught " + e.message;
  }
}
const h = handlers();
for (let r = h.next(); !r.done; r = h.next("v")) console.log(r.value);

// for-of and for-in inside a generator; closures capture loop variables.
function* loops(obj, arr) {
  for (const k in obj) yield "key " + k;
  for (const x of arr) {
    yield "val " + x;
  }
  const fns = [];
  for (let i = 0; i < 3; i++) {
    fns.push(() => i);
    yield i;
  }
  yield fns[0]() + fns[2]();
  let count = 0;
  const inc = () => ++count;
  inc();
  yield count;
  inc();
  yield count;
}
console.log([...loops({ a: 1, b: 2 }, ["x", "y"])]);

// Breaking out of a for-of over a generator that's inside another generator.
function* take(n, it) {
  if (n <= 0) return;
  for (const v of it) {
    yield v;
    if (--n <= 0) return;
  }
}
function* naturals() {
  let i = 0;
  try {
    while (true) yield i++;
  } finally {
    console.log("naturals closed at", i);
  }
}
console.log([...take(4, naturals())]);

// Parameters: defaults run when the generator is created; `this`.
function log(x) {
  console.log("default", x);
  return x;
}
function* params(a, b = log(a + 1), ...rest) {
  yield a;
  yield b;
  yield rest;
}
const p = params(1);
console.log("created");
console.log([...p]);
console.log([...params(1, 2, 3, 4)]);
function* thisGen() {
  yield this.name;
}
console.log(thisGen.call({ name: "bound" }).next());
const named = function* () {};
const arrowish = { gen: function* () {} };
console.log(named, arrowish.gen, named.name, named.length, params.length);

// Destructuring and spread with yields.
function* destr() {
  const { a, b = yield "default b" } = yield "obj";
  const [x, y] = [yield "x", yield "y"];
  console.log(a, b, x, y);
  console.log([1, ...(yield "spread")]);
}
const d = destr();
console.log(d.next().value, d.next({ a: 1 }).value, d.next("B").value);
console.log(d.next("X").value, d.next("Y").value, d.next([7, 8]));

// Uncaught exceptions leave the generator completed.
function* thrower() {
  yield 1;
  throw new RangeError("bad");
}
const th = thrower();
th.next();
try {
  th.next();
} catch (e) {
  console.log(e.name, e.message);
}
console.log(th.next());
