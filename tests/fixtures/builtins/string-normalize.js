// T87: String.prototype.normalize in all four forms over every code point to U+2FFFF (no data beyond) (one string, so neighbours
// compose too), and sequences for reordering, Hangul, blocked composition and the diacritics idiom.
function hash(s) {
  let h = 0;
  for (let i = 0; i < s.length; i++) h = (h * 31 + s.charCodeAt(i)) | 0;
  return h;
}
function allCodePoints() {
  let all = "";
  for (let c = 0; c <= 0x2ffff; c++) {
    if (c < 0xd800 || c > 0xdfff) all += String.fromCodePoint(c);
  }
  return all;
}
const all = allCodePoints();
for (const f of ["NFC", "NFD", "NFKC", "NFKD"]) {
  const r = all.normalize(f);
  console.log(f, r.length, hash(r));
}
const samples = [
  "é", "é", "ẛ̣", "ḍ̇", "q̣̇", "Å", "Å", "Ω",
  "한국어", "각", "가", "각", "ﬁ", "①", "x²", "Ą̊́",
  "ą́̂", "̈́", "ཱི", "\uD800é", "Crème brûlée à São Paulo", "\u{1D15E}", "\u{2F800}",
];
for (const s of samples) {
  console.log(JSON.stringify([s.normalize(), s.normalize("NFD"), s.normalize("NFKC"), s.normalize("NFKD")]));
}
console.log("Crème brûlée".normalize("NFD").replace(/[̀-ͯ]/g, ""));
try { "a".normalize("NFX"); } catch (e) { console.log(e.constructor.name, e.message); }
