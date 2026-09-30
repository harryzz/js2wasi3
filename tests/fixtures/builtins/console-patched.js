// A program that replaces `console` methods: every `console.x(…)` goes through the object (T53).
const lines = [];
const orig = console.log;
console.log = (...args) => {
  lines.push(args.join(" "));
  orig("[log]", ...args);
};
console.log("direct", 1);
console.log(...["spread", 2]);
[3, 4].forEach((x) => console.log(x));
console.error = function (msg) {
  orig("error went to stdout:", msg);
};
console.error("oops");
console.warn("still stderr");
console.custom = 5;
console.log(console.custom, lines.length);
delete console.log;
console.info(typeof console.log);
console.log = orig;
console.log(lines);
