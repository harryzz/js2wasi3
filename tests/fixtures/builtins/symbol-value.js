// `Symbol` as a value (T49).
console.log(typeof Symbol);
const S = Symbol;
const a = S("x");
console.log(typeof a, a.toString(), a.description, S().description);
console.log(Symbol.name, Symbol.length);
console.log(Symbol.prototype.constructor === Symbol, Object(a).constructor === Symbol);
console.log(a.constructor === Symbol);
try {
  new S();
} catch (e) {
  console.log(e instanceof TypeError, e.message);
}
try {
  Reflect.construct(Symbol, []);
} catch (e) {
  console.log(e.constructor === TypeError, e.message);
}
console.log(S.iterator === Symbol.iterator, S.asyncIterator === Symbol.asyncIterator);
console.log(S.for("k") === Symbol.for("k"), S.keyFor(S.for("k")));
const d = Object.getOwnPropertyDescriptor(Symbol, "iterator");
console.log(d.writable, d.enumerable, d.configurable, typeof d.value);
const h = Object.getOwnPropertyDescriptor(Symbol, "hasInstance");
console.log(h.writable, h.enumerable, h.configurable, h.value === Symbol.hasInstance);
console.log(Object.getOwnPropertyNames(Symbol).includes("toPrimitive"));
class X extends Symbol {}
try {
  new X();
} catch (e) {
  console.log(e.constructor === TypeError, e.message);
}
console.log([1, 2].map(Symbol).map((s) => s.toString()));
console.log(Symbol.call(null, "c").toString(), Symbol.apply(null, [3]).toString());
try {
  Symbol(Symbol());
} catch (e) {
  console.log(e.message);
}
console.log(Object.prototype.toString.call(Object(a)), Object.prototype.toString.call(a));
console.log(Symbol.prototype[Symbol.toStringTag], typeof Symbol.prototype[Symbol.toPrimitive]);
console.log(Object.getPrototypeOf(Symbol) === Function.prototype);
console.log(Symbol);
const tp = Symbol.prototype[Symbol.toPrimitive];
console.log(tp.name, tp.length, tp.call(a) === a, Object(a)[Symbol.toPrimitive]("default") === a);
try {
  tp.call(1);
} catch (e) {
  console.log(e.message);
}
try {
  `${Object(a)}`;
} catch (e) {
  console.log(e.message);
}
