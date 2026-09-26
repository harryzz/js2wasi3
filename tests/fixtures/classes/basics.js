class Point {
  constructor(x, y) {
    this.x = x;
    this.y = y;
  }
  norm2() {
    return this.x * this.x + this.y * this.y;
  }
  static origin() {
    return new Point(0, 0);
  }
  get sum() {
    return this.x + this.y;
  }
  set sum(v) {
    this.x = v - this.y;
  }
}
const p = new Point(3, 4);
console.log(p.x, p.y, p.norm2(), p.sum);
p.sum = 10;
console.log(p.x, p.sum);
console.log(Point.origin().norm2());
console.log(typeof Point, Point.name, Point.length, p instanceof Point, p.constructor === Point);
console.log(Object.keys(p), Object.getPrototypeOf(p) === Point.prototype);
try {
  Point(1, 2);
} catch (e) {
  console.log(e.message);
}

class Counter {
  count = 0;
  step = 1;
  inc() {
    this.count += this.step;
    return this;
  }
}
const c = new Counter();
c.inc().inc();
console.log(c.count, Object.keys(c));

class Animal {
  constructor(name) {
    this.name = name;
  }
  speak() {
    return this.name + " makes a sound";
  }
  toString() {
    return "Animal(" + this.name + ")";
  }
}
class Dog extends Animal {
  constructor(name, breed) {
    super(name);
    this.breed = breed;
  }
  speak() {
    return super.speak() + " (woof)";
  }
}
const d = new Dog("Rex", "lab");
console.log(d.speak(), d.breed, d instanceof Dog, d instanceof Animal);
console.log(`${d}`, "" + d);
class Puppy extends Dog {}
const pup = new Puppy("Bit", "pug");
console.log(pup.speak(), pup.breed, Object.getPrototypeOf(Puppy) === Dog);

function Legacy(a) {
  this.a = a;
}
Legacy.prototype.get = function () {
  return this.a;
};
const l = new Legacy(7);
console.log(l.get(), l instanceof Legacy);
