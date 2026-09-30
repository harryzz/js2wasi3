// `structuredClone` (T53).
const shared = { s: 1 };
const o = { a: 1, b: "two", c: [1, , 3], d: { e: null, f: undefined }, g: shared, h: shared, n: -0, big: 2n };
o.self = o;
const c = structuredClone(o);
console.log(c !== o, c.self === c, c.g === c.h, c.g !== shared, Object.is(c.n, -0));
console.log(c);

class P { constructor() { this.x = 1; } get y() { return 2; } }
const p = structuredClone(new P());
console.log(p, p instanceof P, Object.getPrototypeOf(p) === Object.prototype);

const withGetter = { get g() { return 5; } };
Object.defineProperty(withGetter, "hidden", { value: 1, enumerable: false });
withGetter[Symbol("k")] = 1;
console.log(structuredClone(withGetter));
const a = [1, , 3];
a.prop = "x";
console.log(structuredClone(a));
console.log(structuredClone(Object.assign(Object.create(null), { z: 1 })));

console.log(structuredClone(new Number(3)), structuredClone(new String("s")), structuredClone(new Boolean(false)), structuredClone(Object(1n)));
const re = /ab+c/gi;
re.lastIndex = 3;
const cre = structuredClone(re);
console.log(cre, cre !== re, cre.lastIndex, cre.test("xABBC"));
const d = new Date(86400000);
const cd = structuredClone(d);
console.log(cd, cd !== d, cd.getTime());

const u = new Uint8Array([1, 2, 3, 4]);
const view = new Uint16Array(u.buffer, 2, 1);
const r = structuredClone({ u, view, buf: u.buffer });
console.log(r, r.u.buffer === r.view.buffer, r.buf === r.u.buffer, r.u.buffer !== u.buffer);
r.u[0] = 9;
console.log(u[0], r.u[0]);

const key = { k: 1 };
const m = new Map([[key, new Set([1, key])], ["x", key]]);
const cm = structuredClone(m);
console.log(cm);
const [[k2, s2]] = cm;
console.log(k2 !== key, s2.has(k2), cm.get("x") === k2);

console.log(structuredClone(-0), structuredClone(1n), structuredClone(undefined), structuredClone(null), structuredClone("s"));
console.log(structuredClone(Math), structuredClone(JSON));

const e = new RangeError("boom");
e.extra = 1;
const ce = structuredClone(e);
console.log(ce instanceof RangeError, ce.message, ce.extra, Object.keys(ce), ce !== e);
console.log(structuredClone(new Error("m", { cause: 42 })).cause);
class MyErr extends TypeError {}
const me = structuredClone(new MyErr("z"));
console.log(me.constructor.name, me.name, me.message);
const ne = new Error("q");
ne.name = "Custom";
console.log(structuredClone(ne).name, Object.hasOwn(structuredClone(new Error()), "message"));

for (const v of [() => {}, function foo() {}, Symbol("s"), { a: 1, f() {} }, [1, Symbol.iterator], Promise.resolve(1), [].values(), new Map().keys(), (function* () {})()]) {
  try {
    structuredClone(v);
    console.log("cloned?");
  } catch (err) {
    console.log(err.name, "|", err.message, "|", err.constructor.name, err instanceof Error, err.code, Object.keys(err));
  }
}
try {
  structuredClone();
} catch (err) {
  console.log(err.name, err.code, err.message);
}
console.log(structuredClone.length, structuredClone.name, structuredClone(7, {}), structuredClone(8, null));
console.log([{ a: 1 }].map((x) => structuredClone(x)), globalThis.structuredClone === structuredClone);
for (const opts of [0, "s", { transfer: [] }, () => {}]) {
  try {
    console.log(structuredClone(1, opts));
  } catch (err) {
    console.log(err.name, err.code, err.message);
  }
}
structuredClone({ f() {} });
