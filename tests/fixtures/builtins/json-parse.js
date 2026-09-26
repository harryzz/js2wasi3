// JSON.parse (T25 slice 1): values, escapes, numbers, duplicate keys, `__proto__`, the reviver,
// and node's SyntaxError messages.

function show(text) {
  try {
    const v = JSON.parse(text);
    console.log(JSON.stringify(text), "->", JSON.stringify(v), v === 0 && 1 / v < 0 ? "(-0)" : "");
  } catch (e) {
    console.log(JSON.stringify(text), "=>", e.name + ": " + e.message);
  }
}

// Values and nesting.
const ok = [
  "1", "-1", "0", "-0", "-0.0", "0.1", "1.5e3", "1E+2", "1e-2", "1e400", "-1e400", "1e-400",
  "123456789012345", "1234567890123456", "12345678901234567890", "123456789012345678901234567890",
  "9007199254740993", "0.30000000000000004", "2.2250738585072014e-308", "5e-324",
  "true", "false", "null", '""', '"abc"', "[]", "{}", "[ ]", "{ }", " \t\r\n1\n ",
  '{ "a" : 1 , "b" : [ ] }', '{"a":{"b":[1,{"c":null}]},"d":[[],[[]],{}]}', "[1,true,null,\"x\",[2],{\"y\":3}]",
];
for (const t of ok) {
  show(t);
}

// Strings and escapes.
show('"\\/\\b\\f\\n\\r\\t\\"\\\\"');
show('"\\u00e9\\u00E9\\uD83D\\uDE00"');
show('"\\ud834\\udd1e"');
show('"\\ud800"');
show('"a\\u0000b"');
show('"é😀 plain"');
show('"x\\ny' + "z".repeat(40) + '"');
const lone = JSON.parse('"\\udc00"');
console.log(lone.length, lone.charCodeAt(0));

// Duplicate keys: the last value wins, the key keeps its first position.
show('{"a":1,"a":2,"b":3,"a":4}');
console.log(Object.keys(JSON.parse('{"b":1,"a":2,"b":3}')).join());

// `__proto__` is an own property, not the prototype.
const p = JSON.parse('{"__proto__":{"x":1},"y":2}');
console.log(Object.keys(p).join(), p.x, Object.getPrototypeOf(p) === Object.prototype);
console.log(Object.hasOwn(p, "__proto__"), JSON.stringify(p));

// ToString of the argument.
show(1);
show(true);
show(null);
show([1, 2]);
show(undefined);
show({});
try {
  JSON.parse(Symbol("s"));
} catch (e) {
  console.log(e.name);
}

