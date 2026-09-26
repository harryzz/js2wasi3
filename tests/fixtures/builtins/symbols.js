// T25 slice 5: symbols (F53): Symbol.prototype methods, Symbol.for/keyFor, well-known symbols,
// Symbol.toPrimitive and Symbol.hasInstance, symbol keys in getOwnPropertySymbols/spread/rest/assign,
// `sym == 1`. (`Symbol("")`'s description is undefined here: F53.)

{
  const s = Symbol("a"), e = Symbol();
  console.log(s.toString(), s.description, e.description, s.valueOf() === s, String(s));
  console.log(typeof s, s == 1, s === s, s != s, [s].includes(s));
}

{
  const s = Symbol("a");
  const o = { [s]: 1, x: 2 };
  console.log(Object.getOwnPropertySymbols(o), { ...o }[s], Object.assign({}, o)[s]);
  const { x, ...rest } = o; console.log(rest[s]);
}

{
  const r = Symbol.for("app"); console.log(r === Symbol.for("app"), Symbol.keyFor(r), Symbol.keyFor(Symbol("x")), r.description);
}

{
  console.log(typeof Symbol.toStringTag, Symbol.toPrimitive.toString(), Symbol.hasInstance.description, Symbol.asyncIterator.toString());
}

{
  const o = { [Symbol.toPrimitive](hint) { console.log("hint", hint); return hint === "number" ? 42 : "str"; } };
  console.log(+o, `${o}`, o + "", o * 2, o == "str", String(o), [o].join());
  const bad = { [Symbol.toPrimitive]() { return {}; } };
  try { "" + bad; } catch (e) { console.log(e.message); }
  const notfn = { [Symbol.toPrimitive]: 5 };
  try { "" + notfn; } catch (e) { console.log(e.message); }
  class Even { static [Symbol.hasInstance](n) { return n % 2 === 0; } }
  console.log(2 instanceof Even, 3 instanceof Even);
  const odd = { [Symbol.hasInstance]: (n) => n % 2 };
  console.log(3 instanceof odd, 4 instanceof odd);
}
