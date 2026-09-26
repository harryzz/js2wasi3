function* inner() {
  const x = yield "i1";
  console.log("inner got", x);
  yield "i2";
  return "inner result";
}

function* outer() {
  yield "o1";
  const r = yield* inner();
  console.log("yield* gave", r);
  yield* [10, 20];
  yield* "hi";
  return "outer done";
}

const g = outer();
let r = g.next();
while (!r.done) {
  console.log(r);
  r = g.next("sent");
}
console.log(r);
console.log([...outer()]);

// return() and throw() are forwarded to the inner generator.
function* guarded() {
  try {
    yield "g1";
    yield "g2";
  } finally {
    console.log("inner cleanup");
  }
}
function* wrapper() {
  try {
    yield* guarded();
  } finally {
    console.log("outer cleanup");
  }
}
const w = wrapper();
console.log(w.next());
console.log(w.return("stop"));

const w2 = wrapper();
w2.next();
try {
  w2.throw(new Error("thrown in"));
} catch (e) {
  console.log("caught", e.message);
}

function* catching() {
  try {
    yield "c1";
  } catch (e) {
    console.log("inner caught", e);
    return "handled";
  }
}
function* outer2() {
  const v = yield* catching();
  console.log("result", v);
  yield "after";
}
const o2 = outer2();
console.log(o2.next());
console.log(o2.throw("err"));

// An iterator object without throw/return.
const plain = {
  [Symbol.iterator]() {
    let i = 0;
    return {
      next(v) {
        console.log("next called with", v);
        i++;
        return { value: i, done: i > 2 };
      },
    };
  },
};
function* overPlain() {
  const res = yield* plain;
  console.log("plain result", res);
}
const p = overPlain();
console.log(p.next("a"));
console.log(p.next("b"));
console.log(p.next("c"));
const p2 = overPlain();
p2.next();
console.log(p2.return("early"));
const p3 = overPlain();
p3.next();
try {
  p3.throw(new Error("x"));
} catch (e) {
  console.log(e.message);
}

// Errors.
function* bad() {
  yield* 5;
}
try {
  bad().next();
} catch (e) {
  console.log(e.message);
}
function* badResult() {
  yield* {
    [Symbol.iterator]() {
      return { next() { return 3; } };
    },
  };
}
try {
  badResult().next();
} catch (e) {
  console.log(e.message);
}

// Generators as methods and closures.
const obj = {
  *items() {
    yield this.a;
    yield this.b;
  },
  a: "A",
  b: "B",
};
console.log([...obj.items()]);
class Tree {
  constructor(v, kids) {
    this.v = v;
    this.kids = kids;
  }
  *[Symbol.iterator]() {
    yield this.v;
    for (const k of this.kids) yield* k;
  }
}
const t = new Tree(1, [new Tree(2, []), new Tree(3, [new Tree(4, [])])]);
console.log([...t]);
function counter(start) {
  let n = start;
  return function* () {
    while (true) yield n++;
  };
}
const it = counter(5)();
console.log(it.next().value, it.next().value, it.next().value);
