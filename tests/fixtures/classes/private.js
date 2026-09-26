const t = (f) => {
  try {
    f();
    console.log("ok");
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
class Account {
  #balance = 0;
  #log = [];
  static #count = 0;
  static #bump() {
    return ++Account.#count;
  }
  constructor(owner) {
    this.owner = owner;
    this.id = Account.#bump();
  }
  #record(what) {
    this.#log.push(what);
  }
  deposit(n) {
    this.#balance += n;
    this.#record("+" + n);
    return this;
  }
  withdraw(n) {
    if (n > this.#balance) throw new RangeError("insufficient funds");
    this.#balance -= n;
    this.#record("-" + n);
    return this;
  }
  get balance() {
    return this.#balance;
  }
  history() {
    return this.#log.join(" ");
  }
  static isAccount(o) {
    return #balance in o;
  }
  static peek(o) {
    return o.#balance;
  }
  static poke(o) {
    o.#balance = 1;
  }
  static callRecord(o) {
    o.#record("x");
  }
  static count() {
    return Account.#count;
  }
  counter() {
    let n = 0;
    return () => {
      this.#balance++;
      return ++n;
    };
  }
}
const a = new Account("ann");
a.deposit(10).withdraw(3).deposit(5);
console.log(a.balance, a.history(), a.id, new Account("bob").id, Account.count());
t(() => a.withdraw(100));
console.log(Account.isAccount(a), Account.isAccount({}), Object.keys(a), a);
t(() => Account.peek({}));
t(() => Account.poke({}));
t(() => Account.callRecord({}));
t(() => Account.peek(undefined));
t(() => Account.isAccount(1));
const c = a.counter();
c();
c();
console.log(a.balance, Object.isFrozen(Object.freeze(a)));
a.deposit(1);
console.log(a.balance);

class Base {
  #secret = "base";
  reveal() {
    return this.#secret;
  }
}
class Sub extends Base {
  #secret = "sub";
  revealSub() {
    return this.#secret;
  }
}
const s = new Sub();
console.log(s.reveal(), s.revealSub());
class Pt {
  #x;
  #y;
  constructor(x, y) {
    this.#x = x;
    this.#y = y;
  }
  equals(o) {
    return #x in o && this.#x === o.#x && this.#y === o.#y;
  }
  static #origin = new Pt(0, 0);
  static origin() {
    return Pt.#origin;
  }
}
console.log(new Pt(1, 2).equals(new Pt(1, 2)), new Pt(1, 2).equals({}), Pt.origin().equals(new Pt(0, 0)));
class M {
  #m() {}
  static w(o) {
    o.#m = 1;
  }
}
t(() => M.w(new M()));
