// URI functions and Annex B `escape`/`unescape` (T53).
const samples = [
  "",
  "plain-text_.!~*'()",
  "a b&c=d/e?f#g;h:i@j+k$l,m",
  "é ü ß €",
  "日本語",
  "😀 emoji \u{10FFFF}",
  "100% sure",
  "\0\x7f\x80\xff",
];
for (const s of samples) {
  const c = encodeURIComponent(s);
  const u = encodeURI(s);
  console.log(JSON.stringify(s), c, u);
  console.log(decodeURIComponent(c) === s, decodeURI(u) === s);
  const e = escape(s);
  console.log(e, unescape(e) === s);
}
console.log(decodeURI("%3B%2F%3F%3A%40%26%3D%2B%24%2C%23%41%20"));
console.log(decodeURIComponent("%3B%2F%3F%3A%40%26%3D%2B%24%2C%23%41%20"));
console.log(decodeURIComponent("%e2%82%ac%F0%9F%98%80"), decodeURI("%e2%82%AC"));
console.log(encodeURIComponent(42), encodeURI(undefined), encodeURIComponent(null), decodeURI(true));
console.log(unescape("%u20AC%41%zz%u12%4"), unescape("%"), escape("Ā@*_+-./"));

function tryIt(f, s) {
  try {
    console.log(f.name, JSON.stringify(f(s)));
  } catch (e) {
    console.log(f.name, JSON.stringify(s), e instanceof URIError, e.name, e.message);
  }
}
for (const bad of ["%", "%4", "%zz", "%C0%80", "%E0%80%80", "%ED%A0%80", "%F4%90%80%80", "%80", "%C3", "%C3%28", "%F8%80%80%80%80", "%C3%"]) {
  tryIt(decodeURIComponent, bad);
  tryIt(decodeURI, bad);
}
tryIt(encodeURIComponent, "\uD800");
tryIt(encodeURIComponent, "\uDC00x");
tryIt(encodeURI, "a\uD800b");
tryIt(encodeURI, "😀");
console.log(encodeURIComponent.name, encodeURIComponent.length, decodeURI.length, escape.length, typeof unescape);
console.log(["a b", "c/d"].map(encodeURIComponent), globalThis.encodeURI === encodeURI);
