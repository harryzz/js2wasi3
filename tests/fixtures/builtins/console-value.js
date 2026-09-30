// `console` as a value and `console.log(...xs)` (T53).
const xs = [1, "two", { three: 3 }, [4]];
console.log(...xs);
console.log(...[]);
console.error(..."ab");
console.log("%s and %d", ...["fmt", 42]);
console.log("pre", ...xs, "post");

const log = console.log;
log("aliased", -0, null);
[10, 20].forEach((x) => console.info(x));
["a", "b"].forEach(console.log);
const bound = console.warn.bind(console);
bound("bound to stderr");
console.debug.call(console, "via call");
console.log.apply(null, ["via", "apply"]);

console.log(typeof console, typeof console.log, console.log.name, console.log.length);
console.log(console.log === console.log, console.error.name);
console.log("log" in console, Object.keys(console).includes("error"));
console.log("%%", "single %s arg");
console.log("%s");
const c = console;
c.log("from c", { a: [1, 2] });
function report(fn, ...args) {
  fn(...args);
}
report(console.log, "rest", "args", 3);
report(console.error, "to", "stderr");
