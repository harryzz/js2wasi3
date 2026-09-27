// Object.getOwnPropertyNames (T28, for test262's propertyHelper).
const o = { b: 1, a: 2, 1: 3 };
Object.defineProperty(o, "hidden", { value: 4, enumerable: false });
console.log(Object.getOwnPropertyNames(o), Object.keys(o));
console.log(Object.getOwnPropertyNames([5, 6]), Object.getOwnPropertyNames([]));
const a = [1]; a.x = 2;
console.log(Object.getOwnPropertyNames(a));
console.log(Object.getOwnPropertyNames({ [Symbol("s")]: 1, z: 1 }));
console.log(Object.getOwnPropertyNames(function f(x, y) {}));
console.log(Object.getOwnPropertyNames("ab"));
try { Object.getOwnPropertyNames(null); } catch (e) { console.log(e.message); }
class C { m() {} static s() {} }
console.log(Object.getOwnPropertyNames(C.prototype), Object.getOwnPropertyNames(new C()));
