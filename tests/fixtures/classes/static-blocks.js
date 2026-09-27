// `static {}` blocks and private names in optional chains (T31).
const order = [];
class Base {
  static base() {
    return "base";
  }
}
class A extends Base {
  static x = (order.push("x"), 1);
  static {
    order.push("block 1");
    var local = this.x + 1;
    this.y = local;
    this.fromSuper = super.base();
    function helper() {
      return local * 10;
    }
    this.helper = helper;
    let counter = 0;
    this.inc = () => ++counter;
  }
  static z = (order.push("z"), A.y + 1);
  static #secret = 42;
  static {
    order.push("block 2");
    A.reveal = (o) => o.#secret;
    this.selfName = this.name;
  }
}
console.log(order.join(", "));
console.log(A.x, A.y, A.z, A.fromSuper, A.helper(), A.inc(), A.inc(), A.selfName);
console.log(A.reveal(A), typeof local);
try {
  A.reveal({});
} catch (e) {
  console.log(e.constructor.name, e.message);
}

// A throwing static block stops the class definition.
let B;
try {
  B = class {
    static {
      throw new Error("from static block");
    }
  };
} catch (e) {
  console.log(e.message, B);
}

// `o?.#x`, `o?.a.#x`, `o?.#m()`.
class P {
  #v;
  #m() {
    return "m" + this.#v;
  }
  constructor(v) {
    this.#v = v;
    this.self = this;
  }
  static read(o) {
    return [o?.#v, o?.self.#v, o?.#m(), o?.self?.#m()];
  }
}
console.log(P.read(new P(7)));
console.log(P.read(null));
console.log(P.read(undefined));
try {
  P.read({ self: null });
} catch (e) {
  console.log(e.constructor.name, e.message);
}
