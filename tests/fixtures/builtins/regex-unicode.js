// Unicode in regexes (T27 slice 3): \p{…}, case folding for i (with and without u), v-mode class sets,
// lastIndex inside a surrogate pair.
const t = (f) => { try { console.log(f()); } catch (e) { console.log(e.name + ": " + e.message); } };
for (const p of ["[a&&b]","[a--b]","[a&&&b]","[(]","[a-z&&b]","[a&&b--c]","[^\\q{ab}]","[\\q{ab]","[&&a]","[a&&]","[a--]","[--a]","[a||b]","[!!]","[\\z]","[\\-]","[a-]","[[a]","]","[a-\\d]","[\\d-a]","[z-a]","[\\q{a|b}]","[]","[^]","[^[^a]]","[{]","[a&b]","[\\q{}]","[a&&[b]]"]) t(() => String(new RegExp(p, "v")));
const m = (re, ...ss) => ss.map((s) => { const r = re.exec(s); return r && r[0]; });
t(() => m(/[\p{L}--[a-z]]+/v, "abcDEFé"));
t(() => m(/[\p{L}&&\p{sc=Greek}]+/v, "abcαβγ"));
t(() => m(/[[a-z]--[aeiou]]+/v, "hello"));
t(() => m(/^[\q{abc|ab|a}]/v, "abcd", "abx", "ax"));
t(() => m(/^[\q{abc|}x]+/v, "abcx", "q"));
t(() => m(/[\q{ab|c}]/vi, "AB", "c"));
t(() => m(/[^a]/vi, "A", "b"));
t(() => m(/[^a]/ui, "A", "b"));
t(() => m(/\P{Lu}/vi, "A", "a", "1"));
t(() => m(/\P{Lu}/ui, "A", "a", "1"));
t(() => m(/[\w--\d]+/v, "ab12cd"));
t(() => m(/[\W]/vi, "ſ", "K", "!"));
t(() => m(/[^\W]/vi, "ſ", "K", "!"));
t(() => m(/[[^a-c]&&[a-f]]+/v, "abcdef"));
t(() => m(/[\q{👍🏽|x}]/v, "👍🏽", "👍"));
t(() => new RegExp("[a-c--b]", "v"));
t(() => m(/[\s--\n]/v, "\n", " "));
t(() => /[\q{}]/v.exec("x"));
t(() => { const r = /./gu; r.lastIndex = 1; const m = r.exec("😀x"); return [m && m[0], m && m.index, r.lastIndex]; });
t(() => { const r = /\udc00/gu; r.lastIndex = 1; const m = r.exec("𐀀"); return [m && m.index, r.lastIndex]; });
t(() => { const r = /\udc00/g; r.lastIndex = 1; const m = r.exec("𐀀"); return [m && m.index, r.lastIndex]; });
t(() => { const r = /(?:)/yu; r.lastIndex = 1; const m = r.exec("😀"); return [m && m.index, r.lastIndex]; });
t(() => { const r = /x/yu; r.lastIndex = 1; const m = r.exec("😀x"); return [m && m.index, r.lastIndex]; });
t(() => "😀😀".split(/(?:)/u));
t(() => "😀😀".replace(/(?:)/gu, "-"));
t(() => [..."a😀b".matchAll(/(?:)/gu)].map((m) => m.index));
t(() => /^.$/u.test("\ud800"));
t(() => /\ud800/u.test("𐀀"));
t(() => /\ud800/.test("𐀀"));
t(() => /[\ud800-\udbff]/u.test("😀"));
t(() => /(?<=\ud83d)\ude00/u.test("😀"));
const count = (re) => { let n = 0, h = 0; for (let c = 0; c <= 0x10ffff; c += 1499) { if (c >= 0xd800 && c < 0xe000) continue; if (re.test(String.fromCodePoint(c))) { n++; h = (h * 31 + c) % 1000003; } } return [String(re), n, h]; };
t(() => count(/\p{L}/u)); t(() => count(/\P{L}/u)); t(() => count(/\p{Lu}/u)); t(() => count(/\p{Script=Greek}/u));
t(() => count(/\p{scx=Hira}/u)); t(() => count(/[\p{N}\p{P}]/u)); t(() => count(/[^\p{L}]/u));
t(() => count(/\p{Lu}/iu)); t(() => count(/\P{Lu}/iu));

t(() => "Ünïcödé ωορλδ 123".match(/\p{L}+/gu));
t(() => new RegExp("\\p{Script=Latin}+", "u").exec("abc ω"));
t(() => new RegExp("\\p{Foo}", "u"));
t(() => new RegExp("[\\p{Foo}]", "u"));
t(() => new RegExp("\\p{Script}", "u"));
t(() => new RegExp("\\p{L", "u"));
t(() => /\p{L}/.test("p{L}"));
// Greedy one-character loops in `u` mode backtrack over whole surrogate pairs.
{
  const astral = "a😀b😀😀c𝒳";
  console.log(/^.+😀$/u.exec("x😀😀😀"), /(?:😀)*😀/u.exec("😀😀😀")[0].length, /.*x/u.exec("😀x😀x😀"));
  console.log(/^(.*)(.)$/u.exec(astral), /^(.{2,3})(.*)$/u.exec(astral), /^(\S+?)(\S*)$/u.exec(astral));
  console.log(/^[^a]*(.)$/u.exec("😀😀𝒳"), /^(\p{L}*)(.)/u.exec("abc😀"), /^(.*)\udc00/u.exec("😀\udc00"));
  console.log(/^(.*)(\ude00)/u.exec("x😀"), /^(.*)(\ude00)/.exec("x😀"), /(k*)K/iu.exec("KkkK"), /^\P{ASCII}+$/u.test("é😀ü"));
  console.log(/^.{3}$/u.test("😀😀😀"), /^.{3}$/.test("😀"), /^\S{1,2}(\S)$/u.exec("𝒳𝒳𝒳"));
  const re = /.*/gu; re.lastIndex = 1; console.log(re.exec("😀a"), re.lastIndex);
}
