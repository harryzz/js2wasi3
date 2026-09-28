// BigInt built-ins (T40): BigInt(…), asIntN/asUintN, the prototype methods, JSON.
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
t(() => BigInt(12));
t(() => BigInt(-0));
t(() => BigInt(2 ** 80));
t(() => BigInt(1.5));
t(() => BigInt("  -123  "));
t(() => BigInt("0x1f"));
t(() => BigInt("1.5"));
t(() => BigInt(true));
t(() => BigInt(undefined));
t(() => BigInt(Symbol("s")));
t(() => BigInt({ valueOf() { return 7n; } }));
t(() => BigInt.asIntN(8, 255n));
t(() => BigInt.asUintN(8, -1n));
t(() => BigInt.asIntN(64, 2n ** 63n));
t(() => BigInt.asUintN(64, -(2n ** 63n)));
t(() => BigInt.asIntN(-1, 1n));
t(() => (255n).toString(16));
t(() => (-255n).toString(2));
t(() => (255n).toString(37));
t(() => (123456789n).toLocaleString());
t(() => (-1234n).toLocaleString());
t(() => 5n.valueOf());
t(() => (2n ** 100n).toString(36));
t(() => BigInt.asIntN(2 ** 53 - 1, -5n));
t(() => BigInt.asUintN(2 ** 53 - 1, 5n));
t(() => BigInt.asUintN(3, 13n) + BigInt.asIntN(3, 13n));
t(() => BigInt("") + BigInt("0b101") + BigInt("0o17") + BigInt("  12\n"));
t(() => BigInt("1e3"));
t(() => BigInt("-0x10"));
t(() => BigInt(" 1 2 "));
t(() => (0n).toString(2) + " " + (-0n).toLocaleString());
t(() => 10n.toString(undefined));
t(() => JSON.stringify({ toJSON() { return 1; }, v: 2n }));
t(() => {
  BigInt.prototype.toJSON = function () {
    return this.toString() + "n";
  };
  return JSON.stringify({ a: 1n, b: [2n] });
});
