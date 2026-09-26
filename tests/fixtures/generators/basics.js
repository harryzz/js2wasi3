function* count(n) {
  for (let i = 0; i < n; i++) {
    yield i;
  }
  return "done";
}

const g = count(3);
console.log(g.next());
console.log(g.next());
console.log(g.next());
console.log(g.next());
console.log(g.next());

for (const x of count(4)) console.log("for-of", x);
console.log([...count(5)]);
const [a, b, ...rest] = count(6);
console.log(a, b, rest);

// Values sent in with next().
function* echo() {
  let got = yield "first";
  while (got !== "stop") {
    got = yield "got " + got;
  }
  return "stopped";
}
const e = echo();
console.log(e.next("ignored"));
console.log(e.next("a"));
console.log(e.next("b"));
console.log(e.next("stop"));
console.log(e.next("again"));

// Yields inside expressions keep evaluation order.
function* expr() {
  const x = 10 + (yield 1) * (yield 2);
  console.log("x", x);
  const arr = [yield "a", "mid", yield "b"];
  console.log(arr);
  const pair = (p, q) => p + "-" + q;
  console.log("call", pair(yield "m1", yield "m2"));
}
const ex = expr();
console.log(ex.next().value, ex.next(3).value, ex.next(4).value);
console.log(ex.next("A").value, ex.next("B").value);
console.log(ex.next(7).value, ex.next(9));

console.log(count, g, typeof g.next, g[Symbol.iterator]() === g);
