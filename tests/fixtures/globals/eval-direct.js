// Direct `eval` of code known at build time (T42): strict eval code in the caller's scope.
let x = 1;
console.log(eval("x + 1"), eval(`x * 10`));
eval("x = 5");
console.log(x);

// Completion values.
console.log(eval("1; 2; 3"), eval("var v = 1;"), eval(""), eval("'str'"));
console.log(eval("1; if (true) {}"), eval("1; if (true) { 2 } else { 3 }"), eval("1; {}"));
console.log(eval("let i = 0; while (i < 3) { i++; }"), eval("let j = 0; do { j += 2; } while (j < 5)"));
console.log(eval("for (let k = 0; k < 3; k++) k * 2;"), eval("5; for (const q of []) q;"));
console.log(eval("switch (2) { case 1: 'one'; case 2: 'two'; case 3: 'three'; break; case 4: 'four' }"));
console.log(eval("try { 'tried' } finally { 'finally' }"), eval("try { throw 1 } catch (e) { 'caught' }"));
console.log(eval("outer: while (true) { 7; break outer; }"), eval("function f() { return 9 } f()"));

// Strict: var and function declarations stay inside.
eval("var hidden = 1; function inner() {}");
console.log(typeof hidden, typeof inner);

// `this`, `arguments`, `new.target`, `super` of the caller.
function f(a, b) { return eval("a + b + this.k"); }
console.log(f.call({ k: 100 }, 1, 2));
function Ctor() { this.t = eval("new.target === Ctor"); }
console.log(new Ctor().t);
class A { m() { return "A.m"; } }
class B extends A { m() { return eval("super.m()") + "+B"; } }
console.log(new B().m());
const arrow = () => eval("typeof this");
console.log(arrow());

// Early errors are SyntaxErrors at the call.
try { eval("return 1"); } catch (e) { console.log(e instanceof SyntaxError); }
try { eval("new.target"); } catch (e) { console.log(e instanceof SyntaxError); }
try { eval("let let = 1"); } catch (e) { console.log(e instanceof SyntaxError); }
try { eval("}"); } catch (e) { console.log(e instanceof SyntaxError); }
let ran = false;
class C { #x = eval("ran = true; () => arguments"); }
try { new C(); } catch (e) { console.log(e instanceof SyntaxError, ran); }

// Non-string arguments are returned as they are.
console.log(eval(42), eval(), eval(null), eval(true));

// Nested eval.
console.log(eval("eval('x + 100')"));

// Code that tries to leave its function, labelled loops, eval in generators and async functions.
try { eval("} m() {"); } catch (e) { console.log(e instanceof SyntaxError); }
try { eval("}); x = 99; ((() => {"); } catch (e) { console.log(e instanceof SyntaxError, x); }
console.log(eval("let n = 0; l: for (;;) { n++; if (n < 3) continue l; break; } n"));
function* g() { yield eval("1 + 1"); try { eval("yield 3"); } catch (e) { yield e instanceof SyntaxError; } }
console.log([...g()]);
async function af() { const v = await 5; return eval("v * 2"); }
af().then((v) => console.log(v));
console.log(eval("'use strict'; 'directive value'"), eval("/* comment */ 8 // trailing"));
