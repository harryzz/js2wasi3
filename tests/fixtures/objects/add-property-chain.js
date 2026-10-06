"use strict";
// Adding a property with a constant key from one site, many times (T90: the site remembers that the
// receiver's prototype chain has no such key), with every change that must make it look again.

function Point(x, y) {
  this.x = x;
  this.y = y;
}

function add(o, v) {
  o.tag = v;
  return o;
}

function tryAdd(o, v) {
  try {
    add(o, v);
    return "ok " + Object.prototype.hasOwnProperty.call(o, "tag") + " " + o.tag;
  } catch (e) {
    return e.constructor.name + ": " + e.message;
  }
}

const log = [];
const proto = { kind: "p" };
function make() {
  return Object.create(proto);
}

// The plain case, repeated.
let sum = 0;
for (let i = 0; i < 100; i++) {
  const p = new Point(i, 2 * i);
  sum += p.x + p.y;
}
console.log("points", sum);
for (let i = 0; i < 3; i++) console.log("plain", tryAdd(make(), i));

// A setter added to the prototype later.
Object.defineProperty(proto, "tag", {
  set(v) {
    log.push("setter " + v);
  },
  configurable: true,
});
console.log("setter", tryAdd(make(), 10), log.join());

// Removed again: adds work again.
delete proto.tag;
for (let i = 0; i < 2; i++) console.log("deleted", tryAdd(make(), 20 + i));

// A read-only key on a prototype further up the chain.
const top = {};
Object.setPrototypeOf(proto, top);
console.log("reparented", tryAdd(make(), 30));
Object.defineProperty(top, "tag", { value: "fixed", writable: false, configurable: true });
console.log("read-only", tryAdd(make(), 31));

// A writable inherited key: an own one is added; then it turns read-only.
Object.defineProperty(top, "tag", { value: "w", writable: true, configurable: true });
console.log("writable", tryAdd(make(), 32), tryAdd(make(), 33));
Object.defineProperty(top, "tag", { writable: false });
console.log("now read-only", tryAdd(make(), 34));
delete top.tag;
console.log("gone", tryAdd(make(), 35));

// setPrototypeOf on the receiver: another chain, one with a setter.
const withSetter = {
  set tag(v) {
    log.push("other " + v);
  },
};
const r = make();
add(make(), 40);
Object.setPrototypeOf(r, withSetter);
console.log("receiver reparented", tryAdd(r, 41), log.join());

// A key added by plain assignment to Object.prototype (the end of every chain).
for (let i = 0; i < 2; i++) console.log("before", tryAdd({}, 50 + i));
Object.defineProperty(Object.prototype, "tag", {
  get() {
    return "getter only";
  },
  configurable: true,
});
console.log("getter only", tryAdd({}, 52));
delete Object.prototype.tag;
console.log("restored", tryAdd({}, 53));

// Frozen, sealed and non-extensible receivers.
console.log("frozen", tryAdd(Object.freeze(make()), 60));
console.log("sealed", tryAdd(Object.seal({}), 61));
console.log("non-extensible", tryAdd(Object.preventExtensions({}), 62));
console.log("extensible again", tryAdd({}, 63));

// An own property: updated, or refused when read-only, or an own setter.
const own = make();
add(own, 70);
add(own, 71);
console.log("own", own.tag);
Object.defineProperty(own, "tag", { writable: false });
console.log("own read-only", tryAdd(own, 72));
const ownSetter = {
  set tag(v) {
    log.push("own " + v);
  },
};
console.log("own setter", tryAdd(ownSetter, 73), log.join());

// A null prototype, and the chain changing through a class's prototype.
console.log("null proto", tryAdd(Object.create(null), 80), tryAdd(Object.create(null), 81));
class A {
  constructor(v) {
    this.tag = v;
  }
}
const as = [];
for (let i = 0; i < 3; i++) as.push(new A(i).tag);
console.log("class", as.join());
Object.defineProperty(A.prototype, "tag", {
  set(v) {
    log.push("class " + v);
  },
});
const a = new A(5);
console.log("class setter", Object.keys(a).length, log.join());

// Arrays and functions as receivers.
const arr = [1, 2];
console.log("array", tryAdd(arr, 90), tryAdd([], 91));
console.log("function", tryAdd(function f() {}, 92));
