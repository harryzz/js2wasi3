// String.prototype methods (T21 slice 2).
const s = "Hello, World";
console.log(s.at(0), s.at(-1), s.at(99), s.charAt(4), s.charAt(-1) === "", s.charCodeAt(1), s.charCodeAt(99));
console.log(s.indexOf("o"), s.indexOf("o", 5), s.indexOf(""), s.indexOf("", 99), s.indexOf("zz"), s.lastIndexOf("o"), s.lastIndexOf("o", 5), s.lastIndexOf(""));
console.log(s.includes("World"), s.includes("world"), s.startsWith("Hell"), s.startsWith("World", 7), s.endsWith("World"), s.endsWith("Hello", 5));
console.log(s.slice(7), s.slice(-5), s.slice(3, -3), s.slice(5, 2), s.slice(), s.slice(-100, 3));
console.log(s.substring(7), s.substring(5, 2), s.substring(-3, 3), s.substring(3, NaN), s.substr(-5, 3), s.substr(2));
console.log("ab".repeat(3), "x".repeat(0), "".repeat(5), "-".padStart(3), "5".padStart(3, "0"), "abc".padEnd(8, "12"), "abc".padStart(2), "a".padEnd(4, ""));
console.log("[" + "  \t hi \n ".trim() + "]", "[" + "  hi ".trimStart() + "]", "[" + "  hi  ".trimEnd() + "]", "[" + " ﻿x ".trim() + "]");
console.log("MiXeD 123".toUpperCase(), "MiXeD 123".toLowerCase(), "straße".toUpperCase(), "ÀÉÎõü".toLowerCase(), "ÿ".toUpperCase());
console.log("Ωμέγα".toUpperCase(), "ПРИВЕТ".toLowerCase(), "日本語".toUpperCase());
console.log("a,b,,c".split(","), "abc".split(""), "a b c".split(" ", 2), "abc".split(), "".split(","), "".split(""), "a--b--c".split("--"));
console.log("aaa".replace("a", "b"), "aaa".replaceAll("a", "b"), "abc".replace("b", "[$&|$`|$'|$$|$1]"), "xyz".replace("q", "!"));
console.log("a-b-c".replace("-", (m, i, str) => "<" + m + i + str.length + ">"), "a-b-c".replaceAll("-", () => "+"), "abc".replaceAll("", "."));
console.log("a".concat("b", 1, null), "x".concat(), "😀".codePointAt(0), "😀".codePointAt(1), "abc".codePointAt(5));
console.log("ab\ud800".isWellFormed(), "ab".isWellFormed(), "a\udc00b".toWellFormed() === "a�b");
console.log(String(12), String(null), String(), String(Symbol("x")), String([1, 2]), String({}), String(-0));
console.log(String.fromCharCode(72, 105, 0x10041), String.fromCodePoint(0x1f600, 65));
{
  const cps = [];
  for (let c = 0; c < 3000; c++) cps.push(c % 3 === 0 ? 0x1f600 + c : 0x41 + (c % 26));
  const s = String.fromCodePoint.apply(null, cps);
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) % 1000003;
  console.log(s.length, h, s.codePointAt(0), s.codePointAt(2), String.fromCodePoint(), String.fromCodePoint({ valueOf() { return 0x10ffff; } }).length);
  let seen = 0;
  try { String.fromCodePoint(65, { valueOf() { seen++; return 66; } }, 1.5, { valueOf() { seen++; return 67; } }); } catch (e) { console.log(e.name, e.message, seen); }
  try { String.fromCodePoint(0x110000); } catch (e) { console.log(e.message); }
}
console.log("abc".toString(), "abc".valueOf(), String.prototype.slice.call(12345, 1, 3), "b" < "a", "a" < "b", "abc" < "abd", "ab" < "abc", "B" >= "a", "10" < "9");
const words = ["pear", "Apple", "fig"];
let out = [];
for (const w of words) {
  out.push(w.toLowerCase().padEnd(6, ".") + w.length);
}
console.log(out.join("|"));
try {
  "x".repeat(-1);
} catch (e) {
  console.log(e.name, e.message);
}
try {
  String.prototype.trim.call(null);
} catch (e) {
  console.log(e.name, e.message);
}
try {
  String.prototype.toString.call(1);
} catch (e) {
  console.log(e.name, e.message);
}
