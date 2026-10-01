// A function's `constructor` is `Function` (T61), also when a function is made before
// `Object.prototype` (Function.prototype was made twice) and for async/generator functions.
const f = function () {};
console.log(typeof f.constructor, f.constructor === Function, f.constructor.name);
const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
console.log(typeof AsyncFunction, (async () => {}) instanceof AsyncFunction);
const o = {};
console.log(o.constructor === Object, Object.getPrototypeOf(f) === Function.prototype);
