// Hoisting, mutual recursion, deep nesting, captured params and catch bindings.
function outer() {
  const isEven = (n) => (n === 0 ? true : isOdd(n - 1));
  function isOdd(n) {
    return n === 0 ? false : isEven(n - 1);
  }
  return isEven(10) + " " + isOdd(7) + " " + hoisted();
  function hoisted() {
    return "hoisted";
  }
}
console.log(outer());

function level1(a) {
  return function level2(b) {
    return (c) => {
      a++;
      return a + b + c;
    };
  };
}
const l3 = level1(1)(10);
console.log(l3(100), l3(100));

function param(x) {
  const get = () => x;
  x = x * 3;
  return get();
}
console.log(param(7));

let caught;
try {
  throw new Error("boom");
} catch (e) {
  caught = () => e.message;
}
console.log(caught());

function fib(n) {
  function go(k) {
    return k < 2 ? k : go(k - 1) + go(k - 2);
  }
  return go(n);
}
console.log(fib(20));

// Shadowing: the inner `x` is a different variable.
function shadow() {
  let x = "outer";
  const f = () => {
    let x = "inner";
    return x;
  };
  return f() + " " + x;
}
console.log(shadow());

// A closure in a finally block, a closure returning from a loop.
function withFinally() {
  let log = "";
  const add = (s) => {
    log += s;
  };
  try {
    add("t");
    return log;
  } finally {
    add("f");
    console.log("finally saw", log);
  }
}
console.log(withFinally());

function find(limit) {
  for (let i = 0; ; i++) {
    const check = () => i * i > limit;
    if (check()) return i;
  }
}
console.log(find(50));

// Top-level code: block-scoped variables and nested `var`s (F28).
{
  var nestedVar = "nested var";
  let blockLet = "block let";
  var readBoth = () => nestedVar + " / " + blockLet;
}
console.log(readBoth());
function readNestedVar() {
  return nestedVar;
}
console.log(readNestedVar());
