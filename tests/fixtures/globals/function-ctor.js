// `Function(…)` / `new Function(…)` of code known at build time, and indirect eval (T42).
const add = Function("a", "b", "return a + b");
console.log(add(2, 3), add.name, add.length);
const mul = new Function("a, b", "return a * b");
console.log(mul(4, 5), mul.length);
console.log(Function("return this")() === globalThis);
console.log(Function("'use strict'; return this")());
console.log(Function("return typeof this").call(5), Function("return this").call("s") instanceof String);
const fns = [1, 2].map(() => Function("return 1"));
console.log(fns[0] === fns[1], fns[0]());
console.log(Function("a = 1", "{ b } = {}", "return [a, b]")());
console.log(Function("x", "return x").toString());
console.log(Function()(), typeof Function(""));
try { Function("a", "}"); } catch (e) { console.log(e instanceof SyntaxError); }
try { Function("a) { return 1; } function f(", "return 2"); } catch (e) { console.log(e instanceof SyntaxError); }
try { Function("return 1 +"); } catch (e) { console.log(e instanceof SyntaxError); }

// Indirect eval: global code.
console.log((0, eval)("1 + 2"), globalThis.eval("'g'"), (0, eval)("this") === globalThis);
globalThis.shared = 7;
console.log((0, eval)("shared * 2"), Function("return shared + 1")());
try { (0, eval)("new.target"); } catch (e) { console.log(e instanceof SyntaxError); }

// `eval` and `Function` as values.
console.log(typeof eval, typeof Function, eval.name, Function.name, Function.length);
const e = eval;
console.log(e(5), e(null), (function () {}) instanceof Function, Function.prototype === Object.getPrototypeOf(() => 1));
console.log(globalThis.eval === eval, globalThis.Function === Function);
