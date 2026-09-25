// Prototype chains: Object.create, getPrototypeOf, shadowing, instanceof, built-in prototypes.
const animal = { kind: "animal", legs: 4 };
const dog = Object.create(animal);
dog.kind = "dog";
const puppy = Object.create(dog);
puppy.age = 1;
console.log(puppy.kind, puppy.legs, puppy.age, dog.age);
console.log(Object.getPrototypeOf(puppy) === dog, Object.getPrototypeOf(dog) === animal);
console.log(Object.getPrototypeOf(animal) === Object.prototype, Object.getPrototypeOf(Object.prototype));

// Changing a prototype is visible through the chain; writes create own properties.
animal.legs = 3;
puppy.legs = 5;
console.log(dog.legs, puppy.legs, animal.legs);

// A null prototype.
const dict = Object.create(null);
dict.key = "value";
console.log(dict.key, dict.toString, Object.getPrototypeOf(dict) === null);

// Object.prototype is shared by all plain objects.
Object.prototype.everywhere = "hi";
const fresh = {};
console.log(fresh.everywhere, dict.everywhere, "everywhere" in fresh);
delete Object.prototype.everywhere;
console.log(fresh.everywhere);

// __proto__ assignment runs the setter.
const target = {};
target.__proto__ = animal;
console.log(target.kind, Object.getPrototypeOf(target) === animal);

// instanceof with built-in constructors.
console.log(fresh instanceof Object, dict instanceof Object, puppy instanceof Object);
console.log(5 instanceof Object, "s" instanceof String, null instanceof Object);
const e = new TypeError("t");
console.log(e instanceof TypeError, e instanceof Error, e instanceof Object, e instanceof RangeError);
const fake = Object.create(RangeError.prototype);
console.log(fake instanceof RangeError, fake instanceof Error, fake.name, "" + fake);

// Primitives' prototypes.
console.log(Object.getPrototypeOf("x") === String.prototype, Object.getPrototypeOf(1) === Number.prototype);
console.log(Object.getPrototypeOf(true) === Boolean.prototype, String.prototype === Number.prototype);
