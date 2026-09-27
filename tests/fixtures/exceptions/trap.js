// A Wasm trap is not a JS exception: neither `catch` nor `finally` runs.
// (`String.prototype.anchor`, an Annex B method, isn't implemented: reading it traps.)
console.log("before");
try {
  console.log("".anchor);
} catch (e) {
  console.log("caught", e);
} finally {
  console.log("finally");
}
