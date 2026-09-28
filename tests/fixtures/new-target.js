// `new.target` in ordinary functions and arrows (T38).
function F(x) {
  this.x = x;
  this.nt = new.target;
}
const a = new F(1);
console.log(a.x, a.nt === F, F.call({}, 2) === undefined);

function Mode() {
  return new.target === undefined ? "call" : "new";
}
console.log(Mode(), new Mode() instanceof Mode, typeof new Mode());

// A constructor returning an object.
function Obj() {
  if (new.target) {
    return { made: true };
  }
  return 5;
}
console.log(new Obj(), Obj());

// Without `this`.
function Only() {
  console.log("new.target:", new.target === Only);
}
Only();
new Only();

// Arrows capture it.
function Outer() {
  const f = () => new.target;
  const g = () => () => new.target;
  this.f = f();
  this.g = g()();
}
const o = new Outer();
console.log(o.f === Outer, o.g === Outer, Outer.call({}) === undefined);

// Through a value, `Reflect.construct` and a subclass.
const G = function () {
  return { nt: new.target };
};
console.log(new G().nt === G);
class Base {}
console.log(Reflect.construct(F, [3], Base).nt === Base, Object.getPrototypeOf(Reflect.construct(F, [3], Base)) === Base.prototype);
function Parent() {
  this.who = new.target.name;
}
class Child extends Parent {}
console.log(new Child().who, new Parent().who);

// Classes still work, also through arrows.
class C {
  constructor() {
    const f = () => new.target;
    this.same = f() === new.target;
    this.name = new.target.name;
  }
}
class D extends C {}
console.log(new C(), new D());

// Methods, generators, async functions: `undefined`.
const m = {
  method() {
    return new.target;
  },
};
function* gen() {
  yield new.target;
}
async function af() {
  return new.target;
}
console.log(m.method(), gen().next().value);
af().then((v) => console.log("async", v));

// Nested functions and closures over variables.
function Counter(start) {
  let n = start;
  const inc = () => (new.target ? ++n : -1);
  this.inc = inc;
}
const c = new Counter(10);
console.log(c.inc(), c.inc());
function makeCtor(tag) {
  return function Tagged() {
    this.tag = tag;
    this.ok = new.target === Tagged;
  };
}
const T = makeCtor("t");
console.log(new T(), T.length, T.name);
console.log(Reflect.construct(function () {}, [], T) instanceof T);

// Static blocks and static fields: `undefined`, also inside a constructor.
let sv = null, sw = null, sx = null;
class SC { static { sv = new.target; } static f = new.target; }
function SF() { class SD { static { sw = new.target; } static g = (() => new.target)(); } sx = SD.g; return new.target; }
console.log(sv, SC.f, new SF() === undefined, sw, sx);
