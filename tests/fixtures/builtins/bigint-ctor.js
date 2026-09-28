// BigInt as a value (T40): not a constructor, wrapper objects, prototype, Number(…) of a BigInt.
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
t(() => typeof BigInt);
t(() => BigInt.name + " " + BigInt.length);
t(() => (5n).constructor === BigInt);
t(() => new BigInt(1));
t(() => [1, 2].map(BigInt));
t(() => Object(1n));
t(() => typeof Object(1n));
t(() => Object(1n) + 1n);
t(() => Object.prototype.toString.call(1n));
t(() => Number(1n));
t(() => Number(2n ** 1100n));
t(() => Number(-(2n ** 64n) - 1n));
t(() => Number(Object(3n)));
t(() => parseInt("12n"));
t(() => BigInt.prototype.toString.call(Object(5n)));
t(() => BigInt.prototype.toString.call(5));
t(() => BigInt.prototype.valueOf.call("x"));
t(() => 1n instanceof BigInt);
t(() => Object(1n) instanceof BigInt);
t(() => Object.getPrototypeOf(1n) === BigInt.prototype);
t(() => String(Object(10n)));
