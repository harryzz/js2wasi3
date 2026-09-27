// An array's `length` through Object.defineProperty and assignment (T32): ArraySetLength's
// conversions, RangeError, non-writable length, shrinking past non-configurable elements.
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.constructor.name + ": " + e.message);
  }
};

// The value is converted twice (ToUint32, ToNumber), before any other check.
let calls = 0;
const counted = {
  valueOf() {
    calls++;
    return 2;
  },
};
const a = [1, 2, 3, 4];
t(() => Object.defineProperty(a, "length", { value: counted }));
t(() => [a, calls]);
calls = 0;
a.length = counted;
t(() => [a, calls]);
t(() => Object.defineProperty(a, "length", { value: -1 }));
t(() => Object.defineProperty(a, "length", { value: 1.5 }));
t(() => Object.defineProperty(a, "length", { value: 2 ** 32 }));
t(() => {
  a.length = "x";
});
t(() => Object.defineProperty(a, "length", { value: "3" }));
t(() => a);
t(() => Object.defineProperty(a, "length", { configurable: true }));
t(() => Object.defineProperty(a, "length", { enumerable: true }));
t(() => Object.defineProperty(a, "length", { get() {} }));
t(() => Object.defineProperty(a, "length", { value: 3, enumerable: false, configurable: false }));
t(() => Object.getOwnPropertyDescriptor(a, "length"));

// Non-writable length.
const b = [1, 2];
Object.defineProperty(b, "length", { writable: false });
t(() => Object.getOwnPropertyDescriptor(b, "length"));
t(() => b.push(3));
t(() => b.pop());
t(() => {
  b[5] = 1;
});
t(() => {
  b[1] = "ok";
  return b;
});
t(() => {
  b.length = 1;
});
t(() => {
  b.length = 2;
});
t(() => Object.defineProperty(b, "5", { value: 1 }));
t(() => Object.defineProperty(b, "length", { value: 2 }));
t(() => Object.defineProperty(b, "length", { value: 1 }));
t(() => Object.defineProperty(b, "length", { writable: true }));
t(() => Object.defineProperty(b, "length", { value: -1 }));
t(() => b.shift());
t(() => b.unshift(0));
t(() => b.splice(0, 1));
t(() => b);
t(() => Object.isFrozen(b));

// Shrinking stops at a non-configurable element.
const c = [1, 2, 3, 4, 5];
Object.defineProperty(c, "1", { value: 2, configurable: false });
t(() => Object.defineProperty(c, "length", { value: 0 }));
t(() => [c, c.length]);
const d = [1, 2, 3, 4, 5];
Object.defineProperty(d, "2", { value: 3, configurable: false });
t(() => {
  d.length = 1;
});
t(() => [d, d.length]);
// `writable: false` with a failing shrink still makes it read-only.
const e = [1, 2, 3];
Object.defineProperty(e, "1", { value: 2, configurable: false });
t(() => Object.defineProperty(e, "length", { value: 0, writable: false }));
t(() => [e, Object.getOwnPropertyDescriptor(e, "length")]);

// Growing.
const g = [1];
Object.defineProperty(g, "length", { value: 3 });
t(() => g);
g.length = 5;
t(() => [g, g.length]);
