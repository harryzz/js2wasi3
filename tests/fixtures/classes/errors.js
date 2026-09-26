const t = (f) => {
  try {
    f();
    console.log("no error");
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
class A {
  constructor(x) {
    this.x = x;
  }
  get g() {
    return 1;
  }
}
class B extends A {}
t(() => A());
t(() => {
  class C extends B {
    constructor() {
      this.x = 1;
    }
  }
  new C();
});
t(() => {
  class C extends B {
    constructor() {}
  }
  new C();
});
t(() => {
  class C extends B {
    constructor() {
      super();
      super();
    }
  }
  new C();
});
t(() => {
  class C extends B {
    constructor() {
      super();
      return 5;
    }
  }
  new C();
});
t(() => {
  class C extends B {
    constructor() {
      return {};
    }
  }
  console.log(new C() instanceof C);
});
t(() => {
  const f = () => 1;
  new f();
});
t(() => {
  const o = { m() {} };
  new o.m();
});
t(() => {
  const n = 5;
  new n();
});
t(() => {
  class C extends 5 {}
});
t(() => {
  const v = {};
  class C extends v {}
});
t(() => {
  function F() {}
  F.prototype = 3;
  class C extends F {}
});
t(() => {
  const x = {};
  ({}) instanceof x;
});
t(() => {
  const F = () => 1;
  ({}) instanceof F;
});
t(() => {
  class C extends null {}
  new C();
});
t(() => {
  const o = new A();
  o.g = 2;
});
t(() => {
  new (class X {
    static m() {}
  }).m();
});
t(() => new B(1).x.y.z);
function Plain() {
  return 5;
}
console.log(typeof new Plain(), new Plain() instanceof Plain);
function Obj() {
  return { made: true };
}
console.log(new Obj().made, new Obj() instanceof Obj);
