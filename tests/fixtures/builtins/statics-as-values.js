// Built-in statics read as values (T28, for test262's propertyHelper).
var isArray = Array.isArray;
var defineProperty = Object.defineProperty;
var getOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
var getOwnPropertyNames = Object.getOwnPropertyNames;
console.log(isArray([]), isArray({}), isArray.name, isArray.length);
var o = {};
defineProperty(o, "x", { value: 1 });
console.log(getOwnPropertyDescriptor(o, "x"), defineProperty.name, defineProperty.length);
console.log(getOwnPropertyNames(o), getOwnPropertyNames.length, getOwnPropertyDescriptor.length);
var join = Function.prototype.call.bind(Array.prototype.join);
var push = Function.prototype.call.bind(Array.prototype.push);
var hasOwn = Function.prototype.call.bind(Object.prototype.hasOwnProperty);
var list = [];
push(list, "a");
push(list, "b");
console.log(join(list, "; "), hasOwn(o, "x"), hasOwn(o, "y"));
for (const f of [Object.keys, Object.values, Object.entries, Object.freeze, Object.isFrozen, Object.hasOwn]) {
  console.log(f.name, f.length);
}
console.log(Object.keys === Object.keys, [Object.getPrototypeOf][0]([]) === Array.prototype);
const A = Array;
console.log(Object.getOwnPropertyDescriptor(A, "isArray").writable, A.isArray([1]));
