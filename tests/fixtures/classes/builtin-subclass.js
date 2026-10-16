// T126 (F106/F108): subclasses of built-ins: instances get the subclass's prototype (`new.target`), Array species and
// `of`/`from`, `console.log` names as node (`Stack(3) [ … ]`, `M(1) [Map] { … }`). (Promise species and display,
// `extends Function` and `console.log(error)` stacks are left out: see tasks/T126-builtin-subclasses.md.)
"use strict";
import { inspect } from "node:util";
const out = (k, f) => { try { console.log(k, f()); } catch (e) { console.log(k, "!!", e.constructor.name + ": " + e.message); } };
{
class MyError extends Error { constructor(m, code) { super(m); this.name = "MyError"; this.code = code; } }
out("Error subclass", () => { const e = new MyError("boom", 7); return [e instanceof MyError, e instanceof Error, e.message, e.code, e.name, String(e), Object.getPrototypeOf(e) === MyError.prototype, typeof e.stack]; });
class Plain extends Error {}
out("Error plain", () => { const e = new Plain("x"); return [e instanceof Plain, e.name, e.message, String(e), e.constructor.name]; });
class TE extends TypeError {}
out("TypeError subclass", () => { const e = new TE("t"); return [e instanceof TE, e instanceof TypeError, e instanceof Error, e.name, String(e)]; });
out("throw/catch subclass", () => { try { throw new MyError("thrown", 1); } catch (e) { return [e instanceof MyError, e.code]; } });
class Stack extends Array { peek() { return this[this.length - 1]; } }
out("Array subclass", () => { const s = new Stack(); s.push(1, 2, 3); return [s instanceof Stack, Array.isArray(s), s.length, s.peek(), Object.getPrototypeOf(s) === Stack.prototype]; });
out("Array subclass ctor args", () => { const s = new Stack(3); const t = new Stack(1, 2); return [s.length, t.length, t[1]]; });
out("Array subclass map", () => { const s = Stack.from([1, 2]); const m = s.map(x => x * 2); return [s instanceof Stack, m instanceof Stack, m.peek()]; });
out("Array subclass of", () => Stack.of(4, 5) instanceof Stack);
class MyMap extends Map { getOr(k, d) { return this.has(k) ? this.get(k) : d; } }
out("Map subclass", () => { const m = new MyMap([[1, "a"]]); return [m.getOr(1, "z"), m.getOr(2, "z"), m instanceof Map]; });
class MyRegExp extends RegExp { exec(s) { return super.exec(s); } }
out("RegExp subclass", () => { const r = new MyRegExp("a+", "g"); return [r instanceof MyRegExp, r.test("caa"), r.source, r.flags]; });
class MyDate extends Date { year() { return this.getUTCFullYear(); } }
out("Date subclass", () => { const d = new MyDate(0); return [d instanceof MyDate, d.year()]; });
class MyStr extends String { shout() { return this.toUpperCase() + "!"; } }
out("String subclass", () => { const s = new MyStr("hi"); return [s instanceof MyStr, s.length, s.shout(), String(s)]; });
class MyNum extends Number { twice() { return this * 2; } }
out("Number subclass", () => new MyNum(21).twice());
class MyObj extends Object { constructor() { super(); this.z = 1; } }
out("Object subclass", () => { const o = new MyObj(); return [o.z, o instanceof MyObj]; });
class MySet extends Set {}
out("Set subclass", () => new MySet([1, 2, 2]).size);
class Agg extends AggregateError {}
out("AggregateError subclass", () => { const e = new Agg([1], "m"); return [e instanceof Agg, e.errors.length, e.message]; });
class Bool extends Boolean {}
out("Boolean subclass", () => new Bool(false) instanceof Bool);
}
{
class Stack extends Array { peek() { return this[this.length - 1]; } }
const s = Stack.from([3, 1, 2]);
for (const m of ["map", "filter", "slice", "splice", "concat", "flat", "flatMap"]) {
  out(m, () => { const r = m === "map" ? s.map(x => x) : m === "filter" ? s.filter(() => true) : m === "slice" ? s.slice(1) : m === "splice" ? Stack.from([1, 2, 3]).splice(0, 1) : m === "concat" ? s.concat([9]) : m === "flat" ? s.flat() : s.flatMap(x => [x]); return [r instanceof Stack, Array.from(r)]; });
}
out("console.log", () => s);
out("spread/JSON/isArray", () => [[...s], JSON.stringify(s), Array.isArray(s), s.length]);
out("sort/reverse keep class", () => [s.sort() instanceof Stack, s.reverse().peek()]);
out("length write", () => { const t = new Stack(1, 2, 3); t.length = 1; return [t.length, t.peek()]; });
out("Symbol.species", () => [Array[Symbol.species] === Array, Stack[Symbol.species] === Stack]);
class Plain extends Array {}
out("plain map", () => new Plain(1, 2).map(x => x) instanceof Plain);
class NoSpecies extends Array { static get [Symbol.species]() { return Array; } }
out("species override", () => [new NoSpecies(1, 2).map(x => x) instanceof NoSpecies, Array.isArray(new NoSpecies(1, 2).map(x => x))]);
class Args extends Array { constructor(...a) { super(...a); this.tag = "t"; } }
out("ctor with tag", () => { const a = new Args(1, 2); return [a.tag, a.length, a.map(x => x).tag]; });
out("plain arrays still plain", () => [[1, 2].map(x => x).constructor === Array, Array.of(1, 2), Array.from("ab")]);
class E1 extends Error { constructor(m, opts) { super(m, opts); this.name = "E1"; } }
out("Error cause", () => { const e = new E1("m", { cause: 5 }); return [e.cause, e.message, String(e), e.stack.split("\n")[0]]; });
class RE extends RegExp {}
out("RegExp methods", () => { const r = new RE("(\\d+)", "g"); return ["a1b22".match(r), "x9".replace(new RE("\\d"), "#"), r.lastIndex, r instanceof RE]; });
class O2 extends Object { constructor(v) { super(v); this.v = v; } }
out("Object subclass arg", () => { const o = new O2(5); return [o.v, o instanceof O2, typeof o]; });
}
{
class A extends Array {} class M extends Map {} class St extends Set {} class D extends Date {}
class R extends RegExp {} class S extends String {} class N extends Number {} class B extends Boolean {}
class O extends Object {} class U8 extends Uint8Array {} class AB extends ArrayBuffer {}
class E extends Error {} class TE extends TypeError {}
console.log(new A(1, 2), new A());
console.log(new M([[1, "a"]]), new St([1]));
console.log(new D(0), new R("x+", "g"));
console.log(new S("hi"), new N(3), new B(true));
console.log(new O(), new U8(2), new AB(2));
console.log([new A(1)], { a: new A(1, 2) });
console.log(String(new E("m")), Object.prototype.toString.call(new A()), Object.prototype.toString.call(new E()));
console.log(inspect(new A(1, 2)), `${[new A(3, 4)]}`);
}
