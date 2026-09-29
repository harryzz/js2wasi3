// RegExp modifier groups (T46): `(?ims-ims:…)`.
const t = (re, s) => re.test(s);
console.log(t(/(?i:a)b/, "Ab"), t(/(?i:a)b/, "AB"), t(/(?-i:a)b/i, "aB"), t(/(?-i:a)b/i, "AB"));
console.log(t(/(?s:.)/, "\n"), t(/(?-s:.)/s, "\n"), t(/^a(?m:$)\n/, "a\n"), t(/(?-m:^b)/m, "a\nb"));
console.log(t(/(?i:[a-c]+)d/, "ABCd"), t(/(?i:[a-c]+)d/, "ABCD"), t(/(?i:\w)/u, "\u017f"));
console.log(t(/(?i:(a)\1)/, "aA"), t(/(a)(?i:\1)/, "aA"), t(/(?i:(?-i:a)b)/, "aB"), t(/(?i:(?-i:a)b)/, "AB"));
// Backtracking out of a group restores the outer flags.
console.log(t(/(?:(?i:a)|b)+c$/, "AbAC"), t(/(?:(?i:a)|b)+c$/, "Abac"), "AbabA".match(/(?i:a)b|a/g));
console.log(/(?i:x(?=Y))./.exec("xYz")?.[0], /(?<=(?i:a))b/.test("Ab"), /(?i-ms:a.)$/.test("A\n"));
for (const src of ["(?ii:a)", "(?i-i:a)", "(?-:a)", "(?x:a)", "(?i", "(?-i-m:a)", "(?i-:a)"]) {
  try { new RegExp(src); console.log("ok", src); } catch (e) { console.log(e.message); }
}
console.log("aBc".replace(/(?i:b)/, "_"), [..."AaA".matchAll(/(?i:a)/g)].length);
