// node's `querystring` module (T64).
import qs from "node:querystring";
console.log(qs.parse("a=1&b=x+y&a=2&c=%20%zz&d&=e&f=%E2%82%AC"));
console.log(qs.parse("a:1;b:2", ";", ":"), qs.parse("a=1&b=2&c=3", null, null, { maxKeys: 2 }));
console.log(qs.stringify({ a: [1, 2], b: "x y", c: true, d: null, e: undefined, f: {}, g: "é&=" }));
console.log(qs.stringify({ a: 1, b: 2 }, ";", ":"), qs.encode({ x: "~!*()'" }), qs.decode === qs.parse);
console.log(qs.escape("a b&c/é"), qs.unescape("a%20b%zz+c"), qs.unescape("%E2%82%AC"), qs.unescapeBuffer("a%41+b", true).toString());
