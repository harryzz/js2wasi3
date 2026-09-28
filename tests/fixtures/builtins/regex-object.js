// RegExp objects (T27): constructor, flags, accessors, toString, printing, errors.
const t = (f) => {
  try {
    console.log(f());
  } catch (e) {
    console.log(e.name + ": " + e.message);
  }
};
for (const [p, f] of [
  ["a(", ""], ["a)", ""], ["[b-a]", ""], ["a**", ""], ["*", ""], ["a{2,1}", ""], ["\\", ""],
  ["(?<a>x)(?<a>y)", ""], ["a", "gg"], ["a", "x"], ["[", ""], ["(?<1>a)", ""], ["\\k<a>", ""],
  ["(?<a>x)\\k<b>", ""], ["a{", ""], ["x{1}{2}", ""], ["(?", ""], ["(?=a)*", ""], ["(?<=a)*", ""],
  ["\\1(a)", ""], ["\\u{12}", "u"], ["\\c", "u"], ["a", "uv"], ["{", "u"], ["]", "u"],
  ["(?=a)*", "u"], ["(?<=a)+", ""], ["^*", ""], ["$+", ""], ["\\b*", ""], ["a{2", "u"], ["\\1", "u"],
  ["\\8", ""], ["\\08", "u"], ["\\q", "u"], ["[\\q]", "u"], ["\\k", "u"], ["\\k<a", ""],
  ["(?<a>.)\\k", ""], ["(?<a", ""], ["(?<>a)", ""], ["[\\d-z]", "u"], ["\\x1", "u"], ["\\u12", "u"],
  ["\\u{110000}", "u"], ["a{99999999999}", ""], ["(", ""], ["a|*", ""], ["[z-a]", "u"], ["\\c1", ""],
  ["[\\c1]", ""], ["\\-", "u"], ["[\\-]", "u"], ["a)", "u"], ["(?:", ""], ["(?=", ""], ["[\\1]", "u"],
  ["(?<a>x)|(?<a>y)", ""], ["((?<a>x)|(?<a>y))(?<a>z)", ""],
]) {
  t(() => new RegExp(p, f));
}
t(() => RegExp.prototype.source);
t(() => RegExp.prototype.global);
t(() => RegExp.prototype.flags);
t(() => RegExp.prototype.toString());
t(() => Object.getOwnPropertyDescriptor(RegExp.prototype, "source").get.call({}));
t(() => Object.getOwnPropertyDescriptor(RegExp.prototype, "global").get.call({}));
t(() => Object.getOwnPropertyDescriptor(RegExp.prototype, "flags").get.call(1));
t(() => RegExp.prototype.exec.call({}, "a"));
t(() => RegExp.prototype.test.call({}, "a"));
t(() => RegExp.prototype.test.call(1, "a"));
t(() => RegExp.prototype.toString.call(1));
t(() => RegExp.prototype.toString.call({ source: "x", flags: "q" }));
console.log(
  new RegExp("/", "g").source,
  new RegExp("\n").source,
  RegExp("").source,
  /[/]/.source,
  new RegExp("\\/").source,
  new RegExp("a b").source,
  new RegExp("\n\r  /").source,
  new RegExp("[/]\\/").source,
);
console.log(/a(b)?(?<n>c)/d.exec("xac"), [/a/g, { r: /x/ }], /a/gimsuyd, new RegExp("a", "v"));
const r = /a/;
r.x = 1;
console.log(r);
console.log(String(/a\/b/), Object.prototype.toString.call(/a/), typeof /a/, `${/x/g}`, /a/ + "");
console.log([/a/, /b/g].join(), new RegExp("a", "yimg").flags, new RegExp("a", "gimsuyd").flags);
console.log(Object.getOwnPropertyNames(/a/), Object.keys(/a/), Object.getOwnPropertyDescriptor(/a/, "lastIndex"));
console.log(/a/ instanceof RegExp, RegExp.prototype.exec.call(/b/, "abc"), RegExp.name, RegExp.length);
console.log(new RegExp("a+", "gi"), RegExp("x"), new RegExp(/ab/g), new RegExp(/ab/g, "i"), new RegExp("/"));
console.log(new RegExp(undefined), new RegExp(null), new RegExp(12, undefined), new RegExp("a", undefined));
console.log(/a/gimsuy.flags, /a/v.unicodeSets, /a/.hasIndices, /a/y.sticky, /a/s.dotAll, /a/m.multiline);
console.log([/a/g.global, /a/i.ignoreCase, /a/u.unicode, /a/.global]);
const src = /x/g;
const copy = new RegExp(src);
console.log(copy === src, copy.source, copy.flags, copy.lastIndex);
function f(re) {
  return re.test("abc");
}
console.log(f(/b/), f(/d/));
// A literal makes a new object at each evaluation.
const made = [];
for (let i = 0; i < 2; i++) {
  made.push(/q/g);
}
console.log(made[0] === made[1], made[0].lastIndex);
const frozen = Object.freeze(/a/g);
t(() => frozen.exec("a"));
// A user `exec` is what `test` calls.
const custom = /a/;
custom.exec = function (s) {
  console.log("custom exec", s);
  return null;
};
console.log(custom.test("a"));
custom.exec = () => 1;
t(() => custom.test("a"));
t(() => Map.prototype.get.call(/a/));
t(() => JSON.stringify({ r: /a/g }));
t(() => Object.assign({}, /a/));
t(() => ({ ...(/a/) }));
t(() => [/a/].includes(/a/));
t(() => /a/ == "/a/");
t(() => `${[/a/, /b/]}`);
t(() => Object.entries(/x/));
t(() => { for (const k in /x/) console.log(k); return "done"; });
t(() => Object.getPrototypeOf(/a/) === RegExp.prototype);
t(() => /a/.constructor === RegExp);
t(() => RegExp.prototype.constructor.name);
t(() => Reflect.ownKeys(/a/));
t(() => Object.isExtensible(/a/));
t(() => { const r = /a/g; delete r.lastIndex; return r.lastIndex; });
t(() => { const r = /a/g; r.lastIndex = 3; r.foo = 2; return [r.lastIndex, r.foo]; });
