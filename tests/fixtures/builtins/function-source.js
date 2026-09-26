// T25 slice 3: Function.prototype as a function (F47), Function.prototype.toString (source
// text; native code for built-ins and bound functions), names from computed keys (F59).

{
  const FP = Object.getPrototypeOf(function(){});
  console.log(typeof FP, FP.name === "", FP.length, FP());
  console.log(FP);
  console.log(Object.getPrototypeOf(FP) === Object.prototype);
  console.log(FP === Function.prototype);
  console.log(Object.keys(FP));
  try { new FP(); } catch (e) { console.log(e.message); }
  console.log([FP]);
}

{
  function f(a, b) { return a + b; }
  console.log(f.toString());
  console.log("" + f);
  class C { m() { return 1; } get g() { return 2; } static s(x) {} }
  console.log(C.toString()); console.log(C.prototype.m.toString());
  console.log(Object.getOwnPropertyDescriptor(C.prototype, "g").get.toString());
  const a = (x) => x * 2; console.log(String(a));
  const o = { *gen() {}, f: function () {} };
  console.log(o.gen.toString(), o.f.toString());
  console.log(f.bind(null).toString());
  console.log(Math.max.toString(), [].map.toString(), Function.prototype.toString());
  console.log(`${f}`);
}

{
  class C { static  s(x) {} static get g() { return 1; } async am() {} *gm() {} x = () => 1; }
  console.log(C.s.toString()); console.log(Object.getOwnPropertyDescriptor(C, "g").get.toString());
  console.log(C.prototype.am.toString(), C.prototype.gm.toString(), new C().x.toString());
  const o = { get a() { return 1; }, set a(v) {}, m ( a ) { }, * ag() {}, 'q r'() {} };
  const d = Object.getOwnPropertyDescriptor(o, "a");
  console.log(d.get.toString(), d.set.toString(), o.m.toString(), o.ag.toString(), o["q r"].toString());
  function /* c */ g /* d */ ( ) { } console.log(g.toString());
  const e = class   extends C {}; console.log(e.toString());
  async function af() {} function* gf() {} console.log(af.toString(), gf.toString());
  console.log(String(function(){}), `${async (a) => a}`);
  try { Function.prototype.toString.call({}); } catch (x) { console.log(x.message); }
  console.log(parseInt.toString(), Math.max.toString());
}

{
  const k = "dyn", s = Symbol("tag"), s2 = Symbol("acc"), e = Symbol(), n = 42;
  const o = {
    [k]() {}, [k + 2]: function () {}, [s]: () => 1, [e]() {}, [n]: class {},
    get [k + "g"]() { return 1; }, set [s2](v) {}, 7() {}, 1.5e3: () => 0,
    [k + "named"]: function named() {},
  };
  console.log(o.dyn.name, o.dyn2.name, o[s].name, JSON.stringify(o[e].name), o[42].name, o[7].name, o[1500].name, o.dynnamed.name);
  console.log(Object.getOwnPropertyDescriptor(o, "dyng").get.name, Object.getOwnPropertyDescriptor(o, s2).set.name);
  class C {
    [k]() { return "m"; }
    static [s]() {}
    get [k + "x"]() { return 2; }
    static [n] = () => 3;
    static [k + "f"] = function () {};
  }
  const c = new C();
  console.log(c.dyn.name, c.dyn(), C[s].name, Object.getOwnPropertyDescriptor(C.prototype, "dynx").get.name, C[42].name, C[42](), C.dynf.name);
  console.log({ [k]() {}, [n]: () => 1 }, C.prototype.dyn, o[s]);
}