// Errors.
const bad = [
  "", " ", "tru", "nul", "t", "f", "[", "{", "{a:1}", '{"a":1,}', '{"a" 1}', '{"a":1', "[1,]", "'a'",
  "trux", "tr1", 'tr"', "nul-", "fals ", "falsy", "truex", "nulll", "nan", '"abc', '"a',
  "01", "-01", "[01]", "1.", "2.", "1.e5", "[1.]", "-", "-x", "- 1", "[-", "[-]", "-Infinity",
  "1e", "1e+", "[1e]", "1e5x", "1e5.1", "0x10", "1.5.3", "[1.5.3]", ".5", "+1",
  '"\\x"', '"\\', '"\\u', '"\\u12"', '"\\u12', '"\\uZZZZ"', '"\\u00zz"',
  '"\u0001"', '"a\nb"', '"\t"', "[1] x", "[1] ", "[1]]", '{"a":1}}', '"a" "b"', "1 2", '"a"x',
  "undefined", "NaN", "NaN ", " NaN", "Infinity", "[object Object]", " undefined", "[undefined]",
  '["a" "b"]', "[1 2]", "[1", "[1x]", "[1,2 3]", "[[1] 2]", "[1,,2]", "[,]", "]", "}", ":", ",", "x",
  '{"a":1 "b":2}', '{"a" "b"}', "{1:2}", "{,}", '{"a":}', '{"a"', '{"a"x', '{"a":', '{"a":1,',
  '{"a":1x}', '{"a":1 ', '{"a":tru}', "[tru]", "[1,", '{"a":1,"b"}', '{"a":1,"b":}',
  '{"a":1,"b" 1}', '{"a":1,"b"', '{"a":1,"b":', '{"a":1, x}', '{"a":1,"b":2 "c":3}', '{"a":1,,}',
  '{"a":1,"b" "c"}', '{"a":1,"b" x}', '{"a":1,"b" t}', '{"a":1,"b" {}', '{"a":{"b":1 "c"}}',
  '{"a":{"b" 1}}', '[{"a" 1}]', '[{"a":1,"b" 1}]', '{"a":1,"b":{"c":1,"d" 1}}', '[{},{"x"}]',
  "  1", "﻿1", "\v1", '"\\uD83D" x',
  // The context shown for long inputs.
  "x".repeat(20), "x".repeat(21), "x".repeat(30),
  "[1,1,1,1,x,2,2,2]", "[1,1,1,1,x,2,2,2,2]", "[1,1,1,1,x,2,2,2,2,2]",
  "[" + "1,".repeat(10) + "x" + ",2".repeat(10) + "]",
  "[" + "1,".repeat(9) + "x" + ",2".repeat(11) + "]",
  "[" + "1,".repeat(20) + "x]",
  "[1,x" + ",2".repeat(20) + "]",
  "[1,2,3,4,5,6,7,8,9,10,]",
  "[" + "1,".repeat(14) + "tx]",
  "a".repeat(11) + "b" + "a".repeat(9),
  "[" + "1,".repeat(5) + "y" + "3".repeat(10) + "]",
  "[" + "1,".repeat(5) + "y" + "3".repeat(9) + "]",
  '{"a":1,"b":2,"c":3,"d":4,"e":5,}',
];
for (const t of bad) {
  show(t);
}

// The reviver: depth first, `this` is the holder, keys are strings, `undefined` deletes.
const calls = [];
const r = JSON.parse('{"a":[1,{"b":2}],"c":{"d":null},"e":"x"}', function (k, v) {
  calls.push(typeof k + ":" + JSON.stringify(k) + "=" + JSON.stringify(v) + " in " + (Array.isArray(this) ? "array" : typeof this));
  if (k === "e") {
    return undefined;
  }
  if (typeof v === "number") {
    return v * 10;
  }
  return v;
});
console.log(calls.join("\n"));
console.log(JSON.stringify(r), "e" in r);

// Deleting array elements leaves holes.
const holes = JSON.parse("[1,2,3,4]", (k, v) => (v === 2 || v === 4 ? undefined : v));
console.log(holes.length, 1 in holes, 3 in holes, holes[0], holes[2]);

// The root is called with "" on a wrapper; its result is the result.
console.log(JSON.parse("5", (k, v) => (k === "" ? [v, typeof k] : v)));
console.log(JSON.parse('{"a":1}', (k, v) => (k === "" ? undefined : v)));
let root;
JSON.parse('{"x":1}', function (k, v) {
  if (k === "") {
    root = this;
  }
  return v;
});
console.log(Object.keys(root).map((k) => JSON.stringify(k)).join(), JSON.stringify(root[""]));

// The reviver adds a property: it isn't visited (keys are read before the walk).
const seen = [];
JSON.parse('{"a":{"x":1},"b":2}', function (k, v) {
  seen.push(k);
  if (k === "x") {
    this.y = 3;
  }
  return v;
});
console.log(seen.join());

// A non-function reviver is ignored.
console.log(JSON.stringify(JSON.parse("[1]", 5)), JSON.stringify(JSON.parse("[1]", {})));

// Round trips with stringify.
const data = { n: -1.5e-7, s: "tab\there \"q\"   😀", a: [0, [], {}, null, true], o: { "": 1, "k k": [2] } };
const text = JSON.stringify(data);
console.log(text);
console.log(JSON.stringify(JSON.parse(text)) === text);
const pretty = JSON.stringify(data, null, 2);
console.log(JSON.stringify(JSON.parse(pretty)) === text);

// Line and column in the position form (node 24): \n, \r\n and \r end lines.
for (const t of ["[1,\n2 x]", "[1,\r\n2 x]", "[1,\r2 x]", "\n\n  {\"a\" 1}", "\n\"\u0001\"", "[1]\r\n\r\n x"]) {
  show(t);
}
