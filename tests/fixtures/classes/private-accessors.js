// Private getters and setters (T31): instance and static, pairs, brand checks, compound
// assignment, destructuring targets, `#x in o`, missing halves.
function attempt(f) {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.constructor.name, e.message);
  }
}

class Counter {
  #n = 0;
  get #count() {
    return this.#n;
  }
  set #count(v) {
    console.log("set", v);
    this.#n = v;
  }
  get #onlyGet() {
    return "g";
  }
  set #onlySet(v) {
    this.#n = v * 10;
  }
  static #made = 0;
  static get #total() {
    return Counter.#made;
  }
  static set #total(v) {
    Counter.#made = v;
  }
  constructor() {
    Counter.#total += 1;
  }
  bump() {
    this.#count++;
    this.#count += 5;
    [this.#count] = [this.#count * 2];
    ({ a: this.#count } = { a: this.#count + 1 });
    return this.#count;
  }
  names() {
    return [
      #count in this,
      #onlyGet in this,
      #onlySet in {},
      Object.getOwnPropertyNames(this).length,
    ];
  }
  writeGetter() {
    this.#onlyGet = 1;
  }
  readSetter() {
    return this.#onlySet;
  }
  useSetter() {
    this.#onlySet = 4;
    return this.#n;
  }
  static total() {
    return Counter.#total;
  }
  static readFrom(o) {
    return o.#count;
  }
  static writeTo(o) {
    o.#count = 1;
  }
  static staticFrom(o) {
    return o.#total;
  }
  static writeStatic() {
    Counter.#total = 99;
    return Counter.#made;
  }
}

const c = new Counter();
new Counter();
attempt(() => c.bump());
attempt(() => c.names());
attempt(() => c.writeGetter());
attempt(() => c.readSetter());
attempt(() => c.useSetter());
attempt(() => Counter.total());
attempt(() => Counter.readFrom({}));
attempt(() => Counter.writeTo({}));
attempt(() => Counter.staticFrom(c));
attempt(() => Counter.writeStatic());

// Accessors see `this` of subclasses and run on instances made by `super()`.
class Base {
  #v = 1;
  get #double() {
    return this.#v * 2;
  }
  double() {
    return this.#double;
  }
}
class Derived extends Base {
  constructor() {
    super();
  }
}
attempt(() => new Derived().double());
attempt(() => Base.prototype.double.call({}));

// The getter's name.
class Named {
  static get #g() {
    return 0;
  }
  static fn() {
    return Object.getOwnPropertyNames(Named);
  }
}
attempt(() => Named.fn());
