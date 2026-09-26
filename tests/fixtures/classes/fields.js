let made = 0;
function tag(s) {
  made++;
  return s;
}
class Base {
  kind = tag("base");
  id = made;
  handler = () => this.kind;
  constructor() {
    this.ready = true;
  }
}
class Derived extends Base {
  extra = this.kind + "+";
  constructor(n) {
    console.log("before super", typeof n);
    super();
    this.n = n;
  }
}
const d = new Derived(5);
console.log(d.kind, d.id, d.extra, d.n, d.ready, d.handler());
console.log(Object.keys(d));
const h = d.handler;
console.log(h());

class Statics {
  static count = 1;
  static double = Statics.count * 2;
  static self = this;
  static inc() {
    return ++this.count;
  }
}
console.log(Statics.count, Statics.double, Statics.self === Statics, Statics.inc(), Statics.count);

const Anon = class {
  m() {
    return "anon";
  }
};
const Named = class Inner {
  who() {
    return Inner.name;
  }
};
console.log(Anon.name, new Anon().m(), Named.name, new Named().who());

function makeCounter(start) {
  class C {
    value = start;
    next() {
      return ++this.value;
    }
    static from(x) {
      return new C() instanceof C ? x : 0;
    }
  }
  return new C();
}
const mc = makeCounter(10);
console.log(mc.next(), mc.next(), makeCounter(0).next());

class Shape {
  constructor() {
    this.name = new.target.name;
  }
  area() {
    return 0;
  }
  describe() {
    return `${this.name} with area ${this.area()}`;
  }
}
class Square extends Shape {
  constructor(s) {
    super();
    this.s = s;
  }
  area() {
    return this.s * this.s;
  }
}
class Circle extends Shape {
  constructor(r) {
    super();
    this.r = r;
  }
  area() {
    return 3 * this.r * this.r;
  }
  describe() {
    const base = () => super.describe();
    return "(" + base() + ")";
  }
}
for (const s of [new Square(2), new Circle(1), new Shape()]) {
  console.log(s.describe());
}

class Returner {
  constructor(o) {
    if (o) return o;
    this.plain = true;
  }
}
console.log(new Returner().plain, new Returner({ other: 1 }).other, new Returner(5).plain);
