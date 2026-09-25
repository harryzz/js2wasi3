// Objects as values: typeof, truthiness, ToString / ToPrimitive, equality, ToNumber.
const o = { a: 1 };
const p = o;
console.log(typeof o, typeof Object.create(null), !o, !!{}, o ? "truthy" : "falsy");
console.log("" + o, `${o}`, o + 1, 1 + o, o + o);
console.log(o === p, o === { a: 1 }, o == p, o == "[object Object]", o != "x", o == null, o == undefined);
console.log(o * 2, +o, o - o, o < 1, o > 0);
const e = new RangeError("r");
console.log("" + e, e + "!", e == "RangeError: r", e * 1);
let count = 0;
if (o) count++;
while ({}) {
  count++;
  break;
}
console.log(count, o && "and", null || o ? "or" : "no");
console.log((o ?? "default") === o, undefined ?? o === o);
