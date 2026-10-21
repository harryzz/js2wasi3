// T143: `arguments.length` and `arguments[k]` read the argument array; the object is made only for other uses.
function sum() { let s = 0; for (let i = 0; i < arguments.length; i++) s += arguments[i]; return s; }
console.log(sum(), sum(1), sum(1, 2, 3), sum(...[4, 5, 6, 7]), sum.apply(null, [1, 2]), sum.call(null, 9, 8));

// Keys: numbers, numeric strings, out of range, negative, fractions, non-index names.
function keys() {
  const r = [];
  for (const k of [0, 1, 2, 3, -1, 1.5, "1", "01", "length", "x", NaN, 2 ** 31, 4294967297]) r.push(String(arguments[k]));
  return [arguments.length, r.join("|")];
}
console.log(keys("a", "b"), keys());

// Typed index (an integer local) and a computed expression.
function typed(a, b) {
  let i = 0, out = [];
  while (i < arguments.length) { out.push(arguments[i], arguments[arguments.length - 1 - i]); i += 1; }
  return out.join();
}
console.log(typed(1, 2, 3), typed(), typed("x"));

// Mixed with other uses: the object is built, as before.
function mixed() { const a = arguments; return [arguments.length, a[0], Array.prototype.slice.call(arguments, 1).join()]; }
console.log(mixed(1, 2, 3));
function spread() { return [...arguments].length + arguments[0]; }
console.log(spread(5, 6, 7));
function assigns() { arguments[0] = 9; return arguments[0] + arguments.length; }
console.log(assigns(1, 2));
function typeofs() { return [typeof arguments, typeof arguments.length, typeof arguments[0]].join(); }
console.log(typeofs(1));

// A parameter changed after the call started: `arguments` keeps the original value (unmapped, strict).
function params(a, b) { a = 100; return [arguments[0], arguments[1], arguments.length, a, b]; }
console.log(params(1, 2), params(1));

// Parameters fewer or more than the arguments; defaults; rest.
function counts(a, b = 5, ...r) { return [arguments.length, arguments[0], arguments[1], arguments[2], r.length]; }
console.log(counts(), counts(1), counts(1, 2, 3, 4));

// Arrow functions read the enclosing function's arguments (the object is made), and arrows without their own.
function outer() { const f = () => arguments[0] + arguments.length; return f(); }
console.log(outer(10, 20));

// A call with `this`: `arguments[0]()` is a method call on the object, `arguments.length` is a number.
function method() { return arguments[0](); }
console.log(method(function () { return typeof this + ":" + this.length; }, 2));

// Evaluation order, getters on the prototype, deleting, in.
function order() {
  const log = [];
  const k = (n) => { log.push("k" + n); return n; };
  const r = [arguments[k(0)], arguments[k(1)], arguments.length];
  return [r.join(), log.join()];
}
console.log(order("p", "q"));
function inop() { return [0 in arguments, 5 in arguments, "length" in arguments]; }
console.log(inop(1));

// Generators, async functions, constructors, methods and many calls.
function* gen() { yield arguments.length; yield arguments[1]; }
console.log([...gen(7, 8, 9)]);
(async function () { await null; console.log("async", arguments.length, arguments[0]); })("a", "b");
class K { constructor() { this.n = arguments.length + (arguments[0] || 0); } m() { return arguments.length; } }
console.log(new K(5, 6).n, new K().n, new K().m(1, 2, 3));
let total = 0;
for (let i = 0; i < 1000; i++) total += sum(i, 1, 2);
console.log(total);

// `var a = arguments` read only as a.length / a[i] (earley-boyer's sc_list), and the cases where it is not.
function list() { var res = null; var a = arguments; for (var i = a.length - 1; i >= 0; i--) res = [a[i], res]; return JSON.stringify(res); }
console.log(list(), list(1), list(1, 2, 3));
function aliasLet() { const a = arguments; let s = ""; for (let i = 0; i < a.length; i++) s += a[i]; return s + a.length; }
console.log(aliasLet("x", "y"));
function aliasPassed() { const a = arguments; return Array.prototype.slice.call(a).join() + a.length; }
console.log(aliasPassed(1, 2));
function aliasReassigned() { var a = arguments; a = [9, 8, 7]; return a.length + a[0]; }
console.log(aliasReassigned(1));
function aliasClosure() { var a = arguments; const f = () => a.length + a[0]; return f(); }
console.log(aliasClosure(4, 5));
function aliasEarly() { var before = typeof a; var a = arguments; return [before, a.length]; }
console.log(aliasEarly(1, 2));
function aliasTwice() { var a = arguments; var b = arguments; return a.length + b.length + a[0] + b[1]; }
console.log(aliasTwice(1, 2, 3));
function aliasAssigned() { var a = arguments; a[0] = "changed"; return arguments[0] + a.length; }
console.log(aliasAssigned("orig", 2));
function aliasWithApply(f) { var a = arguments; return f.apply(null, arguments) + a.length; }
console.log(aliasWithApply(function () { return arguments.length; }, 1, 2));
