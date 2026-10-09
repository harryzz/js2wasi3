// T97: a typed array's `.length` takes a fast path while %TypedArray%.prototype's getter is the built-in one
// and nothing on the chain shadows it. Every override must still be seen.
const TA = Object.getPrototypeOf(Uint8Array.prototype);
const u8 = new Uint8Array(4), f64 = new Float64Array(3), buf = Buffer.alloc(5);
class Sub extends Uint16Array {}
const sub = new Sub(6);
const all = () => [u8.length, f64.length, buf.length, sub.length].join(",");
console.log("plain", all());
for (let i = 0; i < 3; i++) console.log("again", all()); // the cached path

// An own `length` on one instance.
const own = new Uint8Array(2);
Object.defineProperty(own, "length", { value: 99 });
console.log("own", own.length, u8.length);

// While Uint8Array.prototype's or %TypedArray%.prototype's `length` is overridden, node's own console.log
// stops writing (it uses Buffer lengths), so results are collected and printed after restoring.
const out = [];
const log = (...a) => out.push(a.join(" "));

// `length` added on a subclass prototype, then on Uint8Array.prototype.
Object.defineProperty(Sub.prototype, "length", { get() { return -1; }, configurable: true });
log("sub proto", sub.length, u8.length, all());
delete Sub.prototype.length;
log("sub proto deleted", sub.length);
Object.defineProperty(Uint8Array.prototype, "length", { get() { return -2; }, configurable: true });
log("u8 proto", u8.length, buf.length, f64.length);
delete Uint8Array.prototype.length;
log("u8 proto deleted", u8.length, buf.length);

// The built-in getter redefined, then deleted.
const desc = Object.getOwnPropertyDescriptor(TA, "length");
Object.defineProperty(TA, "length", { get() { return "redefined:" + this.byteLength; }, configurable: true });
log("redefined", u8.length, f64.length, buf.length);
delete TA.length;
log("deleted", u8.length, f64.length, String(buf.length));
Object.defineProperty(TA, "length", desc);
log("restored", all());

// A typed array whose prototype changes.
const moved = new Int8Array(7);
Object.setPrototypeOf(moved, { get length() { return "mine"; } });
log("moved", moved.length, new Int8Array(7).length);
Object.setPrototypeOf(moved, Int8Array.prototype);
log("moved back", moved.length);

// Lengths after subarray / set, in a loop that alternates Buffer and Uint8Array receivers.
let sum = 0;
for (let i = 0; i < 1000; i++) sum += (i & 1 ? buf : u8.subarray(i & 3)).length;
log("loop", sum);
console.log(out.join("\n"));
