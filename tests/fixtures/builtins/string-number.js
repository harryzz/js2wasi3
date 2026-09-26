// String → number (F20), Number(), parseInt/parseFloat, number formatting (T21 slice 2).
const strs = ["", " ", "42", " 42 ", "-7.5", "+3", "1e3", "1E-3", ".5", "5.", ".", "1_0", "0x1F", "0X1f", "0o17", "0b101",
  "-0x10", "0x", "Infinity", "-Infinity", "+Infinity", "infinity", "1e", "12px", "\n\t 8  ", "0.1", "-0",
  "9007199254740993", "1.7976931348623157e308", "1.7976931348623159e308", "2e308", "5e-324", "2.4703282292062328e-324",
  "2.4703282292062327e-324", "1e-400", "123456789012345678901234567890", "0.000001234567890123456789",
  "3.14159265358979323846264338327950288", "8.98846567431158e307", "4.9406564584124654e-324", "2.2250738585072011e-308",
  "2.2250738585072012e-308", "7.2057594037927933e16", "1.00000000000000011102230246251565404236316680908203125",
  "1.00000000000000011102230246251565404236316680908203124", "0.1e-2", "000123", "1e+2", "1e-0"];
for (const s of strs) {
  console.log(JSON_q(s), Number(s), +s, s * 1, parseFloat(s), parseInt(s), parseInt(s, 16));
}
function JSON_q(s) {
  return "'" + s + "'";
}
console.log(Number(), Number(undefined), Number(null), Number(true), Number("12") + 1, "5" * "2", "10" / "4", "7" - 2, -"3");
console.log("1" == 1, "1.0" == 1, "a" == 1, "" == 0, " \n" == 0, "0x10" == 16, 2 < "10", "2" < "10", "2" < 10);
console.log(parseInt("  -12abc"), parseInt("0x1A"), parseInt("1A", 16), parseInt("z", 36), parseInt("101", 2), parseInt("8", 8),
  parseInt("12", 1), parseInt("12", 37), parseInt("-0"), parseInt(""), parseInt("0.00000001"), parseInt(0.0000005), parseInt("123", 0));
console.log(parseFloat("3.14abc"), parseFloat("  -.5e1x"), parseFloat("e5"), parseFloat("-Infinityx"), parseFloat("1e1000"));
console.log(isNaN("abc"), isNaN("12"), isFinite("12"), isFinite("Infinity"), Number.isNaN("abc"), Number.isInteger(5.0),
  Number.isInteger(5.5), Number.isSafeInteger(2 ** 53), Number.isFinite(1 / 0), Number.parseFloat("1.5"), Number.parseInt("08"));
console.log(Number.MAX_SAFE_INTEGER, Number.MIN_SAFE_INTEGER, Number.EPSILON, Number.MAX_VALUE, Number.MIN_VALUE,
  Number.POSITIVE_INFINITY, Number.NEGATIVE_INFINITY, Number.NaN);
const nums = [0, -0, 1, -1, 255, 0.5, 0.1, -0.1, 1 / 3, 123.456, 1e21, 1.5e-7, 2 ** 53, 2 ** 60 + 1, NaN, Infinity, -Infinity, 3.14159, 1e-300, 5e-324];
for (const n of nums) {
  console.log(n.toString(), n.toString(2), n.toString(16), n.toString(36), n.toString(7));
}
for (const n of [0, 1, -1, 0.5, 1.5, 2.5, -2.5, 1.005, 1.45, 123.456, 0.000001, 1e-10, 1e20, 1e21, -1e21, 1.23e-5, 10.235, NaN, 2 ** 70, 5e-324, 0.1 + 0.2]) {
  console.log(n.toFixed(), n.toFixed(2), n.toFixed(5), n.toFixed(20));
}
console.log((255).toString(16), (3.75).toString(2), (-255).toString(36), (0.1).toFixed(30), (1234.5678).toFixed(100).length);
try {
  (1).toString(1);
} catch (e) {
  console.log(e.name, e.message);
}
try {
  (1).toFixed(101);
} catch (e) {
  console.log(e.name, e.message);
}
try {
  Number.prototype.toString.call("1");
} catch (e) {
  console.log(e.name, e.message);
}
