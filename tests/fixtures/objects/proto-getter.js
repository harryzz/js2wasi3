// T148: `Object.prototype.__proto__`'s getter (reads of `x.__proto__`), reached by npm packages such as dunder-proto.
console.log([].__proto__ === Array.prototype, ({}).__proto__ === Object.prototype, Object.create(null).__proto__);
const d = Object.getOwnPropertyDescriptor(Object.prototype, "__proto__");
console.log(typeof d.get, d.enumerable, d.configurable);
console.log(d.get.call([]) === Array.prototype, d.get.call(5) === Number.prototype, d.get.call("s") === String.prototype);
try { d.get.call(null); } catch (e) { console.log(e.name); }
try { d.get.call(undefined); } catch (e) { console.log(e.name); }
const o = { x: 1 }, p = { y: 2 };
o.__proto__ = p;
console.log(o.y, o.__proto__ === p, Object.getPrototypeOf(o) === p);
class A {} class B extends A {}
console.log(new B().__proto__ === B.prototype, B.__proto__ === A, A.__proto__ === Function.prototype);
const own = { ["__proto__"]: 5 };
console.log(own.__proto__, Object.getPrototypeOf(own) === Object.prototype);
const lit = { __proto__: p };
console.log(lit.y, lit.__proto__ === p);
console.log((() => {}).__proto__ === Function.prototype, Symbol().__proto__ === Symbol.prototype, (1n).__proto__ === BigInt.prototype);
let cur = new B(), depth = 0;
while (cur.__proto__ !== null) { cur = cur.__proto__; depth++; }
console.log(depth);
