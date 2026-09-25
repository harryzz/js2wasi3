// A Wasm trap is not a JS exception: neither `catch` nor `finally` runs.
// (`%s` with more arguments is a format string, not implemented yet: it traps.)
function fmt() {
  return "%s";
}
console.log("before");
try {
  console.log(fmt(), 1);
} catch (e) {
  console.log("caught", e);
} finally {
  console.log("finally");
}
