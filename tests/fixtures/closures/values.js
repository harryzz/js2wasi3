// Functions as values: passed, returned, stored, compared; calling non-functions.
function twice(f, x) {
  return f(f(x));
}
function double(x) {
  return x * 2;
}
console.log(twice(double, 3), twice((s) => s + "!", "hi"));

function compose(f, g) {
  return (x) => f(g(x));
}
const inc = (x) => x + 1;
console.log(compose(double, inc)(5), compose(inc, double)(5));

console.log(double === double, inc === inc, compose(inc, inc) === compose(inc, inc));

const fact = function f(n) {
  return n <= 1 ? 1 : n * f(n - 1);
};
console.log(fact(10), fact.name);

console.log(
  (function () {
    return "iife";
  })(),
  ((a, b) => a * b)(6, 7),
);

// Extra arguments are evaluated and dropped; missing ones are undefined.
function one(a) {
  return a;
}
let side = 0;
console.log(one(1, side++, side++), side, one());
const arrow = (a, b) => a;
console.log(typeof one, typeof arrow);

// Calling something that isn't a function.
const o = { a: {}, n: 5 };
function attempt(label, f) {
  try {
    f();
  } catch (e) {
    console.log(label, e instanceof TypeError, e.message);
  }
}
attempt("number", () => o.n());
attempt("missing", () => o.a.b());
attempt("computed", () => o["a"]());
attempt("call result", () => one(5)());
attempt("undefined var", () => {
  let u;
  u();
});
attempt("null member", () => o.x.y());

// A named function expression seen only from a nested arrow function.
const selfie = function me() {
  return () => me;
};
console.log(selfie()() === selfie, selfie.name);
