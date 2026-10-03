// T63: Object.getOwnPropertyDescriptors, the %TypedArray%.prototype[Symbol.toStringTag] getter,
// console._stdout/_stderr.

const o = { a: 1, get b() { return 2; }, [Symbol.for("s")]: 3 };
Object.defineProperty(o, "hidden", { value: 4 });
Object.defineProperty(o, "__proto__", { value: 5, enumerable: true });
const ds = Object.getOwnPropertyDescriptors(o);
console.log(Object.keys(ds), Object.getOwnPropertySymbols(ds).length);
console.log(ds.a, ds.hidden, ds.__proto__, Object.getPrototypeOf(ds) === Object.prototype);
console.log(typeof ds.b.get, ds.b.set, ds.b.enumerable, ds.b.configurable);
console.log(Object.getOwnPropertyDescriptors("hi"));
console.log(Object.keys(Object.getOwnPropertyDescriptors([1, 2])));
try {
  Object.getOwnPropertyDescriptors(null);
} catch (e) {
  console.log(e instanceof TypeError, e.message);
}
// A shallow clone with accessors, as readable-stream does.
const clone = Object.create(Object.getPrototypeOf(o), Object.getOwnPropertyDescriptors(o));
console.log(clone.a, clone.b, clone.hidden);
console.log(Object.getOwnPropertyDescriptors.length, Object.getOwnPropertyDescriptors.name);

// safe-stable-stringify's check.
const TypedArrayPrototype = Object.getPrototypeOf(Object.getPrototypeOf(new Uint8Array()));
const desc = Object.getOwnPropertyDescriptor(TypedArrayPrototype, Symbol.toStringTag);
console.log(typeof desc.get, desc.set, desc.enumerable, desc.configurable, desc.get.name);
const tag = desc.get;
console.log(tag.call(new Uint8Array(2)), tag.call(new Float64Array(1)), tag.call([]), tag.call({}), tag.call(1));
console.log(new Int16Array(1)[Symbol.toStringTag], Object.prototype.toString.call(new Uint32Array()));
console.log(Uint8Array.prototype[Symbol.toStringTag]);

// winston's Console transport.
if (console._stdout) {
  console._stdout.write("to stdout\n");
}
console._stderr.write("to stderr\n");
console.log(console._stdout === process.stdout, console._stderr === process.stderr);
