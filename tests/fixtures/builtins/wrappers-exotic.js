// T30: String wrapper own properties, tags, JSON and printing of wrapper objects.
const s = new String("ab");
console.log(Object.keys(s), Object.getOwnPropertyNames(s));
for (const k in s) console.log("key", k);
console.log(s.hasOwnProperty("0"), s.hasOwnProperty("length"), "1" in s, 2 in s);
try { s[0] = "z"; } catch (e) { console.log(e.constructor.name, e.message); }
try { s.length = 5; } catch (e) { console.log(e.constructor.name, e.message); }
s.extra = 1;
console.log(Object.keys(s), s[0]);
console.log(Object.prototype.toString.call(s), Object.prototype.toString.call(new Number(1)), Object.prototype.toString.call(new Boolean(true)), Object.prototype.toString.call(Object(Symbol())));
console.log(String(Object.prototype.toString.call(1)));
console.log(JSON.stringify([new String("x"), new Number(3), new Boolean(false), { a: new String("y") }]));
const s2 = new String("ab");
console.log(s2, new Number(1), new Number(-0), new Boolean(false), Object(Symbol("q")));
console.log([s2, new Number(3)], { a: new Boolean(true) });
s2.extra = 1;
console.log(s2);
class Foo extends Object {}
const n = new Number(5);
n.__proto__ = Foo.prototype;
console.log(n);
const z = new Number(7);
z.__proto__ = null;
console.log(z);
console.log({ a: { b: { c: { d: s2 } } } });
console.log({ a: { b: { c: { d: new String("x") } } } });
console.log(new String("it's2"));
console.log("%s2|%O", new String("q"), new Number(2));
console.log(JSON.stringify({ a: 1, b: [2] }, [new String("a")], new Number(2)));
console.log(JSON.stringify("x", null, new String("--")), JSON.stringify(Object(Symbol("s"))));
const nv = new Number(1);
nv.valueOf = () => 9;
console.log(JSON.stringify([nv]));
