// T87: localeCompare beyond ASCII and with options (exact where js2wasi3's levels match ICU's root
// collation: ASCII, Latin letters with accents, canonical equivalence; see F367 for the rest).
const words = ["resume", "résumé", "Résumé", "resumé", "Resume", "cote", "côte", "coté", "côté", "Cote", "a", "A", "á", "Á",
  "ab", "äb", "Äb", "b", "e", "é", "è", "ê", "ë", "E", "É", "z", "Z", "zebra", "Zebra", "éclair", "eclair", "naïve",
  "naive", "Ångström", "angstrom", "file10", "file2", "file1", "File2", "a b", "a-b", "a_b", "ab", "a.b", "", " ", "1",
  "10", "2", "-", "Crème", "creme", "crème brûlée", "creme brulee", "São", "Sao", "Müller", "Muller", "Mueller"];
const sorted = [...words].sort((x, y) => x.localeCompare(y));
console.log(sorted.join("|"));
for (const options of [{ sensitivity: "base" }, { sensitivity: "accent" }, { sensitivity: "case" }, { numeric: true },
  { caseFirst: "upper" }, { ignorePunctuation: true }]) {
  console.log(JSON.stringify(options), [...words].sort((x, y) => x.localeCompare(y, "en", options) || (x < y ? -1 : x > y ? 1 : 0)).join("|"));
}
for (const [x, y] of [["é", "é"], ["Å", "Å"], ["Å", "Å"], ["ộ", "ộ"],
  ["résumé", "resume"], ["a", "á"], ["á", "b"]]) {
  console.log(x.localeCompare(y), x.localeCompare(y, undefined, { sensitivity: "base" }), x.localeCompare(y, "de"));
}
console.log("a".localeCompare(), "undefined".localeCompare(), "未定义".localeCompare(undefined) === "未定义".localeCompare());
try { "a".localeCompare("b", undefined, { sensitivity: "x" }); } catch (e) { console.log(e.constructor.name, e.message); }
