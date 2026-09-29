// `arguments` in arrow functions (T43): the enclosing function's object.
function f(a, b) {
  const g = () => arguments.length + ":" + arguments[0];
  const h = () => () => arguments;
  return [g(), h()() === arguments, [...h()()].join(",")];
}
console.log(f(1, 2, 3));

function sum() {
  return [1, 2].map(() => Array.prototype.reduce.call(arguments, (x, y) => x + y, 0));
}
console.log(sum(10, 20, 30));

// Arrows keep the outer function's `arguments`, not their own call's.
function outer() {
  const inner = (x) => arguments[0] + x;
  return inner(100);
}
console.log(outer(5));

// In a method, a generator and an async function.
const o = { m() { return (() => arguments.length)(); } };
console.log(o.m(1, 2));
function* gen() { yield (() => arguments[1])(); }
console.log([...gen("a", "b")]);
async function af() { await 0; return (() => arguments[0])(); }
af("async").then((v) => console.log(v));

// A nested ordinary function has its own `arguments`.
function two(a) {
  return (() => function () { return arguments[0]; })()("inner") + (() => arguments[0])();
}
console.log(two("outer"));

// Direct eval code using `arguments` (an arrow, T42).
function ev(a, b) { return eval("arguments.length + a + b"); }
console.log(ev(1, 2));

// A function using `arguments` whose parameters are captured (was a miscompile: fresh `undefined` cells).
function captured(a, b) { const n = arguments.length; return (() => a + b)() + n; }
console.log(captured(1, 2));
