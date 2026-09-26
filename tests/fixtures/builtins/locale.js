// T25 slice 4: String.prototype.localeCompare (ICU root order for ASCII, 2000-pair sweep) and
// normalize (strings no form changes) (F87); methods read with a key from a string (F88).

const words = ["b", "a", "B", "A", "ab", "Ab", "aB", "a b", "a-b", "a_b", "10", "9", "a1", "", " ", "$", "~", "Zebra", "apple", "Äpfel".slice(1)];
console.log([...words].sort((x, y) => x.localeCompare(y)));
console.log("a".localeCompare("b"), "b".localeCompare("a"), "a".localeCompare("a"), "a".localeCompare("A"), "résumé".slice(0, 1).localeCompare("r"));
// A deterministic sweep over short ASCII strings.
let seed = 7;
const rnd = (n) => (seed = (seed * 1103515245 + 12345) % 2147483648) % n;
const alphabet = "aAbB09 -_.,!$~\t\u0001zZ";
const pick = () => { let s = ""; for (let k = rnd(5); k > 0; k--) s += alphabet[rnd(alphabet.length)]; return s; };
let out = "";
for (let i = 0; i < 2000; i++) out += String(pick().localeCompare(pick()) + 1);
console.log(out);
console.log("abc".normalize(), "abc".normalize("NFKD"), "é".normalize("NFC"));
try { "a".normalize("x"); } catch (e) { console.log(e.constructor.name, e.message); }

const m = ["slice", "padStart"]; const s = "hello"; console.log(s[m[0]](1, 3), s[m[1]](7, "*"), [3, 1, 2]["toSorted"]());
