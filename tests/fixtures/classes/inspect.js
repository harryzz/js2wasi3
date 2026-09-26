class A {
  constructor(x) {
    this.x = x;
  }
}
class B extends A {
  y = [1, 2];
}
class Empty {}
const Anon = [class {}][0];
console.log(A, B, Empty, Anon, class extends A {});
console.log(new A(1), new B(2), new Empty(), new Anon());
console.log({ a: new A({ deep: new B(3) }) }, [new A(1), new Empty()]);
console.log({ l1: { l2: { l3: new A(1), l4: {} } } });
console.log(Object.create(null), Object.create(new A(5)));
const acc = {
  get a() {
    return 1;
  },
  set b(v) {},
  get c() {
    return 1;
  },
  set c(v) {},
  d: 4,
};
console.log(acc);
class Statics {
  static count = 1;
  static label = "s";
}
console.log(Statics);
function f() {}
f.extra = true;
console.log(f, [f]);
const obj = Object.create(null);
obj.k = 1;
console.log(obj);
console.log(Object.getOwnPropertyDescriptor(A, "prototype"), Object.getOwnPropertyDescriptor(A, "name"));
console.log(Object.getOwnPropertyDescriptor({ get g() { return 1; } }, "g"));
