// BigInt values (T40): literals, typeof, equality, truthiness, printing, ToString, Map/Set keys.
const a = 123n;
console.log(a, typeof a, 0n, -5n, 0x1fn, 0o17n, 0b101n, 1_000_000n);
console.log(12345678901234567890123456789012345678901234567890n, -(2n ** 64n));
console.log(typeof a === "bigint", typeof 1 === "bigint", typeof -a);

// Equality: by value.
const big = 2n ** 100n;
console.log(a === 123n, a !== 123n, a === 124n, big === 2n ** 100n, 0n === -0n);
console.log(a == 123, a == "123", a == 123.5, a == " 123 ", a == "0x7b", a == "abc", 1n == true);
console.log(0n == "", 0n == false, a == null, a == undefined, a != 123, big == 2 ** 100);
console.log(Object.is(a, 123n), Object.is(0n, -0n), [1n, 2n, 3n].includes(2n), [1n, 2n].indexOf(2n));
console.log(1n === 1, 1n == 1, [1n].includes(1), [1].includes(1n));

// Truthiness.
console.log(!!0n, !!1n, !!-1n, 0n ? "y" : "n", 0n || "or", 1n && "and", 0n ?? "nn");

// ToString.
console.log(`${a}`, "x" + a, a + "x", String(a), `${-0n}`, "" + big, [a, -a].join("|"));

// Printing inside values.
console.log([1n, -2n, { a: 3n, b: [4n] }]);
console.log(new Map([[1n, 2n]]), new Set([big]));

// Map and Set keys compare by value.
const m = new Map();
m.set(10n, "ten");
m.set(2n ** 70n, "big");
console.log(m.get(10n), m.get(2n ** 70n), m.has(5n + 5n), m.get(10), m.size);
const s = new Set([1n, 1n, 2n, 1, "1"]);
console.log(s.size, s.has(2n), [...s]);

// JSON.
try {
  JSON.stringify({ a: 1n });
} catch (e) {
  console.log(e.name + ": " + e.message);
}
