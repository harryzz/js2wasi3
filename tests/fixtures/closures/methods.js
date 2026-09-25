// Methods and `this`.
const counter = {
  count: 0,
  inc() {
    this.count++;
    return this;
  },
  get() {
    return this.count;
  },
  arrow: () => typeof this,
  later() {
    return () => this.count * 2;
  },
};
counter.inc().inc();
console.log(counter.get(), counter.arrow(), counter.later()());
console.log(counter["get"](), counter?.get(), counter.missing?.());

function whoAmI() {
  return this === undefined ? "undefined" : this.name;
}
const o1 = { name: "o1", whoAmI };
const o2 = { name: "o2", f: whoAmI };
console.log(whoAmI(), o1.whoAmI(), o2.f(), whoAmI.call(o2), whoAmI.call());

function add(a, b) {
  return this.base + a + b;
}
console.log(add.call({ base: 100 }, 1, 2));

const detached = counter.get;
try {
  detached();
} catch (e) {
  console.log(e.message);
}

console.log(counter.inc.name, counter.arrow.name, whoAmI.name, whoAmI.length, add.length);
console.log(typeof whoAmI.prototype, whoAmI.prototype.constructor === whoAmI, counter.inc.prototype);
console.log(whoAmI instanceof Function, whoAmI instanceof Object, Object.getPrototypeOf(whoAmI) === Function.prototype);
console.log(counter.get);
