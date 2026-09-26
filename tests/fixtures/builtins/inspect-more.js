// T25 slice 5: console.log like node's util.inspect (F24, F62, F99, F52): symbol keys, string escapes,
// extra array properties, > 100 entries, circular references, Map/Set iterators, maps with own
// properties, Symbol.toStringTag prefixes.

{
  console.log({ [Symbol("a")]: 1, b: 2 }, [Symbol.iterator]);
}

{
  console.log(["it's", 'say "hi"', "a\nb", "tab\there", "\x01\x7f", "back\\slash", `both ' and "`]);
}

{
  console.log({ "a-b": 1, "0x": 2, 3: 4, "": 5, "é": 6 });
}

{
  const a = [1, 2]; a.extra = 3; console.log(a);
}

{
  console.log(Array.from({ length: 120 }, (_, i) => i));
}

{
  const o = { name: "o" }; o.self = o; o.list = [o]; console.log(o);
}

{
  const m = new Map([["a", 1]]); console.log(m.keys(), m.entries(), new Set([1]).values(), m[Symbol.iterator]());
}

{
  const m2 = new Map([[1, 2]]); m2.x = 1; console.log(m2);
}

{
  console.log(new Set(Array.from({ length: 105 }, (_, i) => i)));
}

{
  class T { get [Symbol.toStringTag]() { return "Tee"; } } console.log(new T(), { [Symbol.toStringTag]: "Y", a: 1 });
}

{
  console.log(Object.create(null), Object.assign(Object.create(null), { k: 1 }));
}

{
  class P { constructor() { this.x = 1; } } console.log([new P(), new P()], { p: new P() });
}

{
  console.log("日本語", ["日本語", "ab"], { 日本: "語" });
}

{
  console.log([[1, [2, [3, [4, [5]]]]]], { a: { b: { c: { d: {} } } } });
}

{
  console.log(new Array(5), [1, , 3], [, , 1, , ,]);
}

{
  function F() {} F.prototype.k = 1; console.log(new F(), Object.create({ inherited: 1 }));
}

{
  console.log([function named() {}, () => {}, class K {}, Symbol("s")]);
}

{
  console.log({ f() {}, get g() { return 1; }, async h() {}, *i() {} });
}

{
  console.log(["\u0080\u009f\ud800x\udc00", "\b\f\v\r\0", "a\x00b", " ", "😀", "all ' \" ` three", "all ' \" ${x}", "\ud83d"]);
  console.log({ "k\n": "v\\" });
}

{
  const r = (n, f) => Array.from({ length: n }, (_, i) => f(i));
  console.log(r(101, (i) => i));
  console.log(r(150, (i) => "s" + i));
  const sp = []; sp[5] = 1; sp[300] = 2; console.log(sp);
  const big = r(105, (i) => i); big.k = "v"; console.log(big);
  const few = [1, 2, 3, 4, 5, 6, 7]; few.a = 1; few.b = 2; console.log(few);
  console.log([r(101, (i) => i % 3)]);
  console.log(r(7, (i) => ({ i })));
}

{
  const a = { n: 1 }; const b = { a }; a.b = b; console.log(a, [a]);
  const arr = [1]; arr.push(arr); console.log(arr);
  const m = new Map(); m.set("self", m); console.log(m);
  class Node { constructor() { this.next = this; } } console.log(new Node());
  const x = {}, y = {}; x.y = y; y.x = x; x.x = x; console.log(x, { same: y, again: y });
  const deep = { l1: { l2: { l3: {} } } }; deep.l1.l2.l3.back = deep; console.log(deep);
  const f = function g() {}; f.me = f; console.log(f);
}

{
  const m = new Map([["a", 1], ["b", { x: [1] }]]);
  const it = m.keys(); it.next(); console.log(it, [...it]);
  const s = new Set(["p", "q"]); console.log(s.entries(), s.keys(), s[Symbol.iterator](), m.values());
  console.log(new Map().entries(), new Set(Array.from({ length: 120 }, (_, i) => i)).values());
}

{
  class T { get [Symbol.toStringTag]() { return "Tee"; } }
  class Same { get [Symbol.toStringTag]() { return "Same"; } }
  const proto = { [Symbol.toStringTag]: "Inherited" };
  console.log(new T(), new Same(), Object.create(proto), { [Symbol.toStringTag]: "Own" });
  const np = Object.create(null); Object.defineProperty(np, Symbol.toStringTag, { value: "NP" }); console.log(np);
  console.log([new T()], new Map(), Math);
}
