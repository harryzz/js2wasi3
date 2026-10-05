// T87: toUpperCase / toLowerCase over every code point to U+2FFFF (no case data beyond) (node's full, language-insensitive mapping), the
// multi-character mappings, Final_Sigma and lone surrogates.
function hash(s) {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return h;
}
// (Built in a function: a top-level `+=` loop is quadratic, F365.)
function allCodePoints() {
  let all = "";
  for (let c = 0; c <= 0x2ffff; c++) {
    if (c < 0xd800 || c > 0xdfff) all += String.fromCodePoint(c);
  }
  return all;
}
const all = allCodePoints();
const up = all.toUpperCase();
const lo = all.toLowerCase();
console.log(all.length, up.length, lo.length, hash(up), hash(lo));
console.log(all.toLocaleUpperCase() === up, all.toLocaleLowerCase() === lo);
// One by one where they differ from the bulk result's per-character concatenation (context: Σ).
function perChar() {
  let per = "";
  for (const ch of all) per += ch.toLowerCase();
  return per;
}
const per = perChar();
console.log(per === lo, per.length);
for (const s of ["ß", "ŉ", "İ", "ﬀ", "ﬃ", "ΐ", "ǰ", "ᾳ", "ﬓ", "ș", "Ǆ", "ǅ", "Ա", "ა", "Ⴀ", "𐐀", "𞤀", "\u{16E40}"]) {
  console.log(s, s.toUpperCase(), s.toLowerCase(), s.toUpperCase().length);
}
for (const s of ["Σ", "ΑΣ", "ΑΣ Β", "ΑΣΑ", "Α᠎Σ", "Α᠎Σ᠎Β", "Α.Σ", "Α'Σ'", "𐐀Σ", "Σ𐐀", "ΑΣ\uD800", "\uDC00Σ"]) {
  console.log(JSON.stringify(s.toLowerCase()));
}
console.log(JSON.stringify("a\uD800b\uDC00c".toUpperCase()), JSON.stringify("\uDBFF".toLowerCase()));
