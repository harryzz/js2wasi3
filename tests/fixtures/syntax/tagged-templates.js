// Tagged templates and String.raw (T51).
function tag(strings, ...values) {
  return JSON.stringify({ strings, raw: strings.raw, values });
}
console.log(tag`a${1}b${"two"}c`);
console.log(tag``, tag`${1}`, tag`x`);

// Invalid escapes: cooked is undefined, raw keeps the text.
const inv = (s) => [s[0], s.raw[0]];
console.log(inv`\unicode and \u{55}`, inv`\xz`, inv`\01`);
console.log(inv`\u{1F600}`);

// Raw line terminators are normalized.
console.log(JSON.stringify(((s) => s.raw[0])`a
b
c`));

// The same object per site, a new one per site.
const objs = [];
for (let i = 0; i < 3; i++) objs.push(((s) => s)`same`);
const other = ((s) => s)`same`;
console.log(objs[0] === objs[1], objs[1] === objs[2], objs[0] === other);

// Frozen, raw not enumerable.
const t = objs[0];
console.log(Object.isFrozen(t), Object.isFrozen(t.raw), Object.keys(t), Array.isArray(t.raw));
const d = Object.getOwnPropertyDescriptor(t, "raw");
console.log(d.writable, d.enumerable, d.configurable);
try {
  t[0] = "x";
} catch (e) {
  console.log(e.constructor.name);
}

// this from a member tag; a tag returning a tag; new.
const o = {
  name: "o",
  m(s, v) {
    return this.name + s.join("_") + v;
  },
};
console.log(o.m`x${1}y`, o["m"]`p${2}q`);
const curry = (a) => (b) => a[0] + b[0];
console.log(curry`first``second`);
// `new tag`x`` is `new (tag`x`)`.
function mk(s) {
  return function () {
    this.s = s[0];
  };
}
console.log(new mk`built`().s, new mk`bare`().s);
try {
  new mk`x`.s();
} catch (e) {
  console.log(e.message);
}

// Order: the tag, then the substitutions.
const log = [];
const getTag = () => (log.push("tag"), (s, ...v) => v);
console.log(getTag()`${log.push("a")}${log.push("b")}`, log);

// String.raw.
console.log(String.raw`C:\Users\${"me"}\n`, String.raw`\u{`);
console.log(String.raw({ raw: ["x", "y", "z"] }, 1, 2, 3), String.raw({ raw: "abc" }, "-", "+"));
console.log(String.raw({ raw: [] }), String.raw({ raw: { length: 1, 0: "only" } }, 9), String.raw.length);
try {
  String.raw();
} catch (e) {
  console.log(e.constructor.name, e.message);
}
try {
  o.nope`x`;
} catch (e) {
  console.log(e.message);
}
