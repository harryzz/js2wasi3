// A Wasm trap is not a JS exception: neither `catch` nor `finally` runs.
// (A `Number` wrapper object isn't implemented (F106): `new Number` traps.)
console.log("before");
try {
  console.log(new Number(1));
} catch (e) {
  console.log("caught", e);
} finally {
  console.log("finally");
}
