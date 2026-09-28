// Case-insensitive matching (T27 slice 3): Canonicalize without `u` (toUpperCase) and simple case
// folding with `u`, sampled over the BMP.
const pats2 = [/[a-zà-ÿ]/i, /[Α-Ω]/i, /[Ā-ſ]/iu, /\w/iu, /\W/iu, /[^a-z]/i, /σ/i, /k/iu, /s/iu,
  /[Ⴀ-ჿ]/i, /[ⴀ-ⴥ]/iu, /ǅ/i, /[Ḁ-ỿ]/i, /ß/iu, /ẞ/i, /[a-z]/iu, /İ/iu, /i/iu, /[^\W]/iu, /Ꭰ/i, /ꭰ/iu];
for (const p of pats2) {
  let n = 0, h = 0;
  for (let c = 0; c < 0x10000; c += 41) {
    if (p.test(String.fromCharCode(c))) { n++; h = (h * 31 + c) % 1000003; }
  }
  for (let c = 0x10400; c < 0x10500; c++) {
    if (p.test(String.fromCodePoint(c))) { n++; h = (h * 31 + c) % 1000003; }
  }
  console.log(String(p), n, h);
}
console.log(/(ab)\1/i.test("abAB"), /(ſ)\1/iu.test("ſS"), /(ſ)\1/i.test("ſS"));
