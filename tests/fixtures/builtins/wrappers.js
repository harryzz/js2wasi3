// T30: primitive wrapper objects.
const s = new String("ab");
const n = new Number(2);
const b = new Boolean(false);
console.log(typeof s, typeof n, typeof b);
console.log(!!b, b ? "truthy" : "falsy");
console.log(s === s, s == "ab", s === "ab", new String("a") == new String("a"));
console.log(n * 3, n + 1, new Boolean(true) ^ true, s + 1, `${s}!`);
console.log(new String("1") == 1, n == 2, b == false);
console.log(s instanceof String, n instanceof Number, b instanceof Boolean, s instanceof Object);
console.log(Object.getPrototypeOf(s) === String.prototype, Object.getPrototypeOf(n) === Number.prototype);
console.log(s.length, s[0], s[1], s[2]);
console.log(s.valueOf(), s.toString(), n.valueOf(), n.toString(), b.valueOf(), b.toString());
console.log(n.toFixed(2), s.toUpperCase(), s.slice(1));
console.log(typeof String("x"), typeof Number("3"), typeof Boolean(0));
console.log(String(), Number(), new String().length, +new Number(), +new Boolean());

const o = Object("xy");
console.log(typeof o, o instanceof String, o.length, o + "z");
const on = Object(5);
console.log(typeof on, on instanceof Number, on * 2);
const ob = Object(true);
console.log(typeof ob, ob instanceof Boolean, ob == true);
const sym = Symbol("q");
const os = Object(sym);
console.log(typeof os, os.valueOf() === sym, os.description);
const plain = {};
console.log(Object(plain) === plain, new Object(plain) === plain);
console.log(typeof new Object(1), typeof new Object("s"));

// valueOf / toString of the wrong kind.
try {
  Number.prototype.valueOf.call("x");
} catch (e) {
  console.log(e.constructor.name, e.message);
}
try {
  Boolean.prototype.toString.call(1);
} catch (e) {
  console.log(e.constructor.name, e.message);
}
try {
  String.prototype.valueOf.call(new Number(1));
} catch (e) {
  console.log(e.constructor.name, e.message);
}
console.log(Number.prototype.toString.call(new Number(255), 16));
console.log(Boolean.prototype.valueOf.call(new Boolean(true)));

// A user `valueOf` on the wrapper wins.
const w = new Number(10);
w.valueOf = () => 42;
console.log(w + 1);
try {
  new String(Symbol("s"));
} catch (e) {
  console.log(e.constructor.name, e.message);
}
console.log(String(Symbol("s")));
