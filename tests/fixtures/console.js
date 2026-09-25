// T10: a command. Top-level code, globals, console output on stdout and stderr.
// Expected output (from node): console.out (stdout), console.err (stderr).
console.log(early);
var early = "set";
console.log(early);

let count = 0;
const name = "js2wasi3";
let greeting = `hello, ${name}`;

function bump(by) {
  count = count + by;
  return count;
}

console.log(greeting);
console.log("count:", bump(2), bump(3), count);
console.log(-0, 0, 1.5, 1e21, 1 / 3, NaN, -Infinity, 9007199254740993);
console.log(true, false, null, undefined, typeof name);
console.log();
console.log("");
console.log("100%");
console.log(1, "50%", "é€😀");
console.info("info");
console.debug("debug");
console.error("error", 1);
console.warn("warn");
let r = console.log("as a value");
console.log(r);

outer: for (let i = 0; i < 3; i++) {
  for (let j = 0; j < 3; j++) {
    if (j > i) continue outer;
    if (i == 2) break outer;
    console.log(i, j);
  }
}
