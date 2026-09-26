// T25 slice 4: built-in constructors as values (F81/F88/F91/F97), their prototypes' `constructor`
// (F37), JSON's circular message through built-in prototypes (F95), Object(v) and Boolean(v).

{
  const o = { a: 1 };
  console.log(Boolean(1), Boolean(""), Boolean(), Object(null), Object(o) === o, new Object(), new Object(o) === o);
}

{
  console.log([1, "2", 0].map(String), ["1", "x", " 2 "].map(Number), [0, 1, ""].map(Boolean));
  console.log([].constructor === Array, ({}).constructor === Object, "s".constructor === String, (5).constructor === Number);
  console.log(new Map().constructor === Map, new Set().constructor === Set, true.constructor === Boolean);
  const A = Array, O = Object, M = Map, S = Set;
  console.log(A(3).length, new A(1, 2), A.from("ab"), A.of(7), O.assign({}, { a: 1 }), O.fromEntries([["k", 1]]));
  console.log(new M([[1, 2]]), new S([1, 1, 2]), M.groupBy([1, 2, 3], (x) => x % 2));
  console.log(String.name, String.length, Number.length, Array.length, Object.length, Map.length, Set.length, Boolean.length);
  console.log(typeof String, Array.prototype === A.prototype, Object.getPrototypeOf([]) === A.prototype);
  console.log(String, [Number, Map]);
  try { M(); } catch (e) { console.log(e.message); }
  try { S([]); } catch (e) { console.log(e.message); }
  console.log(String.fromCharCode(65), Number.isInteger(2));
  const ctors = [Object, Array, String];
  console.log(ctors.map((c) => c.name).join());
  class P {} console.log(new P().constructor === P, typeof A.from);
}

{
  const e = new Error("x"); e.self = e;
  try { JSON.stringify(e); } catch (x) { console.log(x.message); }
  const m = new Map(); const o = { m }; m.o = o; o.me = o;
  try { JSON.stringify({ a: [o] }); } catch (x) { console.log(x.message); }
  console.log([].constructor.name, "s".constructor.name, (1).constructor.name, ({}).constructor.name, new Set().constructor.name);
}

{
  const xs = [1, 2, 3]; console.log(Array(...xs), new Array(...[4]), Array.apply(null, [5, 6]));
  console.log(String.name, Array.length, Map.name);
}
